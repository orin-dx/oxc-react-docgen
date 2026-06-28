//! Function type and typeof resolution.

use camino::Utf8Path;
use compact_str::CompactString;

use crate::types::*;

use super::collected::resolve_collected_type;
use super::ResolutionContext;

pub(super) fn resolve_function_type(
    params: &[CollectedType],
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

    if returns_react_node && params.len() == 1 {
        let event_type = params[0].to_raw_string();
        return PropType::EventHandler { event_type };
    }

    // Generic event handler: (e: SomeEvent) => void
    if params.len() == 1 {
        let event_type = params[0].to_raw_string();
        return PropType::EventHandler { event_type };
    }

    // Zero-arg callback: () => void
    if params.is_empty() {
        return PropType::EventHandler { event_type: "void".into() };
    }

    // Multi-param function — describe as opaque.
    let param_strs: Vec<String> = params.iter().map(|p| p.to_raw_string()).collect();
    let raw = format!("({}) => {}", param_strs.join(", "), return_type.to_raw_string());

    // Resolve the return type to see if it's ReactNode.
    let _ = resolve_collected_type(return_type, consuming_file, ctx, state, depth + 1);

    PropType::Opaque { raw, reason: OpaqueReason::ConditionalType }
}

pub(super) fn resolve_typeof(
    name: &CompactString,
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> PropType {
    // `typeof X` — look for X in global.enums (for cva() results).
    let found_enum =
        ctx.global.enums.iter().find(|(key, _)| key.ends_with(&format!(":{}", name)) || key.as_str() == name.as_str());

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
