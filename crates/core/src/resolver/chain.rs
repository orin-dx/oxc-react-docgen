//! Props chain resolution — the recursive core of prop resolution.

use camino::Utf8Path;
use compact_str::CompactString;

use crate::known::{push_known_opaque_diagnostic, resolve_known, KnownPatternResult};
use crate::react_types;
use crate::types::*;

use super::alias::resolve_type_alias_chain;
use super::collected::resolve_collected_type;
use super::extends::resolve_extends_ref;
use super::import::{lookup_interface, lookup_type_alias, resolve_to_canonical};
use super::react::resolve_react_types_file;
use super::{ResolutionContext, ResolvedChain, MAX_DEPTH};

/// Resolve a named type to a chain of props.
/// This is the recursive core — handles interfaces, aliases, known patterns, etc.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_props_chain(
    type_name: &str,
    type_args: &[String],
    consuming_file: &Utf8Path,
    mapping: &ComponentMapping,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> ResolvedChain {
    if depth > MAX_DEPTH {
        state.diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Warning,
            message: format!("Max resolution depth exceeded for type '{}'", type_name),
            file: Some(consuming_file.to_string()),
            line: None,
            column: None,
            help: Some("This may indicate a circular type reference".into()),
            code: DiagnosticCode::MaxDepthExceeded,
        });
        return ResolvedChain::empty_with_compose(type_name.to_owned());
    }

    // Cycle detection — scoped by file so same type name in different files is OK.
    // Must include type_args: distinct instantiations of the same generic alias
    // (e.g. `Partial<RowHeights>` vs `Partial<ColumnWidths>` in the same interface's
    // extends list) are not the same visit and must not collide on the bare name alone.
    let visit_key: CompactString = format!("{}:{}<{}>", consuming_file, type_name, type_args.join(",")).into();
    if !state.visited.insert(visit_key) {
        return ResolvedChain::default();
    }

    // Strip "React." namespace prefix before all builtin/utility checks.
    let type_name_bare = type_name.strip_prefix("React.").unwrap_or(type_name);

    // ── Step 0.5: Inline utility type in extends position ─────────────────────
    // Pick/Omit/Partial/Readonly appearing directly in `extends Pick<T,K>`
    // have non-empty type_args. Route through alias resolver (same logic as
    // `type X = Pick<T,K>`) instead of the step-1 silent no-op.
    if !type_args.is_empty() {
        let synthetic = match type_name_bare {
            // Guard: skip if the base type is itself generic (contains '<').
            // Raw string type_args can't represent nested generics reliably.
            "Pick" if type_args.len() >= 2 && !type_args[0].contains('<') => {
                let base = CollectedType::Named { name: type_args[0].as_str().into(), args: vec![] };
                Some(CollectedTypeAlias::Pick {
                    base,
                    picked_keys: parse_string_union_keys(&type_args[1]),
                    file_path: consuming_file.to_owned(),
                })
            }
            "Omit" if type_args.len() >= 2 && !type_args[0].contains('<') => {
                let base = CollectedType::Named { name: type_args[0].as_str().into(), args: vec![] };
                Some(CollectedTypeAlias::Omit {
                    base,
                    omitted_keys: parse_string_union_keys(&type_args[1]),
                    omitted_keys_of: None,
                    file_path: consuming_file.to_owned(),
                })
            }
            "Partial" if !type_args.is_empty() && !type_args[0].contains('<') => {
                let base = CollectedType::Named { name: type_args[0].as_str().into(), args: vec![] };
                Some(CollectedTypeAlias::Partial { base, file_path: consuming_file.to_owned() })
            }
            "Readonly" if !type_args.is_empty() && !type_args[0].contains('<') => {
                let base = CollectedType::Named { name: type_args[0].as_str().into(), args: vec![] };
                Some(CollectedTypeAlias::Passthrough { target: base, file_path: consuming_file.to_owned() })
            }
            _ => None,
        };
        if let Some(alias) = synthetic {
            return resolve_type_alias_chain(&alias, consuming_file, mapping, ctx, state, depth);
        }
    }

    // ── Step 1: TypeScript built-in utility types — silent no-op ─────────────
    // Not prop providers; suppress false "unresolvable" warnings.
    if matches!(
        type_name_bare,
        "Omit"
            | "Pick"
            | "Partial"
            | "Required"
            | "Readonly"
            | "NonNullable"
            | "ReturnType"
            | "Parameters"
            | "Awaited"
            | "Extract"
            | "Exclude"
            | "Record"
            | "ReadonlyArray"
            | "Array"
            | "Promise"
    ) {
        return ResolvedChain::default();
    }

    // ── Step 2: Known pattern check (SxProps, VariantProps, ComponentProps…) ─
    // Run BEFORE is_react_builtin: some builtins (ComponentPropsWithoutRef,
    // PropsWithChildren, etc.) expand into props via known.rs and must NOT be
    // short-circuited as terminal types. resolve_known with the bare name handles
    // both "ComponentPropsWithoutRef" and "React.ComponentPropsWithoutRef".
    {
        let resolved_args: Vec<PropType> = type_args
            .iter()
            .map(|a| {
                let ct = CollectedType::Raw(a.clone());
                resolve_collected_type(&ct, consuming_file, ctx, state, depth + 1)
            })
            .collect();

        if let Some(result) = resolve_known(type_name_bare, &resolved_args, &ctx.global, &ctx.enum_bare_index) {
            return match result {
                KnownPatternResult::Props(props) => ResolvedChain { props, ..Default::default() },
                KnownPatternResult::Type(PropType::HtmlAttributes { element, omitted }) => {
                    // HtmlAttributes from ComponentPropsWithoutRef<'button'> or
                    // HTMLChakraProps<'button'> — record as InheritedLayer so
                    // notable_inherited can be synthesized from the element's attr table.
                    let layer = InheritedLayer {
                        type_name: type_name.to_owned(),
                        file_name: resolve_react_types_file(consuming_file, ctx),
                        omitted,
                        html_element: Some(element),
                        total_props: 0,
                    };
                    ResolvedChain { inheritance: vec![layer], ..Default::default() }
                }
                KnownPatternResult::Type(pt) => {
                    if let PropType::Opaque { reason, .. } = &pt {
                        push_known_opaque_diagnostic(&mut state.diagnostics, reason, type_name_bare, consuming_file);
                    }
                    ResolvedChain::empty_with_compose(pt.raw_string())
                }
                KnownPatternResult::Alias { name } => {
                    resolve_props_chain(&name, &[], consuming_file, mapping, ctx, state, depth + 1)
                }
            };
        }
    }

    // ── Step 2.5: React builtin check (after known patterns) ─────────────────
    // Terminal React types (ReactNode, Ref, FC, etc.) that survived the known-pattern
    // check are not prop providers — add to composes and stop.
    if react_types::is_react_builtin(type_name_bare, &ctx.extra_builtins) {
        return ResolvedChain::empty_with_compose(type_name.to_owned());
    }

    // ── Step 3: Resolve import to canonical (file, name) ─────────────────────
    let (canonical_file, canonical_name) = resolve_to_canonical(type_name, consuming_file, ctx, &mut state.diagnostics)
        .unwrap_or_else(|| (consuming_file.to_owned(), type_name.to_owned()));

    // ── Step 4: Type alias (Omit, Pick, Partial, Union, etc.) ────────────────
    if let Some((matched_key, alias)) = lookup_type_alias(&ctx.global, canonical_file.as_str(), &canonical_name) {
        let alias = alias.clone();
        // Substitute declared type parameters (`type Assign<T, U> = ...`) with the
        // call site's concrete `type_args` before resolving the body. No-op for
        // non-generic aliases (the common case) — see resolver/substitute.rs.
        let alias = super::substitute::apply_generic_args(alias, &matched_key, type_args, consuming_file, ctx);
        return resolve_type_alias_chain(&alias, consuming_file, mapping, ctx, state, depth);
    }

    // ── Step 5: Interface ─────────────────────────────────────────────────────
    if let Some(iface) = lookup_interface(&ctx.global, canonical_file.as_str(), &canonical_name) {
        return resolve_interface_chain(iface, type_args, consuming_file, mapping, ctx, state, depth);
    }

    // ── Step 6: Unresolvable ──────────────────────────────────────────────────
    let scoped_key = format!("{}:{}", canonical_file, canonical_name);
    state.diagnostics.push(Diagnostic {
        severity: DiagnosticSeverity::Warning,
        message: format!("Cannot resolve type '{}' in '{}' (scoped key: '{}')", type_name, consuming_file, scoped_key),
        file: Some(consuming_file.to_string()),
        line: None,
        column: None,
        help: Some(
            "Type may be in an unresolvable cross-package location. Check that the package is installed.".into(),
        ),
        code: DiagnosticCode::UnresolvableImport,
    });
    ResolvedChain::empty_with_compose(type_name.to_owned())
}

