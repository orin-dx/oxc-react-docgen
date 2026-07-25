//! Named type resolution.

use camino::Utf8Path;
use compact_str::CompactString;

use crate::known::{push_known_opaque_diagnostic, resolve_known, KnownPatternResult};
use crate::react_types;
use crate::types::*;

use super::alias::resolve_type_alias_type;
use super::collected::resolve_collected_type;
use super::import::{
    lookup_ambient_global, lookup_interface, lookup_type_alias, resolve_to_canonical, AmbientGlobalLookup,
};
use super::react::react_type_to_prop_type;
use super::{ResolutionContext, MAX_DEPTH};

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
        state.diagnostics.push(super::max_depth_diagnostic(&format!("named type '{}'", name), consuming_file));
        return PropType::Opaque { raw: name.to_string(), reason: OpaqueReason::DepthExceeded };
    }

    // ── 1. React builtin check ────────────────────────────────────────────────
    let lookup_name = name.strip_prefix("React.").unwrap_or(name.as_str());
    if react_types::is_react_builtin(lookup_name, &ctx.extra_builtins) {
        return react_type_to_prop_type(name.as_str(), args, consuming_file, ctx, state, depth);
    }

    // Resolve type arguments eagerly — needed for both source lookups and known patterns.
    let resolved_args: Vec<PropType> =
        args.iter().map(|a| resolve_collected_type(a, consuming_file, ctx, state, depth + 1)).collect();

    // ── 2. Import resolution → canonical (file, name) ─────────────────────────
    // Check source-defined types BEFORE known library patterns so that user-defined
    // types like ThemingProps or StylesApiProps are never silently replaced by
    // opaque known-pattern shortcuts.
    let (canonical_file, canonical_name) =
        resolve_to_canonical(name.as_str(), consuming_file, ctx, &mut state.diagnostics)
            .unwrap_or_else(|| (consuming_file.to_owned(), name.to_string()));

    // ── 3. Type alias lookup ──────────────────────────────────────────────────
    if let Some((matched_key, alias)) = lookup_type_alias(&ctx.global, canonical_file.as_str(), &canonical_name) {
        let alias = alias.clone();
        // A generic alias's own declared type parameters (`type Foo<TData> = ...`)
        // are expected, unexpandable placeholders wherever referenced in its body —
        // not unresolvable types. Register them so step 7 below doesn't warn.
        if let Some(params) = ctx.global.type_alias_params.get(&matched_key) {
            state.in_scope_type_params.extend(params.iter().cloned());
        }
        return resolve_type_alias_type(&alias, ctx, state, depth);
    }

    // ── 4. Interface lookup ───────────────────────────────────────────────────
    // At the prop-TYPE level (not chain level), an interface name is returned as Named.
    // Full prop expansion only happens at the component level via resolve_props_chain.
    if lookup_interface(&ctx.global, canonical_file.as_str(), &canonical_name).is_some() {
        return PropType::Named { name: name.clone(), args: resolved_args };
    }

    // ── 5. Known pattern check (fallback — only when not found in source) ─────
    // Library opaque patterns (ThemingProps, StylesApiProps, VariantProps, …) are
    // applied only when the type cannot be resolved from the project's own source.
    if let Some(result) = resolve_known(name.as_str(), &resolved_args, &ctx.global, &ctx.enum_bare_index) {
        return match result {
            KnownPatternResult::Type(pt) => {
                if let PropType::Opaque { reason, .. } = &pt {
                    push_known_opaque_diagnostic(&mut state.diagnostics, reason, name.as_str(), consuming_file);
                }
                pt
            }
            KnownPatternResult::Alias { name: alias_name } => {
                // Follow the alias through resolve_named.
                let alias_ct = CollectedType::Named { name: alias_name.as_str().into(), args: vec![] };
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
    if super::is_ts_utility_type(bare)
        || matches!(
            bare,
            // TypeScript primitives used as type names
            "object" | "Object" | "Function" | "Symbol" | "BigInt"
            // React HTML attribute types (not in is_react_builtin; appear as prop types)
            | "HTMLAttributes" | "InputHTMLAttributes" | "TextareaHTMLAttributes"
            | "SelectHTMLAttributes" | "ButtonHTMLAttributes" | "AnchorHTMLAttributes"
            | "FormHTMLAttributes" | "LabelHTMLAttributes" | "ImgHTMLAttributes"
            | "VideoHTMLAttributes" | "AudioHTMLAttributes" | "DOMAttributes"
            | "AriaAttributes" | "HTMLInputTypeAttribute" | "HTMLAttributeReferrerPolicy"
            | "HTMLAttributeAnchorTarget" | "HTMLInputAutoCompleteAttribute"
            // React SVG/generic HTML prop types
            | "SVGAttributes" | "SVGProps" | "HTMLProps"
            // React component utility types
            | "ComponentRef" | "JSXElementConstructor"
        )
        || bare.ends_with("HTMLAttributes")
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

    // ── 6.5 Enclosing generic's own type parameter — expected, not unresolvable ─
    if state.in_scope_type_params.contains(bare) {
        return PropType::Named { name: name.clone(), args: resolved_args };
    }

    // ── 6.7 TypeScript's own ambient globals (Date, RegExp, Element, ...) ────
    // Deliberately last: these never go through an import, so nothing earlier
    // ever had a reason to look here — but checked only after every other
    // shortcut above (including the well-known-TS-utility-type list at step 6)
    // has had its chance, since some names (Record, Partial, ...) are
    // themselves declared in these same lib files as mapped types this tool
    // can't expand, and step 6 already handles those correctly.
    if let Some(found) = lookup_ambient_global(ctx, bare) {
        return match found {
            AmbientGlobalLookup::Interface => PropType::Named { name: name.clone(), args: resolved_args },
            AmbientGlobalLookup::TypeAlias(alias) => {
                let alias = alias.clone();
                resolve_type_alias_type(&alias, ctx, state, depth)
            }
        };
    }

    // ── 7. Unresolvable — emit diagnostic, return Named ───────────────────────
    let location_note =
        super::unresolved_location_note(name.as_str(), consuming_file, &canonical_file, &canonical_name);
    state.diagnostics.push(Diagnostic {
        severity: DiagnosticSeverity::Warning,
        message: format!(
            "Cannot resolve type '{}' in '{}'{} — it will appear as an unexpanded named reference",
            name, consuming_file, location_note
        ),
        file: Some(consuming_file.to_string()),
        line: None,
        column: None,
        help: Some("Check that the package is installed and its types are resolvable.".into()),
        code: DiagnosticCode::UnresolvableImport,
    });
    PropType::Named { name: name.clone(), args: resolved_args }
}
