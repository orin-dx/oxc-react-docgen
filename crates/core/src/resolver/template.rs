//! Template literal type expansion.

use camino::Utf8Path;

use crate::types::*;

use super::{ResolutionContext};
use super::collected::resolve_collected_type;
use super::import::resolve_to_canonical;

pub(super) fn resolve_template_literal(
    parts: &[CollectedType],
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> PropType {
    // Try to expand: `compact-${Size}` where Size = "xs"|"sm"|...
    // Each part must be either a string literal or a type that resolves to a LiteralUnion.
    let expanded = try_expand_template_literal(parts, consuming_file, ctx, state, depth);

    if let Some(values) = expanded {
        if values.len() == 1 {
            return PropType::StringLiteral(values.into_iter().next().unwrap());
        }
        return PropType::LiteralUnion { members: values, has_default: false };
    }

    let raw = CollectedType::TemplateLiteral(parts.to_vec()).to_raw_string();
    state.diagnostics.push(Diagnostic {
        severity: DiagnosticSeverity::Info,
        message: format!("Template literal type '{}' could not be statically expanded", raw),
        file: Some(consuming_file.to_string()),
        line: None,
        column: None,
        help: Some(
            "Enable typescript-go or add explicit string literal union for template literal types."
                .into(),
        ),
        code: DiagnosticCode::TemplateLiteralOpaque,
    });
    PropType::Opaque {
        raw: raw.clone(),
        reason: OpaqueReason::TemplateLiteral { expression: raw },
    }
}

/// Try to fully expand a template literal into a list of concrete string values.
/// Returns `None` if any part cannot be resolved to string literals.
pub(super) fn try_expand_template_literal(
    parts: &[CollectedType],
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> Option<Vec<String>> {
    // Collect per-part string alternatives.
    let mut per_part: Vec<Vec<String>> = Vec::new();

    for part in parts {
        match part {
            CollectedType::StringLiteral(s) => {
                per_part.push(vec![s.to_string()]);
            }
            CollectedType::Named { name, .. } => {
                // Look up in global type aliases for a LiteralUnion.
                let resolved =
                    resolve_named_to_string_literals(name.as_str(), consuming_file, ctx, state, depth + 1);
                if let Some(strs) = resolved {
                    per_part.push(strs);
                } else {
                    return None; // Can't expand.
                }
            }
            _ => {
                let pt = resolve_collected_type(part, consuming_file, ctx, state, depth + 1);
                match &pt {
                    PropType::StringLiteral(s) => per_part.push(vec![s.clone()]),
                    PropType::LiteralUnion { members, .. } => per_part.push(members.clone()),
                    PropType::Union(members) => {
                        let strs: Option<Vec<String>> = members
                            .iter()
                            .map(|m| {
                                if let PropType::StringLiteral(s) = m {
                                    Some(s.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if let Some(s) = strs {
                            per_part.push(s);
                        } else {
                            return None;
                        }
                    }
                    _ => return None,
                }
            }
        }
    }

    if per_part.is_empty() {
        return Some(vec![String::new()]);
    }

    // Cartesian product across all parts.
    let mut result = vec![String::new()];
    for alternatives in per_part {
        let mut next = Vec::with_capacity(result.len() * alternatives.len());
        for prefix in &result {
            for alt in &alternatives {
                next.push(format!("{}{}", prefix, alt));
            }
        }
        result = next;
    }

    Some(result)
}

pub(super) fn resolve_named_to_string_literals(
    name: &str,
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> Option<Vec<String>> {
    let (canonical_file, canonical_name) =
        resolve_to_canonical(name, consuming_file, ctx, &mut state.diagnostics)
            .unwrap_or_else(|| (consuming_file.to_owned(), name.to_owned()));

    let scoped_key = format!("{}:{}", canonical_file, canonical_name);

    if let Some(alias) = ctx.global.type_aliases.get(&scoped_key) {
        match alias {
            CollectedTypeAlias::LiteralUnion { members, .. } => {
                return Some(members.clone());
            }
            CollectedTypeAlias::Union { members, .. } => {
                let strs: Option<Vec<String>> = members
                    .iter()
                    .map(|m| {
                        if let CollectedType::StringLiteral(s) = m {
                            Some(s.to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                return strs;
            }
            _ => {}
        }
    }

    // Try resolving via resolve_collected_type and extracting literals.
    let ct = CollectedType::Named { name: name.into(), args: vec![] };
    let pt = resolve_collected_type(&ct, consuming_file, ctx, state, depth);
    match pt {
        PropType::StringLiteral(s) => Some(vec![s]),
        PropType::LiteralUnion { members, .. } => Some(members),
        PropType::Union(members) => members
            .into_iter()
            .map(|m| if let PropType::StringLiteral(s) = m { Some(s) } else { None })
            .collect(),
        _ => None,
    }
}