/// Resolve an interface to a chain of props.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_interface_chain(
    iface: &CollectedInterface,
    _type_args: &[String],
    _consuming_file: &Utf8Path,
    mapping: &ComponentMapping,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> ResolvedChain {
    let mut chain = ResolvedChain::default();

    // A generic interface's own declared type parameters (`interface Foo<TData>`)
    // are expected, unexpandable placeholders wherever referenced in its body —
    // not unresolvable types. Register them so `resolve_named` doesn't warn.
    if let Some(params) = ctx.global.interface_type_params.get(&iface.scoped_key) {
        state.in_scope_type_params.extend(params.iter().cloned());
    }

    // ── Resolve extends first — parent props come before own props ────────────
    for extends_ref in &iface.extends {
        let (parent_chain, maybe_layer) =
            resolve_extends_ref(extends_ref, &iface.file_path, mapping, ctx, state, depth + 1);
        if let Some(layer) = maybe_layer {
            chain.inheritance.push(layer);
        }
        chain.merge_parent(parent_chain);
    }

    // ── Resolve own props ────────────────────────────────────────────────────
    let parent = PropParent { name: iface.name.to_string(), file_name: iface.file_path.to_string() };

    for raw_prop in &iface.props {
        let prop_type = resolve_collected_type(&raw_prop.collected_type, &iface.file_path, ctx, state, depth);

        // Default value: code default takes precedence over JSDoc @default.
        let code_default = mapping.param_defaults.get(&raw_prop.name);
        let jsdoc_default =
            raw_prop.tags.get("default").or_else(|| raw_prop.tags.get("defaultValue")).map(|s| s.trim());

        let default_value = match (code_default, jsdoc_default) {
            (Some(code), Some(jsdoc))
                if code.value.trim_matches('"').trim_matches('\'') != jsdoc.trim_matches('"').trim_matches('\'') =>
            {
                state.diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Info,
                    message: format!(
                        "JSDoc @default '{}' differs from code default '{}' for prop '{}' — using code value",
                        jsdoc, code.value, raw_prop.name
                    ),
                    file: Some(iface.file_path.to_string()),
                    line: None,
                    column: None,
                    help: Some("Update the JSDoc @default to match the code default.".into()),
                    code: DiagnosticCode::JsDocDefaultMismatch,
                });
                Some(DefaultValue { value: code.value.clone(), computed: code.computed })
            }
            (Some(code), _) => Some(DefaultValue { value: code.value.clone(), computed: code.computed }),
            (None, Some(jsdoc)) => Some(DefaultValue { value: jsdoc.to_owned(), computed: false }),
            (None, None) => None,
        };

        chain.props.push(ParsedProp {
            name: raw_prop.name.clone(),
            prop_type,
            required: raw_prop.required,
            default_value,
            description: raw_prop.description.clone(),
            tags: raw_prop.tags.clone(),
            parent: Some(parent.clone()),
            declarations: vec![parent.clone()],
        });
    }

    chain
}

