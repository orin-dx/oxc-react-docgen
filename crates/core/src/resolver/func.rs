//! Function type and typeof resolution.

use camino::Utf8Path;
use compact_str::CompactString;

use crate::types::*;

use super::collected::resolve_collected_type;
use super::ResolutionContext;

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_function_type(
    params: &[CollectedType],
    param_names: &[Option<CompactString>],
    return_type: &CollectedType,
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> PropType {
    // Check if the return type is a React node → render prop pattern.
    let returns_react_node = matches!(
        return_type,
        CollectedType::Named { name, .. }
            if matches!(
                name.as_str(),
                "ReactNode"
                    | "ReactElement"
                    | "JSX.Element"
                    | "Element"
                    | "ReactPortal"
                    | "ReactFragment"
            )
    );

    let first_param_name = || param_names.first().and_then(|n| n.as_ref()).map(|s| s.to_string());

    if returns_react_node && params.len() == 1 {
        let event_type = params[0].to_raw_string();
        return PropType::EventHandler { event_type, param_name: first_param_name() };
    }

    // Generic event handler: (e: SomeEvent) => void
    if params.len() == 1 {
        let event_type = params[0].to_raw_string();
        return PropType::EventHandler { event_type, param_name: first_param_name() };
    }

    // Zero-arg callback: () => void
    if params.is_empty() {
        return PropType::EventHandler { event_type: "void".into(), param_name: None };
    }

    // Multi-param function — describe as opaque.
    let param_strs: Vec<String> = params.iter().map(|p| p.to_raw_string()).collect();
    let raw = format!("({}) => {}", param_strs.join(", "), return_type.to_raw_string());

    // Resolve the return type to see if it's ReactNode.
    let _ = resolve_collected_type(return_type, consuming_file, ctx, state, depth + 1);

    PropType::Opaque { raw, reason: OpaqueReason::MultiParamFunction }
}

pub(super) fn resolve_typeof(
    name: &CompactString,
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> PropType {
    // `typeof X` — look for X in global.enums (for cva() results), via the
    // precomputed bare-name index (O(1) instead of a linear scan over every
    // enum/cva/tv/recipe entry in the project with a per-candidate allocation).
    let found_enum = ctx.enum_bare_index.get(name.as_str()).and_then(|key| ctx.global.enums.get(key.as_str()));

    if found_enum.is_some() {
        // Has cva-like enum entries — the VariantProps<typeof X> pattern handles this.
        // At the type level, surface as Named.
        return PropType::Named { name: name.clone(), args: vec![] };
    }

    diagnostics.push(Diagnostic {
        severity: DiagnosticSeverity::Info,
        message: format!("typeof '{}' in '{}' — could not statically evaluate", name, consuming_file),
        file: Some(consuming_file.to_string()),
        line: None,
        column: None,
        help: None,
        code: DiagnosticCode::OpaqueType,
    });

    PropType::Named { name: name.clone(), args: vec![] }
}
