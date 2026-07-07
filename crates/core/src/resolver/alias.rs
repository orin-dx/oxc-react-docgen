//! Type alias chain and type-level resolution.

use std::collections::BTreeMap;

use camino::Utf8Path;
use rustc_hash::FxHashSet;

use crate::types::*;

use super::chain::resolve_props_chain;
use super::collected::resolve_collected_type;
use super::{ResolutionContext, ResolvedChain};

/// Resolve a `CollectedTypeAlias` as a props chain.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_type_alias_chain(
    alias: &CollectedTypeAlias,
    _consuming_file: &Utf8Path,
    mapping: &ComponentMapping,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> ResolvedChain {
    match alias {
        // Delegate to the generic CollectedType→chain resolver, which already
        // handles Named (interfaces/type aliases), Object (inline literals),
        // Intersection, and Union targets. Previously this arm only matched
        // `Named` and silently dropped everything else (e.g. an inline
        // `{ x: string }` object literal used directly as props), which lost
        // the whole component's props with no diagnostic.
        CollectedTypeAlias::Passthrough { target, file_path } => {
            resolve_base_as_chain(target, file_path, mapping, ctx, state, depth)
        }

        CollectedTypeAlias::Omit { base, omitted_keys, file_path } => {
            // Resolve the base type first, then remove omitted keys.
            let mut chain = resolve_base_as_chain(base, file_path, mapping, ctx, state, depth);
            let omitted_set: FxHashSet<&str> = omitted_keys.iter().map(|k| k.as_str()).collect();
            chain.props.retain(|p| !omitted_set.contains(p.name.as_str()));
            // Record omitted keys in the relevant inheritance layer.
            for layer in &mut chain.inheritance {
                for key in omitted_keys {
                    if !layer.omitted.contains(key) {
                        layer.omitted.push(key.clone());
                    }
                }
            }
            chain
        }

        CollectedTypeAlias::Pick { base, picked_keys, file_path } => {
            let mut chain = resolve_base_as_chain(base, file_path, mapping, ctx, state, depth);
            let picked_set: FxHashSet<&str> = picked_keys.iter().map(|k| k.as_str()).collect();
            chain.props.retain(|p| picked_set.contains(p.name.as_str()));
            chain
        }

        CollectedTypeAlias::Partial { base, file_path } => {
            let mut chain = resolve_base_as_chain(base, file_path, mapping, ctx, state, depth);
            // Make all props optional.
            for prop in &mut chain.props {
                prop.required = false;
            }
            chain
        }

        CollectedTypeAlias::Required { base, file_path } => {
            let mut chain = resolve_base_as_chain(base, file_path, mapping, ctx, state, depth);
            // Make all props required.
            for prop in &mut chain.props {
                prop.required = true;
            }
            chain
        }

        CollectedTypeAlias::Union { members, file_path } => {
            // Discriminated union — try to find the discriminant and merge all members.
            resolve_union_alias(members, file_path, mapping, ctx, state, depth)
        }

        CollectedTypeAlias::Intersection { members, file_path } => {
            // Merge all members' props.
            let mut chain = ResolvedChain::default();
            for member in members {
                let member_chain = resolve_base_as_chain(member, file_path, mapping, ctx, state, depth);
                chain.merge_parent(member_chain);
            }
            chain
        }

        CollectedTypeAlias::LiteralUnion { .. } => {
            // Pure string union used as a type alias — not a props provider.
            ResolvedChain::default()
        }
    }
}

