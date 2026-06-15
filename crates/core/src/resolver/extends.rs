//! Extends ref resolution.

use camino::Utf8Path;

use crate::types::*;

use super::{ResolutionContext, ResolvedChain};
use super::chain::resolve_props_chain;
use super::import::resolve_import_specifier;
use super::react::resolve_react_types_file;

/// Resolve a single `ExtendsRef` and return `(chain, Option<InheritedLayer>)`.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_extends_ref(
    extends_ref: &ExtendsRef,
    iface_file: &Utf8Path,
    mapping: &ComponentMapping,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> (ResolvedChain, Option<InheritedLayer>) {
    match extends_ref {
        ExtendsRef::Builtin { name, element, type_args } => {
            // HTML element attrs — the actual props are not resolved here
            // (they live in @types/react); we record an InheritedLayer instead.
            if let Some(element_name) = element {
                let layer = InheritedLayer {
                    type_name: name.to_string(),
                    file_name: resolve_react_types_file(iface_file, ctx),
                    omitted: vec![],
                    html_element: Some(element_name.clone()),
                    total_props: 0, // unknown without type-checker
                };
                (ResolvedChain::default(), Some(layer))
            } else {
                // Non-HTML-element builtins. ComponentPropsWithoutRef/ComponentProps:
                // expand directly to HtmlAttributes based on the first type arg.
                let bare = name.as_str().strip_prefix("React.").unwrap_or(name.as_str());
                if matches!(bare, "ComponentPropsWithoutRef" | "ComponentPropsWithRef" | "ComponentProps") {
                    if let Some(raw_arg) = type_args.first() {
                        let inner = raw_arg.trim().trim_matches('"').trim_matches('\'');
                        if !inner.is_empty() {
                            let layer = InheritedLayer {
                                type_name: name.to_string(),
                                file_name: resolve_react_types_file(iface_file, ctx),
                                omitted: vec![],
                                html_element: Some(inner.to_lowercase()),
                                total_props: 0,
                            };
                            return (
                                ResolvedChain { inheritance: vec![layer.clone()], ..Default::default() },
                                Some(layer),
                            );
                        }
                    }
                }
                // Other non-HTML builtins (PropsWithChildren, ElementRef, etc.)
                // — resolve through the chain so resolve_known can handle them.
                let chain = resolve_props_chain(
                    name.as_str(),
                    type_args,
                    iface_file,
                    mapping,
                    ctx,
                    state,
                    depth,
                );
                (chain, None)
            }
        }

        ExtendsRef::SameFile { name, type_args } => {
            let chain = resolve_props_chain(
                name.as_str(),
                type_args,
                iface_file,
                mapping,
                ctx,
                state,
                depth,
            );
            (chain, None)
        }

        ExtendsRef::Imported { local_name, type_args, source_specifier } => {
            let resolved_file = source_specifier
                .as_deref()
                .and_then(|spec| {
                    resolve_import_specifier(spec, iface_file, ctx, &mut state.diagnostics)
                })
                .unwrap_or_else(|| iface_file.to_owned());

            let chain = resolve_props_chain(
                local_name.as_str(),
                type_args,
                &resolved_file,
                mapping,
                ctx,
                state,
                depth,
            );
            (chain, None)
        }
    }
}
