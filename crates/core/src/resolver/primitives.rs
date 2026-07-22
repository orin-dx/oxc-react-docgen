//! Union, intersection, and indexed access resolution.

use camino::Utf8Path;

use crate::types::*;

use super::collected::resolve_collected_type;
use super::import::{lookup_interface, lookup_interface_including_ambient, resolve_to_canonical};
use super::substitute::{build_substitution, substitute_type};
use super::{ResolutionContext, MAX_DEPTH};

pub(super) fn resolve_union(
    members: &[CollectedType],
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> PropType {
    // Filter out `undefined` from optional unions: `string | undefined` → `string`
    // (the `required: false` on the prop already captures optionality).
    let meaningful: Vec<&CollectedType> = members.iter().filter(|m| !matches!(m, CollectedType::Undefined)).collect();

    let to_resolve = if meaningful.is_empty() { members.iter().collect::<Vec<_>>() } else { meaningful };

    let resolved: Vec<PropType> =
        to_resolve.iter().map(|m| resolve_collected_type(m, consuming_file, ctx, state, depth + 1)).collect();

    // Flatten nested Unions.
    let mut flat: Vec<PropType> = Vec::with_capacity(resolved.len());
    for pt in resolved {
        if let PropType::Union(inner) = pt {
            flat.extend(inner);
        } else {
            flat.push(pt);
        }
    }

    if flat.len() == 1 {
        flat.remove(0)
    } else {
        PropType::Union(flat)
    }
}

pub(super) fn resolve_intersection(
    members: &[CollectedType],
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> PropType {
    // Normalize `(string & {})` → `PropType::String`.
    // `{}` is `CollectedType::Object([])` (empty object type).
    let non_empty: Vec<&CollectedType> =
        members.iter().filter(|m| !matches!(m, CollectedType::Object(f) if f.is_empty())).collect();

    if non_empty.len() == 1 && matches!(non_empty[0], CollectedType::String) {
        return PropType::String;
    }
    if non_empty.len() == 1 {
        return resolve_collected_type(non_empty[0], consuming_file, ctx, state, depth + 1);
    }

    let resolved: Vec<PropType> =
        members.iter().map(|m| resolve_collected_type(m, consuming_file, ctx, state, depth + 1)).collect();

    PropType::Intersection(resolved)
}

pub(super) fn resolve_indexed_access(
    obj: &CollectedType,
    key: &CollectedType,
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> PropType {
    let obj_name = match obj {
        CollectedType::Named { name, .. } => name.as_str(),
        _ => "",
    };
    let key_str = match key {
        CollectedType::StringLiteral(s) => s.as_str(),
        _ => "",
    };

    // Known table lookup — avoids needing the type checker for common cases.
    let known = match (obj_name, key_str) {
        (
            "CSSProperties" | "React.CSSProperties",
            "zIndex"
            | "opacity"
            | "order"
            | "flexGrow"
            | "flexShrink"
            | "flexBasis"
            | "lineHeight"
            | "fontWeight"
            | "columnCount"
            | "tabSize"
            | "animationIterationCount",
        ) => Some(PropType::Number),
        ("CSSProperties" | "React.CSSProperties", _) if !key_str.is_empty() => Some(PropType::String),
        (
            "HTMLAttributes" | "React.HTMLAttributes" | "DOMAttributes" | "React.DOMAttributes",
            "className" | "id" | "slot" | "title" | "lang" | "dir",
        ) => Some(PropType::String),
        ("HTMLAttributes" | "React.HTMLAttributes", "tabIndex") => Some(PropType::Number),
        ("HTMLAttributes" | "React.HTMLAttributes", "style") => Some(PropType::CssProperties),
        _ => None,
    };

    if let Some(pt) = known {
        return pt;
    }

    // A generic interface referenced with concrete args (e.g.
    // `RenderableProps<FieldRenderProps<FieldValue, T>>["children"]`) resolves
    // to a bare `PropType::Named` at the type level — interfaces are never
    // expanded there (only at the chain/component level) — so the
    // `PropType::Object` fallback below never matches an interface's own
    // fields. Look the field up directly on the interface's declaration
    // instead, substituting its declared type parameters with the caller's
    // concrete arguments before resolving.
    if let CollectedType::Named { name: obj_type_name, args: obj_args } = obj {
        let (canonical_file, canonical_name) =
            resolve_to_canonical(obj_type_name.as_str(), consuming_file, ctx, &mut state.diagnostics)
                .unwrap_or_else(|| (consuming_file.to_owned(), obj_type_name.to_string()));
        if let Some(iface) = lookup_interface_including_ambient(ctx, canonical_file.as_str(), &canonical_name) {
            if let Some(field) = iface.props.iter().find(|f| f.name == key_str) {
                let field_type = match ctx.global.interface_type_params.get(&iface.scoped_key) {
                    Some(params) if !params.is_empty() && !obj_args.is_empty() => {
                        let subst = build_substitution(params, obj_args, consuming_file);
                        substitute_type(&field.collected_type, &subst)
                    }
                    _ => field.collected_type.clone(),
                };
                let iface_file_path = iface.file_path.clone();
                return resolve_collected_type(&field_type, &iface_file_path, ctx, state, depth + 1);
            }
            // Not declared directly on this interface — DOM/ambient interfaces
            // commonly inherit shared attributes through `extends` (e.g.
            // `HTMLDivElement extends HTMLElement`, which declares `dir`/`lang`/
            // `title`), so walk the chain before giving up.
            if let Some((field, owner)) = find_field_in_ancestors(iface, key_str, ctx, depth) {
                let owner_file_path = owner.file_path.clone();
                return resolve_collected_type(&field.collected_type, &owner_file_path, ctx, state, depth + 1);
            }
        }
    }

    // Try to resolve the object type and look for the key.
    let obj_resolved = resolve_collected_type(obj, consuming_file, ctx, state, depth + 1);
    if let PropType::Object(fields) = &obj_resolved {
        if let Some(field) = fields.iter().find(|f| f.name == key_str) {
            return field.prop_type.clone();
        }
    }

    let expression = format!("{}[{}]", obj.to_raw_string(), key.to_raw_string());
    state.diagnostics.push(Diagnostic {
        severity: DiagnosticSeverity::Info,
        message: format!("Indexed access type '{}' could not be statically resolved", expression),
        file: Some(consuming_file.to_string()),
        line: None,
        column: None,
        help: Some("Enable typescript-go to resolve indexed access types.".into()),
        code: DiagnosticCode::IndexedAccessOpaque,
    });
    PropType::Opaque { raw: expression.clone(), reason: OpaqueReason::IndexedAccess { expression } }
}

/// Search an interface's `extends` chain (depth-first) for a field, returning
/// it alongside the interface that actually declares it. Only `SameFile`
/// ancestors are followed — TypeScript's own ambient lib files (the only
/// callers of this today) declare their whole DOM interface hierarchy in one
/// file, so cross-file ancestry never applies here.
fn find_field_in_ancestors<'g>(
    iface: &'g CollectedInterface,
    key_str: &str,
    ctx: &'g ResolutionContext,
    depth: u8,
) -> Option<(&'g RawProp, &'g CollectedInterface)> {
    if depth > MAX_DEPTH {
        return None;
    }
    for extends_ref in &iface.extends {
        let parent = match extends_ref {
            ExtendsRef::SameFile { name, .. } => lookup_interface(&ctx.global, iface.file_path.as_str(), name.as_str()),
            ExtendsRef::Imported { .. } | ExtendsRef::Builtin { .. } => None,
        };
        let Some(parent) = parent else { continue };
        if let Some(field) = parent.props.iter().find(|f| f.name == key_str) {
            return Some((field, parent));
        }
        if let Some(found) = find_field_in_ancestors(parent, key_str, ctx, depth + 1) {
            return Some(found);
        }
    }
    None
}
