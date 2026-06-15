//! Union, intersection, and indexed access resolution.

use camino::Utf8Path;

use crate::types::*;

use super::{ResolutionContext};
use super::collected::resolve_collected_type;

pub(super) fn resolve_union(
    members: &[CollectedType],
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> PropType {
    // Filter out `undefined` from optional unions: `string | undefined` → `string`
    // (the `required: false` on the prop already captures optionality).
    let meaningful: Vec<&CollectedType> = members
        .iter()
        .filter(|m| !matches!(m, CollectedType::Undefined))
        .collect();

    let to_resolve = if meaningful.is_empty() { members.iter().collect::<Vec<_>>() } else { meaningful };

    let resolved: Vec<PropType> = to_resolve
        .iter()
        .map(|m| resolve_collected_type(m, consuming_file, ctx, state, depth + 1))
        .collect();

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
    let non_empty: Vec<&CollectedType> = members
        .iter()
        .filter(|m| !matches!(m, CollectedType::Object(f) if f.is_empty()))
        .collect();

    if non_empty.len() == 1 && matches!(non_empty[0], CollectedType::String) {
        return PropType::String;
    }
    if non_empty.len() == 1 {
        return resolve_collected_type(
            non_empty[0],
            consuming_file,
            ctx,
            state,
            depth + 1,
        );
    }

    let resolved: Vec<PropType> = members
        .iter()
        .map(|m| resolve_collected_type(m, consuming_file, ctx, state, depth + 1))
        .collect();

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
            "zIndex" | "opacity" | "order" | "flexGrow" | "flexShrink" | "flexBasis"
            | "lineHeight" | "fontWeight" | "columnCount" | "tabSize" | "animationIterationCount",
        ) => Some(PropType::Number),
        ("CSSProperties" | "React.CSSProperties", _) if !key_str.is_empty() => {
            Some(PropType::String)
        }
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
    PropType::Opaque {
        raw: expression.clone(),
        reason: OpaqueReason::IndexedAccess { expression },
    }
}
