//! React builtin type mapping and related helpers.

use camino::Utf8Path;
use oxc_resolver::AliasValue;

use crate::types::*;

use super::{ResolutionContext};
use super::collected::resolve_collected_type;

/// Map a React builtin type name to the appropriate `PropType`.
pub(super) fn react_type_to_prop_type(
    name: &str,
    args: &[CollectedType],
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    state: &mut ResolveState,
    depth: u8,
) -> PropType {
    // Strip "React." prefix for matching.
    let strip = name.strip_prefix("React.").unwrap_or(name);

    match strip {
        // React node types.
        "ReactNode" | "ReactElement" | "JSX.Element" | "ReactPortal" | "ReactFragment"
        | "ReactChild" => PropType::ReactNode,

        // CSS properties.
        "CSSProperties" | "CSSObject" => PropType::CssProperties,

        // Named event handlers (e.g. MouseEventHandler).
        n if n.ends_with("EventHandler") || n.ends_with("Handler") => {
            PropType::EventHandler { event_type: name.to_owned() }
        }

        // Synthetic and DOM events — the type IS the event type.
        "SyntheticEvent" | "MouseEvent" | "KeyboardEvent" | "ChangeEvent" | "FocusEvent"
        | "FormEvent" | "DragEvent" | "TouchEvent" | "WheelEvent" | "AnimationEvent"
        | "TransitionEvent" | "ClipboardEvent" | "CompositionEvent" | "PointerEvent" => {
            let raw_args: Vec<String> = args.iter().map(|a| a.to_raw_string()).collect();
            let event_type = if raw_args.is_empty() {
                name.to_owned()
            } else {
                format!("{}<{}>", name, raw_args.join(", "))
            };
            PropType::EventHandler { event_type }
        }

        // Ref types.
        "Ref" | "RefObject" | "ForwardedRef" | "MutableRefObject" | "RefCallback"
        | "LegacyRef" => {
            let element = args.first().map(|a| a.to_raw_string());
            PropType::Ref { element }
        }

        // ElementType — component-as-prop.
        "ElementType" => PropType::ElementType,

        // FC / FunctionComponent — return as Named.
        "FC" | "FunctionComponent" | "VFC" | "VoidFunctionComponent" | "ComponentType"
        | "ForwardRefExoticComponent" => {
            let resolved_args: Vec<PropType> = args
                .iter()
                .map(|a| {
                    resolve_collected_type(a, consuming_file, ctx, state, depth + 1)
                })
                .collect();
            PropType::Named { name: name.into(), args: resolved_args }
        }

        // ComponentPropsWithoutRef<'button'> or ComponentPropsWithoutRef<typeof X>.
        "ComponentPropsWithoutRef" | "ComponentProps" | "ComponentPropsWithRef" => {
            if let Some(first) = args.first() {
                match first {
                    CollectedType::StringLiteral(el) => PropType::HtmlAttributes {
                        element: el.to_lowercase().to_string(),
                        omitted: vec![],
                    },
                    other => PropType::Named {
                        name: name.into(),
                        args: vec![resolve_collected_type(
                            other,
                            consuming_file,
                            ctx,
                            state,
                            depth + 1,
                        )],
                    },
                }
            } else {
                PropType::Any
            }
        }

        // PropsWithChildren / PropsWithRef — resolve inner type.
        "PropsWithChildren" | "PropsWithRef" => {
            if let Some(first) = args.first() {
                resolve_collected_type(first, consuming_file, ctx, state, depth + 1)
            } else {
                PropType::Any
            }
        }

        // ElementRef.
        "ElementRef" => PropType::Ref { element: None },

        // Context / Consumer / Provider — surface as Named.
        "Context" | "Consumer" | "Provider" | "RefAttributes" => {
            let resolved_args: Vec<PropType> = args
                .iter()
                .map(|a| {
                    resolve_collected_type(a, consuming_file, ctx, state, depth + 1)
                })
                .collect();
            PropType::Named { name: name.into(), args: resolved_args }
        }

        // Default — surface as Named with resolved args.
        _ => {
            let resolved_args: Vec<PropType> = args
                .iter()
                .map(|a| {
                    resolve_collected_type(a, consuming_file, ctx, state, depth + 1)
                })
                .collect();
            PropType::Named { name: name.into(), args: resolved_args }
        }
    }
}

/// Best-effort path to the @types/react .d.ts file for RDT propFilter compat.
/// Falls back to a synthetic path if @types/react is not installed.
pub(super) fn resolve_react_types_file(from_file: &Utf8Path, ctx: &ResolutionContext) -> String {
    // Try to resolve from the consuming file's directory.
    if let Some(from_dir) = from_file.parent() {
        if let Ok(resolved) =
            ctx.oxc_resolver.resolve(from_dir.as_std_path(), "@types/react")
        {
            return resolved.path().to_string_lossy().into_owned();
        }
    }
    // Fallback — synthetic path that still satisfies `node_modules` filtering.
    "node_modules/@types/react/index.d.ts".to_owned()
}

/// Read `compilerOptions.paths` from a tsconfig.json and convert to `oxc_resolver`
/// alias format: `Vec<(pattern, Vec<AliasValue>)>`.
pub(super) fn read_tsconfig_paths(tsconfig: Option<&camino::Utf8Path>) -> Vec<(String, Vec<AliasValue>)> {
    let Some(path) = tsconfig else { return vec![] };
    let Ok(content) = std::fs::read_to_string(path.as_std_path()) else { return vec![] };
    let stripped = strip_json_comments(&content);
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&stripped) else {
        return vec![];
    };

    let base_url = value["compilerOptions"]["baseUrl"]
        .as_str()
        .map(|b| path.parent().unwrap_or(path).join(b));

    let paths = match value["compilerOptions"]["paths"].as_object() {
        Some(p) => p,
        None => return vec![],
    };

    paths
        .iter()
        .filter_map(|(pattern, targets)| {
            let resolved: Vec<AliasValue> = targets
                .as_array()?
                .iter()
                .filter_map(|t| t.as_str())
                .map(|t| {
                    // Remove trailing wildcards: "@lib/*" → "@lib/"
                    let t = t.trim_end_matches("/*").trim_end_matches('*');
                    let resolved_path = if let Some(base) = &base_url {
                        base.join(t)
                    } else {
                        path.parent().unwrap_or(path).join(t)
                    };
                    AliasValue::Path(resolved_path.as_std_path().to_string_lossy().into_owned())
                })
                .collect();

            let pattern_clean = pattern.trim_end_matches("/*").to_owned();
            Some((pattern_clean, resolved))
        })
        .collect()
}

/// Minimal JSON comment stripper for tsconfig files.
/// Handles single-line `//` comments; does NOT handle block `/* */` comments.
pub(super) fn strip_json_comments(s: &str) -> String {
    s.lines()
        .map(|line| {
            if let Some(idx) = line.find("//") {
                // Only strip if the `//` is not inside a string.
                // Heuristic: count unescaped `"` before idx — if even, we're outside a string.
                let before = &line[..idx];
                let quote_count = before.chars().filter(|&c| c == '"').count();
                if quote_count % 2 == 0 {
                    return &line[..idx];
                }
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}