/// Resolve a `CollectedType` as a props chain (for base types in aliases).
pub(super) fn resolve_base_as_chain(
    base: &CollectedType,
    file_path: &Utf8Path,
    mapping: &ComponentMapping,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> ResolvedChain {
    match base {
        CollectedType::Named { name, args } => {
            let raw_args: Vec<String> = args.iter().map(|a| a.to_raw_string()).collect();
            resolve_props_chain(name.as_str(), &raw_args, file_path, mapping, ctx, state, depth + 1)
        }
        CollectedType::Intersection(members) => {
            let mut chain = ResolvedChain::default();
            for member in members {
                let sub = resolve_base_as_chain(member, file_path, mapping, ctx, state, depth);
                chain.merge_parent(sub);
            }
            chain
        }
        CollectedType::Object(fields) => {
            // Inline object type in an intersection: `ComponentPropsWithoutRef<'button'> & { asChild?: boolean }`
            // Expand the object fields directly as own props.
            let mut chain = ResolvedChain::default();
            for field in fields {
                let prop_type = resolve_collected_type(&field.collected_type, file_path, ctx, state, depth);
                chain.props.push(ParsedProp {
                    name: field.name.clone(),
                    prop_type,
                    required: field.required,
                    default_value: None,
                    description: field.description.clone(),
                    tags: Default::default(),
                    parent: None,
                    declarations: vec![],
                });
            }
            chain
        }
        CollectedType::Union(members) => {
            // Union base: merge all members together.
            // This handles `Omit<A | B, K>` and similar patterns.
            let mut chain = ResolvedChain::default();
            for member in members {
                let sub = resolve_base_as_chain(member, file_path, mapping, ctx, state, depth);
                chain.merge_parent(sub);
            }
            chain
        }
        _ => ResolvedChain::default(),
    }
}

/// Resolve a `CollectedTypeAlias::Union` — detect discriminated union if possible.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_union_alias(
    members: &[CollectedType],
    file_path: &Utf8Path,
    mapping: &ComponentMapping,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> ResolvedChain {
    use super::chain::find_discriminant_prop;

    // Collect all member chains for named types (only Named can be discriminated).
    let named_members: Vec<(&str, Vec<ParsedProp>)> = members
        .iter()
        .filter_map(|m| {
            if let CollectedType::Named { name, .. } = m {
                let mut branch_state = ResolveState { visited: state.visited.clone(), diagnostics: vec![] };
                let chain =
                    resolve_props_chain(name.as_str(), &[], file_path, mapping, ctx, &mut branch_state, depth + 1);
                state.diagnostics.extend(branch_state.diagnostics);
                Some((name.as_str(), chain.props))
            } else {
                None
            }
        })
        .collect();

    if named_members.len() < 2 {
        // Not a discriminated union — just merge all.
        let mut chain = ResolvedChain::default();
        for member in members {
            let sub = resolve_base_as_chain(member, file_path, mapping, ctx, state, depth);
            chain.merge_parent(sub);
        }
        return chain;
    }

    // Try to find a discriminant prop (a prop whose type is a distinct string literal across all members).
    let discriminant = find_discriminant_prop(&named_members);

    // Merge all props from all members. A prop's merged type is the union of every
    // distinct type it has across the members that declare it (deduped — two members
    // contributing the identical type must not produce a redundant single-member
    // union); a prop missing from at least one member becomes optional overall, even
    // if it was required in every member where it does appear, since the union type
    // doesn't guarantee its presence.
    let mut merged_props: BTreeMap<String, ParsedProp> = BTreeMap::new();
    let mut prop_types: BTreeMap<String, Vec<PropType>> = BTreeMap::new();
    let mut seen_count: BTreeMap<String, usize> = BTreeMap::new();
    let mut required_in_all: BTreeMap<String, bool> = BTreeMap::new();
    let mut total_variants: usize = 0;

    for (_, member_props) in &named_members {
        total_variants += 1;
        record_variant_props(member_props, &mut merged_props, &mut prop_types, &mut seen_count, &mut required_in_all);
    }

    // Also merge non-Named members (inline objects, intersections, nested unions, etc.)
    // that were excluded from the discriminant analysis above.
    for member in members {
        if !matches!(member, CollectedType::Named { .. }) {
            let sub_chain = resolve_base_as_chain(member, file_path, mapping, ctx, state, depth);
            total_variants += 1;
            record_variant_props(
                &sub_chain.props,
                &mut merged_props,
                &mut prop_types,
                &mut seen_count,
                &mut required_in_all,
            );
        }
    }

    for (name, mut types) in prop_types {
        let prop_type = match types.len() {
            0 | 1 => match types.pop() {
                Some(t) => t,
                None => continue,
            },
            _ => PropType::Union(types),
        };

        if let Some(entry) = merged_props.get_mut(&name) {
            entry.prop_type = prop_type;
            let present_everywhere = seen_count.get(&name).copied().unwrap_or(0) == total_variants;
            let required_everywhere = required_in_all.get(&name).copied().unwrap_or(false);
            entry.required = present_everywhere && required_everywhere;
        }
    }

    // If we found a discriminant, merge its type as a union of all its literal values.
    if let Some(ref disc_name) = discriminant {
        let disc_literals: Vec<PropType> = named_members
            .iter()
            .filter_map(|(_, props)| props.iter().find(|p| &p.name == disc_name).map(|p| p.prop_type.clone()))
            .collect();

        if let Some(disc_prop) = merged_props.get_mut(disc_name) {
            disc_prop.prop_type = PropType::Union(disc_literals);
        }
    }

    // Emit discriminated union diagnostic if applicable.
    if discriminant.is_some() {
        state.diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Info,
            message: format!(
                "Discriminated union detected with discriminant prop '{}'",
                discriminant.as_deref().unwrap_or("")
            ),
            file: Some(file_path.to_string()),
            line: None,
            column: None,
            help: None,
            code: DiagnosticCode::DiscriminatedUnion,
        });
    }

    ResolvedChain { props: merged_props.into_values().collect(), discriminant_prop: discriminant, ..Default::default() }
}

