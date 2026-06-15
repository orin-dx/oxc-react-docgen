//! Named type resolution.

use camino::Utf8Path;
use compact_str::CompactString;

use crate::known::{resolve_known, KnownPatternResult};
use crate::react_types;
use crate::types::*;

use super::{ResolutionContext, MAX_DEPTH};
use super::alias::resolve_type_alias_type;
use super::collected::resolve_collected_type;
use super::import::resolve_to_canonical;
use super::react::react_type_to_prop_type;

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_named(
    name: &CompactString,
    args: &[CollectedType],
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> PropType {
    if depth > MAX_DEPTH {
        state.diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Warning,
            message: format!("Max resolution depth exceeded for named type '{}'", name),
            file: Some(consuming_file.to_string()),
            line: None,
            column: None,
            help: None,
            code: DiagnosticCode::MaxDepthExceeded,
        });
        return PropType::Opaque {
            raw: name.to_string(),
            reason: OpaqueReason::DepthExceeded,
        };
    }

    // ── 1. React builtin check ────────────────────────────────────────────────
    let lookup_name = name.strip_prefix("React.").unwrap_or(name.as_str());
    if react_types::is_react_builtin(lookup_name, &ctx.extra_builtins) {
        return react_type_to_prop_type(name.as_str(), args, consuming_file, ctx, state, depth);
    }

    // Resolve type arguments eagerly — needed for both source lookups and known patterns.
    let resolved_args: Vec<PropType> = args
        .iter()
        .map(|a| resolve_collected_type(a, consuming_file, ctx, state, depth + 1))
        .collect();

    // ── 2. Import resolution → canonical (file, name) ─────────────────────────
    // Check source-defined types BEFORE known library patterns so that user-defined
    // types like ThemingProps or StylesApiProps are never silently replaced by
    // opaque known-pattern shortcuts.
    let (canonical_file, canonical_name) =
        resolve_to_canonical(name.as_str(), consuming_file, ctx, &mut state.diagnostics)
            .unwrap_or_else(|| (consuming_file.to_owned(), name.to_string()));

    let scoped_key = format!("{}:{}", canonical_file, canonical_name);

    // ── 3. Type alias lookup ──────────────────────────────────────────────────
    if let Some(alias) = ctx.global.type_aliases.get(&scoped_key).cloned() {
        return resolve_type_alias_type(&alias, consuming_file, ctx, state, depth);
    }

    // ── 4. Interface lookup ───────────────────────────────────────────────────
    // At the prop-TYPE level (not chain level), an interface name is returned as Named.
    // Full prop expansion only happens at the component level via resolve_props_chain.
    if ctx.global.interfaces.contains_key(&scoped_key) {
        return PropType::Named { name: name.clone(), args: resolved_args };
    }

    // ── 5. Known pattern check (fallback — only when not found in source) ─────
    // Library opaque patterns (ThemingProps, StylesApiProps, VariantProps, …) are
    // applied only when the type cannot be resolved from the project's own source.
    if let Some(result) = resolve_known(name.as_str(), &resolved_args, &ctx.global) {
        return match result {
            KnownPatternResult::Type(pt) => pt,
            KnownPatternResult::Alias { name: alias_name, .. } => {
                // Follow the alias through resolve_named.
                let alias_ct =
                    CollectedType::Named { name: alias_name.as_str().into(), args: vec![] };
                resolve_collected_type(&alias_ct, consuming_file, ctx, state, depth + 1)
            }
            KnownPatternResult::Props(_) => {
                // Props result at type level — surface as Named.
                PropType::Named { name: name.clone(), args: resolved_args }
            }
        };
    }

    // ── 6. Silent no-op for well-known unresolvable types ────────────────────
    // TypeScript built-in utility types, DOM element types, and React HTML
    // attribute types all appear as prop types in real-world .d.ts files but
    // cannot be expanded without a type-checker. Return Named silently.
    let bare = name.strip_prefix("React.").unwrap_or(name.as_str());
    if matches!(
        bare,
        // TypeScript utility types
        "Partial" | "Required" | "Readonly" | "NonNullable" | "Record"
            | "ReadonlyArray" | "Array" | "Promise" | "Extract" | "Exclude"
            | "ReturnType" | "Parameters" | "Awaited" | "Omit" | "Pick"
            // TypeScript primitives used as type names
            | "object" | "Object" | "Function" | "Symbol" | "BigInt"
            // React HTML attribute types (not in is_react_builtin; appear as prop types)
            | "HTMLAttributes" | "InputHTMLAttributes" | "TextareaHTMLAttributes"
            | "SelectHTMLAttributes" | "ButtonHTMLAttributes" | "AnchorHTMLAttributes"
            | "FormHTMLAttributes" | "LabelHTMLAttributes" | "ImgHTMLAttributes"
            | "VideoHTMLAttributes" | "AudioHTMLAttributes" | "DOMAttributes"
            | "AriaAttributes" | "HTMLInputTypeAttribute" | "HTMLAttributeReferrerPolicy"
            | "HTMLAttributeAnchorTarget" | "HTMLInputAutoCompleteAttribute"
    ) || bare.ends_with("HTMLAttributes")
    {
        return PropType::Named { name: name.clone(), args: resolved_args };
    }
    // DOM element ref types (HTMLDivElement, HTMLInputElement, etc.)
    if bare.starts_with("HTML") && bare.ends_with("Element") {
        return PropType::Named { name: name.clone(), args: resolved_args };
    }
    if bare.starts_with("SVG") && bare.ends_with("Element") {
        return PropType::Named { name: name.clone(), args: resolved_args };
    }

    // ── 7. Unresolvable — emit diagnostic, return Named ───────────────────────
    state.diagnostics.push(Diagnostic {
        severity: DiagnosticSeverity::Warning,
        message: format!(
            "Cannot resolve type '{}' in '{}' — it will appear as opaque",
            name, consuming_file
        ),
        file: Some(consuming_file.to_string()),
        line: None,
        column: None,
        help: Some(
            "Check that the package is installed and its types are resolvable.".into(),
        ),
        code: DiagnosticCode::UnresolvableImport,
    });
    PropType::Named { name: name.clone(), args: resolved_args }
}