/// Find a discriminant prop — a prop that has a distinct string literal type across all members.
pub(super) fn find_discriminant_prop(members: &[(&str, Vec<ParsedProp>)]) -> Option<String> {
    if members.is_empty() {
        return None;
    }

    // Collect candidate prop names: appear in all members AND have PropType::StringLiteral in at least one.
    // Use the intersection of all member prop names as candidates.
    let mut candidate_names: Option<std::collections::BTreeSet<&str>> = None;
    for (_, member_props) in members {
        let names: std::collections::BTreeSet<&str> = member_props.iter().map(|p| p.name.as_str()).collect();
        candidate_names = Some(match candidate_names {
            None => names,
            Some(existing) => existing.intersection(&names).copied().collect(),
        });
    }
    let candidate_names = candidate_names.unwrap_or_default();

    'outer: for candidate in &candidate_names {
        // Check that this prop has a string literal type in at least one member.
        let has_literal = members.iter().any(|(_, props)| {
            props.iter().any(|p| p.name.as_str() == *candidate && matches!(p.prop_type, PropType::StringLiteral(_)))
        });
        if !has_literal {
            continue;
        }

        let mut literal_values: Vec<&str> = Vec::new();
        for (_, member_props) in members {
            let found = member_props.iter().find(|p| p.name.as_str() == *candidate);
            match found {
                Some(p) => {
                    if let PropType::StringLiteral(ref s) = p.prop_type {
                        if literal_values.contains(&s.as_str()) {
                            // Not distinct — try next prop.
                            continue 'outer;
                        }
                        literal_values.push(s.as_str());
                    } else {
                        continue 'outer; // Not a literal in this member.
                    }
                }
                None => continue 'outer, // Not present in all members.
            }
        }

        if literal_values.len() == members.len() {
            return Some((*candidate).to_owned());
        }
    }

    None
}

/// Parse a TypeScript string union like `'disabled' | 'type' | 'form'` into individual key strings.
fn parse_string_union_keys(raw: &str) -> Vec<String> {
    raw.split('|').map(|s| s.trim().trim_matches('\'').trim_matches('"').to_owned()).filter(|s| !s.is_empty()).collect()
}