/// Record one union member's props into the shared merge accumulators: the
/// first-seen `ParsedProp` (for name/description/parent/etc.), the set of
/// distinct types contributed for each prop name, how many members declared
/// it, and whether every member that declared it marked it required.
fn record_variant_props(
    props: &[ParsedProp],
    merged_props: &mut BTreeMap<String, ParsedProp>,
    prop_types: &mut BTreeMap<String, Vec<PropType>>,
    seen_count: &mut BTreeMap<String, usize>,
    required_in_all: &mut BTreeMap<String, bool>,
) {
    for prop in props {
        merged_props.entry(prop.name.clone()).or_insert_with(|| prop.clone());

        let types = prop_types.entry(prop.name.clone()).or_default();
        if !types.contains(&prop.prop_type) {
            types.push(prop.prop_type.clone());
        }

        *seen_count.entry(prop.name.clone()).or_insert(0) += 1;

        required_in_all
            .entry(prop.name.clone())
            .and_modify(|required| *required = *required && prop.required)
            .or_insert(prop.required);
    }
}

/// Resolve a `CollectedTypeAlias` to a `PropType` (at the type level, not chain level).
pub(super) fn resolve_type_alias_type(
    alias: &CollectedTypeAlias,
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> PropType {
    match alias {
        CollectedTypeAlias::LiteralUnion { members, .. } => {
            PropType::LiteralUnion { members: members.clone(), has_default: false }
        }
        CollectedTypeAlias::Passthrough { target, .. } => {
            resolve_collected_type(target, consuming_file, ctx, state, depth + 1)
        }
        CollectedTypeAlias::Union { members, .. } => {
            let resolved: Vec<PropType> =
                members.iter().map(|m| resolve_collected_type(m, consuming_file, ctx, state, depth + 1)).collect();
            PropType::Union(resolved)
        }
        CollectedTypeAlias::Intersection { members, .. } => {
            let resolved: Vec<PropType> =
                members.iter().map(|m| resolve_collected_type(m, consuming_file, ctx, state, depth + 1)).collect();
            PropType::Intersection(resolved)
        }
        CollectedTypeAlias::Partial { base, .. } | CollectedTypeAlias::Required { base, .. } => {
            resolve_collected_type(base, consuming_file, ctx, state, depth + 1)
        }
        CollectedTypeAlias::Omit { base, .. } | CollectedTypeAlias::Pick { base, .. } => {
            // At the type level, just forward to the base type.
            resolve_collected_type(base, consuming_file, ctx, state, depth + 1)
        }
    }
}
