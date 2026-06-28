//! Central dispatch: CollectedType → PropType.

use camino::Utf8Path;

use crate::types::*;

use super::func::{resolve_function_type, resolve_typeof};
use super::named::resolve_named;
use super::primitives::{resolve_indexed_access, resolve_intersection, resolve_union};
use super::template::resolve_template_literal;
use super::{ResolutionContext, MAX_DEPTH};

/// Central dispatch: convert a `CollectedType` to a `PropType`.
/// Never re-parses strings — everything is already structured.
#[allow(clippy::too_many_arguments)]
pub fn resolve_collected_type(
    ct: &CollectedType,
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> PropType {
    if depth > MAX_DEPTH {
        state.diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Warning,
            message: format!("Max resolution depth exceeded resolving type: {}", ct.to_raw_string()),
            file: Some(consuming_file.to_string()),
            line: None,
            column: None,
            help: None,
            code: DiagnosticCode::MaxDepthExceeded,
        });
        return PropType::Opaque { raw: ct.to_raw_string(), reason: OpaqueReason::DepthExceeded };
    }

    match ct {
        // ── Primitives ────────────────────────────────────────────────────────
        CollectedType::String => PropType::String,
        CollectedType::Number => PropType::Number,
        CollectedType::Boolean => PropType::Boolean,
        CollectedType::Null => PropType::Null,
        CollectedType::Undefined => PropType::Undefined,
        CollectedType::Any => PropType::Any,
        CollectedType::Never => PropType::Never,
        CollectedType::Unknown => PropType::Unknown,
        CollectedType::Void => PropType::Void,
        // BigInt/Symbol — no dedicated PropType; surface as Named.
        CollectedType::BigInt => PropType::Named { name: "bigint".into(), args: vec![] },
        CollectedType::Symbol => PropType::Named { name: "symbol".into(), args: vec![] },

        // ── Literals ─────────────────────────────────────────────────────────
        CollectedType::StringLiteral(s) => PropType::StringLiteral(s.to_string()),
        CollectedType::NumberLiteral(n) => PropType::NumberLiteral(*n),
        CollectedType::BoolLiteral(b) => PropType::BoolLiteral(*b),

        // ── Composites ───────────────────────────────────────────────────────
        CollectedType::Union(members) => resolve_union(members, consuming_file, ctx, state, depth),
        CollectedType::Intersection(members) => resolve_intersection(members, consuming_file, ctx, state, depth),
        CollectedType::Array(inner) => {
            PropType::Array(Box::new(resolve_collected_type(inner, consuming_file, ctx, state, depth + 1)))
        }
        CollectedType::Tuple(members) => PropType::Tuple(
            members.iter().map(|m| resolve_collected_type(m, consuming_file, ctx, state, depth + 1)).collect(),
        ),
        CollectedType::Object(fields) => PropType::Object(
            fields
                .iter()
                .map(|f| ObjectField {
                    name: f.name.clone(),
                    prop_type: resolve_collected_type(&f.collected_type, consuming_file, ctx, state, depth + 1),
                    required: f.required,
                    description: f.description.clone(),
                })
                .collect(),
        ),

        // ── Named type reference ──────────────────────────────────────────────
        CollectedType::Named { name, args } => resolve_named(name, args, consuming_file, ctx, state, depth),

        // ── typeof X ─────────────────────────────────────────────────────────
        CollectedType::TypeOf(name) => resolve_typeof(name, consuming_file, ctx, &mut state.diagnostics),

        // ── Indexed access ───────────────────────────────────────────────────
        CollectedType::IndexedAccess { obj, key } => {
            resolve_indexed_access(obj, key, consuming_file, ctx, state, depth)
        }

        // ── Template literal ─────────────────────────────────────────────────
        CollectedType::TemplateLiteral(parts) => resolve_template_literal(parts, consuming_file, ctx, state, depth),

        // ── Function type ─────────────────────────────────────────────────────
        CollectedType::Function { params, return_type } => {
            resolve_function_type(params, return_type, consuming_file, ctx, state, depth)
        }

        // ── Opaque (needs type checker) ───────────────────────────────────────
        CollectedType::Conditional { .. } => {
            PropType::Opaque { raw: ct.to_raw_string(), reason: OpaqueReason::ConditionalType }
        }
        CollectedType::Mapped { .. } => PropType::Opaque { raw: ct.to_raw_string(), reason: OpaqueReason::MappedType },

        // ── Raw fallback ─────────────────────────────────────────────────────
        CollectedType::Raw(s) => {
            let trimmed = s.trim();
            // `typeof X` — ExtendsRef.type_args serialized form; resolve as Named.
            if let Some(name) = trimmed.strip_prefix("typeof ") {
                return PropType::Named { name: name.trim().into(), args: vec![] };
            }
            // Double-quoted string literal: `"button"` → StringLiteral.
            if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
                return PropType::StringLiteral(trimmed[1..trimmed.len() - 1].to_owned());
            }
            // Single-quoted string literal: `'button'` → StringLiteral.
            if trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2 {
                return PropType::StringLiteral(trimmed[1..trimmed.len() - 1].to_owned());
            }
            // Simple identifier → Named type reference.
            if !trimmed.is_empty()
                && !trimmed.contains(' ')
                && !trimmed.contains('|')
                && !trimmed.contains('&')
                && !trimmed.contains('<')
            {
                PropType::Named { name: trimmed.into(), args: vec![] }
            } else {
                PropType::Opaque { raw: s.clone(), reason: OpaqueReason::DepthExceeded }
            }
        }
    }
}
