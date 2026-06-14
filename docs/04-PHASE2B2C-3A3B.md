# Agent: Import Map (Phase 2b)
# Model: claude-sonnet-4-6
# Runs: After Phase 1a, parallel with Phase 2a + 2c
# Owns: crates/core/src/import_map.rs

## Mission

Given a `GlobalSourceData`, build lookup structures that allow the resolver to
trace any type name → its canonical declaration file.
This is pure data transformation — no I/O.

## Key Types to Implement

```rust
// import_map.rs

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use crate::types::*;

/// Maps (consuming_file, local_name) → canonical declaration location.
/// Built once from GlobalSourceData, used read-only during resolution.
pub struct ImportResolutionMap {
    /// (file_path, local_name) → CanonicalRef
    bindings: FxHashMap<(Utf8PathBuf, CompactString), CanonicalRef>,
    /// barrel_file → [source files it re-exports everything from]
    wildcard_sources: FxHashMap<Utf8PathBuf, Vec<Utf8PathBuf>>,
}

/// Where a type name actually lives.
#[derive(Debug, Clone)]
pub struct CanonicalRef {
    /// Absolute path of the file containing the declaration
    pub file_path: Utf8PathBuf,
    /// The name in that file (may differ after `as` rename)
    pub name: CompactString,
    /// Scoped key: "${file_path}:${name}" — for GlobalSourceData lookup
    pub scoped_key: String,
}

impl ImportResolutionMap {
    /// Build from fully-merged GlobalSourceData.
    /// Called once per extraction run, before parallel resolution starts.
    pub fn build(global: &GlobalSourceData) -> Self {
        let mut bindings = FxHashMap::default();
        let mut wildcard_sources = FxHashMap::default();
        
        for (file_path, imports) in &global.import_map {
            for binding in imports {
                // For each import binding, record the local_name → specifier mapping.
                // Full resolution to absolute path happens when resolver calls
                // resolve_import() with the resolved file from oxc_resolver.
                // Here we just record the logical binding.
                let key = (file_path.clone(), binding.local_name.clone());
                // Note: we can't resolve to absolute path here without oxc_resolver.
                // The resolver will call resolve_for_file() which accepts the
                // resolved absolute path from oxc_resolver.
                bindings.insert(key, CanonicalRef {
                    file_path: file_path.clone(), // placeholder — overwritten by resolver
                    name: binding.exported_name.clone(),
                    scoped_key: String::new(), // built by resolver
                });
            }
        }
        
        for (file_path, exports) in &global.re_export_map {
            for export in exports {
                if let LexedExport::ReExportAll { .. } = export {
                    // Track wildcard sources for resolution
                }
            }
        }
        
        Self { bindings, wildcard_sources }
    }
    
    /// Given a file and a local type name, find where it was imported from.
    /// Returns None if it's a local declaration (not imported).
    pub fn find_import(&self, file: &Utf8Path, local_name: &str) -> Option<&CanonicalRef> {
        self.bindings.get(&(file.to_owned(), local_name.into()))
    }
    
    /// Resolve a re-export chain to the canonical declaration.
    /// Handles: `export { Foo as Bar } from "./types"`
    pub fn resolve_reexport(
        &self,
        barrel_file: &Utf8Path,
        exported_name: &str,
        global: &GlobalSourceData,
    ) -> Option<CanonicalRef> {
        let exports = global.re_export_map.get(barrel_file)?;
        
        for export in exports {
            match export {
                LexedExport::ReExportNamed { local_name, source_name, source_specifier, .. } => {
                    if local_name == exported_name {
                        // Found the re-export — recurse to follow the chain
                        // source_specifier needs to be resolved to absolute path
                        // by the caller (which has access to oxc_resolver)
                        return Some(CanonicalRef {
                            file_path: Utf8PathBuf::from(source_specifier),
                            name: source_name.as_str().into(),
                            scoped_key: format!("{}:{}", source_specifier, source_name),
                        });
                    }
                }
                LexedExport::ReExportAll { source_specifier, .. } => {
                    // Check if the name exists in this wildcard source
                    // Caller handles recursive resolution
                }
                _ => {}
            }
        }
        None
    }
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    // Test that barrel file re-exports resolve correctly
    // Use fixture: packages that re-export through index.ts
    
    #[test]
    fn test_named_reexport() {
        // export { Foo as Bar } from "./foo"
        // import { Bar } from "package"
        // → should resolve Bar to Foo in ./foo
    }
    
    #[test]
    fn test_wildcard_reexport() {
        // export * from "./types"
        // → all names from ./types available
    }
}
```

---

# Agent: Known Patterns (Phase 2c)
# Model: claude-sonnet-4-6
# Runs: After Phase 1a, parallel with Phase 2a + 2b
# Owns: crates/core/src/known.rs

## Mission

A single function that recognizes named generic type patterns and returns either
resolved props or a simplified PropType. No trait objects, no dynamic dispatch —
just a match arm.

## The Complete known.rs

```rust
//! Resolution of known generic type patterns.
//!
//! These are types that appear across many libraries and need special handling
//! because static analysis alone cannot fully resolve them, OR because their
//! full expansion is noise for docgen purposes.
//!
//! To add a new pattern: add a match arm. That's it.

use crate::types::*;

/// Result of recognizing a known type pattern.
pub enum KnownPatternResult {
    /// Pattern resolves to a set of props
    Props(Vec<ParsedProp>),
    /// Pattern is opaque — use this PropType directly
    Type(PropType),
    /// Pattern transparently delegates to another type name
    Alias { name: String, args: Vec<PropType> },
}

/// Attempt to resolve a named generic type as a known pattern.
///
/// Called by the resolver when it encounters a type like `SxProps<Theme>`
/// or `VariantProps<typeof buttonVariants>`.
///
/// Returns `None` if this type is not recognized — caller should continue
/// with normal resolution.
pub fn resolve_known(
    name: &str,
    args: &[PropType],
    global: &GlobalSourceData,
) -> Option<KnownPatternResult> {
    match name {
        // ── Variant systems ──────────────────────────────────────────────────
        // class-variance-authority: VariantProps<typeof buttonVariants>
        // PandaCSS: RecipeVariantProps<typeof buttonStyle>
        "VariantProps" | "RecipeVariantProps" => {
            resolve_cva_variant_props(args, global)
        }
        
        // ── MUI styling ──────────────────────────────────────────────────────
        // SxProps is a massive conditional type — surface as opaque
        "SxProps" | "SystemStyleObject" | "SystemCssProperties" => {
            Some(KnownPatternResult::Type(PropType::SxProps))
        }
        
        // ── React Aria ───────────────────────────────────────────────────────
        // RenderProps<ButtonRenderProps> → simplify to scalar className/style/children
        "RenderProps" => Some(KnownPatternResult::Props(render_props())),
        // SlotProps → { slot?: string | null }
        "SlotProps" => Some(KnownPatternResult::Props(vec![slot_prop()])),
        
        // ── Chakra / Ark ─────────────────────────────────────────────────────
        // HTMLChakraProps<'button'> → same as ComponentPropsWithoutRef<'button'>
        "HTMLChakraProps" | "HTMLArkProps" | "HTMLStyledProps" => {
            html_attrs_from_first_arg(args)
        }
        
        // ── React standard ───────────────────────────────────────────────────
        "PropsWithChildren" => props_with_children(args),
        "PropsWithRef" => props_with_ref(args),
        
        // ComponentPropsWithoutRef<'button'> or ComponentPropsWithoutRef<typeof X>
        "ComponentPropsWithoutRef" | "ComponentProps" => component_props(args, false),
        "ComponentPropsWithRef" => component_props(args, true),
        
        // ElementRef<typeof X> → opaque Ref
        "ElementRef" => {
            Some(KnownPatternResult::Type(PropType::Ref { element: None }))
        }
        
        // ── Transparent utility types (handled by resolver, but shortcircuit here)
        // These are already handled in the resolver's Omit/Pick logic,
        // but if seen as standalone names, pass through:
        "Partial" | "Required" | "Readonly" | "NonNullable" => {
            None // let resolver handle
        }
        
        // ── MUI-specific ─────────────────────────────────────────────────────
        // OverridableStringUnion requires type checker — degrade gracefully
        "OverridableStringUnion" => {
            // First arg is the base union, second is the Overrides interface
            // We can use the base union and note the extension is opaque
            if let Some(base) = args.first() {
                Some(KnownPatternResult::Type(PropType::Union(vec![
                    base.clone(),
                    PropType::Opaque {
                        raw: "/* module augmentation */".into(),
                        reason: OpaqueReason::ModuleAugmentation,
                    },
                ])))
            } else {
                Some(KnownPatternResult::Type(PropType::Opaque {
                    raw: "OverridableStringUnion".into(),
                    reason: OpaqueReason::ModuleAugmentation,
                }))
            }
        }
        
        _ => None,
    }
}

fn resolve_cva_variant_props(args: &[PropType], global: &GlobalSourceData) -> Option<KnownPatternResult> {
    // The arg is typeof buttonVariants — a Named type reference to a cva() call result.
    // We stored the cva() call variants in global.enums during extraction.
    // Look them up and return as individual props.
    //
    // If we can't find the variants (e.g. they're imported from elsewhere),
    // return an Opaque rather than failing.
    
    match args.first() {
        Some(PropType::Named { name, .. }) => {
            // Look for "${scoped_key}:${name}" in global enums
            // If found: create one LiteralUnion prop per variant key
            // If not found: opaque with good diagnostic message
            None // TODO: implement lookup
        }
        _ => Some(KnownPatternResult::Type(PropType::Opaque {
            raw: "VariantProps<...>".into(),
            reason: OpaqueReason::RuntimeDependent {
                function_name: "cva".into(),
            },
        })),
    }
}

fn render_props() -> Vec<ParsedProp> {
    // React Aria RenderProps<T> → simplify to these three props
    // Omit the function overload — noise for docgen
    vec![
        simple_prop("className", PropType::String, false,
            "CSS class name. Accepts a function receiving render state."),
        simple_prop("style", PropType::CssProperties, false,
            "Inline styles. Accepts a function receiving render state."),
        // children handled separately — always comes from ReactNode
    ]
}

fn slot_prop() -> ParsedProp {
    simple_prop(
        "slot",
        PropType::Union(vec![PropType::String, PropType::Null]),
        false,
        "Slot name for component context composition.",
    )
}

fn props_with_children(args: &[PropType]) -> Option<KnownPatternResult> {
    // PropsWithChildren<T> = T & { children?: ReactNode }
    // Delegate to T (first arg), children prop comes from resolver's HTML table
    match args.first() {
        Some(inner) => Some(KnownPatternResult::Alias {
            name: inner.raw_string(),
            args: vec![],
        }),
        None => None,
    }
}

fn props_with_ref(args: &[PropType]) -> Option<KnownPatternResult> {
    match args.first() {
        Some(inner) => Some(KnownPatternResult::Alias {
            name: inner.raw_string(),
            args: vec![],
        }),
        None => None,
    }
}

fn html_attrs_from_first_arg(args: &[PropType]) -> Option<KnownPatternResult> {
    // HTMLChakraProps<'button'> → HtmlAttributes { element: "button" }
    match args.first() {
        Some(PropType::StringLiteral(element)) => {
            Some(KnownPatternResult::Type(PropType::HtmlAttributes {
                element: element.to_lowercase(),
                omitted: vec![],
            }))
        }
        _ => None,
    }
}

fn component_props(args: &[PropType], _include_ref: bool) -> Option<KnownPatternResult> {
    match args.first() {
        Some(PropType::StringLiteral(element)) => {
            // ComponentPropsWithoutRef<'button'>
            Some(KnownPatternResult::Type(PropType::HtmlAttributes {
                element: element.to_lowercase(),
                omitted: vec![],
            }))
        }
        Some(PropType::Named { name, .. }) => {
            // ComponentPropsWithoutRef<typeof X> — delegate to X's props
            Some(KnownPatternResult::Alias { name: name.to_string(), args: vec![] })
        }
        _ => None,
    }
}

fn simple_prop(name: &str, prop_type: PropType, required: bool, description: &str) -> ParsedProp {
    ParsedProp {
        name: name.to_owned(),
        prop_type,
        required,
        default_value: None,
        description: description.to_owned(),
        tags: Default::default(),
        parent: None,
        declarations: vec![],
    }
}
```

---

# Agent: Resolver (Phase 3a)
# Model: claude-sonnet-4-6
# Runs: After Phase 2 complete
# Owns: crates/core/src/resolver.rs, crates/core/src/cache.rs

## Mission

Given GlobalSourceData and an ImportResolutionMap, resolve a `ComponentMapping`
to a complete `ComponentEntry` with fully-typed `PropType` props.

This is the hardest logic in the codebase. Be methodical.

## resolver.rs Structure

```rust
use std::collections::BTreeMap;
use std::sync::Arc;
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use oxc_resolver::{Resolver, ResolveOptions};
use rustc_hash::FxHashSet;

use crate::types::*;
use crate::import_map::ImportResolutionMap;
use crate::react_types;
use crate::known::{resolve_known, KnownPatternResult};

/// Maximum depth for recursive type resolution.
const MAX_DEPTH: u8 = 20;

/// Resolution context — read-only, shared across rayon threads via Arc.
pub struct ResolutionContext {
    pub global: Arc<GlobalSourceData>,
    pub import_map: Arc<ImportResolutionMap>,
    pub oxc_resolver: Arc<Resolver>,
    pub react_version: ReactVersion,
}

impl ResolutionContext {
    pub fn new(global: Arc<GlobalSourceData>, options: &PipelineOptions) -> Self {
        let resolve_options = ResolveOptions {
            condition_names: vec!["types".into(), "import".into(), "require".into(), "default".into()],
            main_fields: vec!["types".into(), "typings".into(), "module".into(), "main".into()],
            extensions: vec![".ts".into(), ".tsx".into(), ".d.ts".into(), ".js".into()],
            ..ResolveOptions::default()
        };
        
        Self {
            import_map: Arc::new(ImportResolutionMap::build(&global)),
            global,
            oxc_resolver: Arc::new(Resolver::new(resolve_options)),
            react_version: options.react_version.clone(),
        }
    }
}

/// Resolve a component mapping to a complete ComponentEntry.
/// Called in parallel via rayon — must be Send + Sync.
pub fn resolve_component(
    mapping: &ComponentMapping,
    ctx: &ResolutionContext,
) -> (ComponentEntry, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut visited = FxHashSet::default();
    
    let chain = resolve_props_chain(
        &mapping.props_type_name,
        &mapping.props_type_args,
        &mapping.file_path,
        ctx,
        &mut visited,
        0,
        &mut diagnostics,
    );
    
    let props: BTreeMap<String, ParsedProp> = chain.props
        .into_iter()
        .map(|p| (p.name.clone(), p))
        .collect();
    
    (
        ComponentEntry {
            display_name: mapping.component_name.clone(),
            file_path: mapping.file_path.clone(),
            description: mapping.description.clone(),
            props,
            html_element: chain.html_element,
            omitted_html_props: chain.omitted_html_props,
            composes: chain.composes,
            tags: mapping.tags.clone(),
            methods: vec![],
        },
        diagnostics,
    )
}

/// Intermediate result of chain resolution.
struct ResolvedChain {
    props: Vec<ParsedProp>,
    html_element: Option<String>,
    omitted_html_props: Vec<String>,
    composes: Vec<String>,
}

fn resolve_props_chain(
    type_name: &str,
    type_args: &[String],
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    visited: &mut FxHashSet<String>,
    depth: u8,
    diagnostics: &mut Vec<Diagnostic>,
) -> ResolvedChain {
    if depth > MAX_DEPTH {
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Warning,
            message: format!("Max resolution depth exceeded for type '{}'", type_name),
            file: Some(consuming_file.to_string()),
            line: None, column: None,
            help: Some("This may indicate a circular type reference".into()),
            code: DiagnosticCode::MaxDepthExceeded,
        });
        return ResolvedChain::empty_with_compose(type_name.to_owned());
    }
    
    // Dedup — prevent infinite recursion on circular extends
    let visit_key = format!("{}:{}", consuming_file, type_name);
    if !visited.insert(visit_key) {
        return ResolvedChain::default();
    }
    
    // Step 1: Try known pattern resolution (cva, SxProps, RenderProps, etc.)
    let prop_type = parse_raw_type(type_name, type_args);
    if let PropType::Named { name, args } = &prop_type {
        if let Some(result) = resolve_known(name, args, &ctx.global) {
            return match result {
                KnownPatternResult::Props(props) => ResolvedChain { props, ..Default::default() },
                KnownPatternResult::Type(pt) => ResolvedChain::from_opaque_type(type_name, pt),
                KnownPatternResult::Alias { name, args } => {
                    resolve_props_chain(&name, &args.iter().map(|a| a.raw_string()).collect::<Vec<_>>(),
                        consuming_file, ctx, visited, depth + 1, diagnostics)
                }
            };
        }
    }
    
    // Step 2: Resolve import to find the canonical file
    let (canonical_file, canonical_name) = resolve_to_canonical(
        type_name, consuming_file, ctx, diagnostics
    ).unwrap_or_else(|| (consuming_file.to_owned(), type_name.to_owned()));
    
    let scoped_key = format!("{}:{}", canonical_file, canonical_name);
    
    // Step 3: Handle type aliases (Omit, Pick, Partial, etc.)
    if let Some(alias) = ctx.global.type_aliases.get(&scoped_key) {
        return resolve_type_alias(alias, consuming_file, ctx, visited, depth, diagnostics);
    }
    
    // Step 4: Handle interfaces (primary case)
    if let Some(iface) = ctx.global.interfaces.get(&scoped_key) {
        return resolve_interface(iface, type_args, consuming_file, ctx, visited, depth, diagnostics);
    }
    
    // Step 5: Unresolvable — degrade gracefully
    diagnostics.push(Diagnostic {
        severity: DiagnosticSeverity::Warning,
        message: format!("Cannot resolve type '{}' in '{}'", type_name, consuming_file),
        file: Some(consuming_file.to_string()),
        line: None, column: None,
        help: Some("Type may be in an unresolvable cross-package location".into()),
        code: DiagnosticCode::UnresolvableImport,
    });
    ResolvedChain::empty_with_compose(type_name.to_owned())
}

fn resolve_interface(
    iface: &CollectedInterface,
    type_args: &[String],
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    visited: &mut FxHashSet<String>,
    depth: u8,
    diagnostics: &mut Vec<Diagnostic>,
) -> ResolvedChain {
    let mut chain = ResolvedChain::default();
    
    // Resolve extends first (parent props come before own props)
    for extends_ref in &iface.extends {
        let extends_chain = resolve_extends_ref(
            extends_ref, &iface.file_path, ctx, visited, depth + 1, diagnostics
        );
        // Smart merge: if same prop name exists, own prop wins
        chain.merge_parent(extends_chain, &iface.name, &iface.file_path);
    }
    
    // Resolve own props
    for raw_prop in &iface.props {
        let prop_type = resolve_raw_type(&raw_prop.raw_type, type_args, &iface.file_path, ctx, visited, depth, diagnostics);
        chain.props.push(ParsedProp {
            name: raw_prop.name.clone(),
            prop_type,
            required: raw_prop.required,
            default_value: None, // set by pipeline from destructured params
            description: raw_prop.description.clone(),
            tags: raw_prop.tags.clone(),
            parent: Some(PropParent {
                name: iface.name.to_string(),
                file_name: iface.file_path.to_string(),
            }),
            declarations: vec![PropParent {
                name: iface.name.to_string(),
                file_name: iface.file_path.to_string(),
            }],
        });
    }
    
    chain
}

fn resolve_extends_ref(
    extends_ref: &ExtendsRef,
    iface_file: &Utf8Path,
    ctx: &ResolutionContext,
    visited: &mut FxHashSet<String>,
    depth: u8,
    diagnostics: &mut Vec<Diagnostic>,
) -> ResolvedChain {
    match extends_ref {
        ExtendsRef::Builtin { name, element, type_args } => {
            // HTML element attrs — look up in react_types tables
            if let Some(element) = element {
                ResolvedChain {
                    props: vec![], // HTML attrs resolved at serialization time
                    html_element: Some(element.clone()),
                    omitted_html_props: vec![],
                    composes: vec![],
                }
            } else {
                // Other builtins (AriaAttributes etc.) — get from baked-in table
                resolve_props_chain(name, type_args, iface_file, ctx, visited, depth, diagnostics)
            }
        }
        ExtendsRef::SameFile { name, type_args } => {
            resolve_props_chain(name, type_args, iface_file, ctx, visited, depth, diagnostics)
        }
        ExtendsRef::Imported { local_name, type_args, source_specifier } => {
            // Use oxc_resolver to find the actual file
            let resolved_file = resolve_import_specifier(
                source_specifier.as_deref().unwrap_or(local_name),
                iface_file,
                ctx,
                diagnostics,
            );
            let file = resolved_file.as_deref().unwrap_or(iface_file);
            resolve_props_chain(local_name, type_args, file, ctx, visited, depth, diagnostics)
        }
    }
}

fn resolve_import_specifier(
    specifier: &str,
    from_file: &Utf8Path,
    ctx: &ResolutionContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Utf8PathBuf> {
    let from_dir = from_file.parent()?;
    match ctx.oxc_resolver.resolve(from_dir.as_std_path(), specifier) {
        Ok(resolved) => Utf8PathBuf::from_path_buf(resolved.path().to_owned()).ok(),
        Err(e) => {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                message: format!("Cannot resolve '{}' from '{}'", specifier, from_file),
                file: Some(from_file.to_string()),
                line: None, column: None,
                help: Some(format!("Resolution error: {}", e)),
                code: DiagnosticCode::UnresolvableImport,
            });
            None
        }
    }
}
```

## 1. Use CollectedType, not raw string parsing

The resolver receives `RawProp.collected_type: CollectedType` (a structured enum, not a string). Pattern-match on it directly. The `resolve_raw_type` function in the spec above becomes `resolve_collected_type(ct: &CollectedType, ...) -> PropType`.

Key mapping:

| CollectedType variant | PropType result |
|---|---|
| `Named { name, args }` | Look up in `known.rs` first, then resolve imports, then check interfaces/aliases |
| `Union(members)` | Resolve each member → `PropType::Union` |
| `Intersection(members)` | Resolve each, merge props for interface intersections → `PropType::Intersection` |
| `Object(fields)` | `PropType::Object` with each field resolved |
| `Array(inner)` | `PropType::Array(resolve_collected_type(inner))` |
| `IndexedAccess { obj, key }` | Call `resolve_indexed_access(obj, key)`; degrade to `PropType::Opaque { reason: OpaqueReason::IndexedAccess { expression } }` |
| `TemplateLiteral(parts)` | Call `expand_template_literal(parts)`; degrade to `PropType::Opaque { reason: OpaqueReason::TemplateLiteral { expression } }` |
| `Conditional { .. }` \| `Mapped { .. }` | `PropType::Opaque { reason: OpaqueReason::ConditionalType }` — flag for typescript-go |
| `Function { params, return_type }` | Detect render prop pattern: single named param → `PropType::RenderProp { state_type, value_type }`, else `PropType::EventHandler { event_type: raw_string }` |
| `TypeOf(name)` | Look up in `global.enums` for `cva()` results |
| Primitives | Map 1:1 to `PropType` primitives |
| `Raw(s)` | Attempt to parse as `Named` type, degrade to `Opaque` |

## 2. Indexed access resolution table

Add `resolve_indexed_access` as a fallback lookup before degrading to `Opaque`:

```rust
fn resolve_indexed_access(obj: &CollectedType, key: &CollectedType) -> Option<PropType> {
    let obj_name = match obj {
        CollectedType::Named { name, .. } => name.as_str(),
        _ => return None,
    };
    let key_str = match key {
        CollectedType::StringLiteral(s) => s.as_str(),
        _ => return None,
    };
    match (obj_name, key_str) {
        ("CSSProperties" | "React.CSSProperties", "zIndex") => Some(PropType::Number),
        ("CSSProperties" | "React.CSSProperties", _) => Some(PropType::String),
        ("HTMLAttributes" | "React.HTMLAttributes", "className") => Some(PropType::String),
        ("HTMLAttributes" | "React.HTMLAttributes", "tabIndex") => Some(PropType::Number),
        ("HTMLAttributes" | "React.HTMLAttributes", "style") => Some(PropType::CssProperties),
        ("HTMLAttributes" | "React.HTMLAttributes", "id") => Some(PropType::String),
        _ => None,
    }
}
```

If `resolve_indexed_access` returns `None`, the caller degrades to:
```rust
PropType::Opaque { reason: OpaqueReason::IndexedAccess { expression } }
```

## 3. Template literal expansion

```rust
fn expand_template_literal(parts: &[CollectedType], ctx: &ResolutionContext) -> Option<PropType> {
    // Try to expand: `compact-${MantineSize}` where MantineSize = "xs"|"sm"|...
    // If any part is a Named type that resolves to a string literal union, expand it.
    // Result: Union of all combinations (cartesian product of literal parts).
    // If not fully expandable: return None (caller degrades to Opaque).
}
```

Callers that receive `None` produce:
```rust
PropType::Opaque { reason: OpaqueReason::TemplateLiteral { expression } }
```

## 4. Discriminated union handling

When resolving `CollectedTypeAlias::Union { members }` where all members are `Named` types that resolve to interfaces:

1. Resolve all member interfaces.
2. Find the discriminant: the prop whose type differs across all members as string literals.
3. Merge all other props — earlier member props take precedence by order.
4. Set `ComponentEntry.discriminant_prop = Some(discriminant_name)`.
5. The discriminant prop's type becomes the union of all discriminant literal values.

## 5. `(string & {})` normalization

In `resolve_collected_type`, when handling `CollectedType::Intersection(members)`:

- If one member is `CollectedType::String` and another is `CollectedType::Object([])` (empty object type — `{}`), return `PropType::String`.
- This handles the `(string & {})` pattern from Mantine, which TypeScript uses to keep a string type open while preserving autocomplete for known literals.

## 6. Inheritance chain building

During `resolve_interface`, as we resolve `extends` clauses, build `InheritedLayer` entries:

```rust
fn resolve_extends_ref(...) -> (ResolvedChain, Option<InheritedLayer>) {
    // Returns props chain + the layer metadata.
    // InheritedLayer.file_name = the actual .d.ts file path (for RDT propFilter compat).
    // InheritedLayer.html_element = from react_types::html_element_for().
    // InheritedLayer.omitted = keys removed by Omit at this layer.
    // InheritedLayer.total_props = props.len() after omissions.
}
```

Push `InheritedLayer` entries to `ResolvedChain.inheritance: Vec<InheritedLayer>`.

After full resolution, copy `chain.inheritance` to `ComponentEntry.inheritance`.

## 7. Notable props population

After resolving all inherited props, populate `ComponentEntry.notable_inherited`:

- For HTML element layers: call `react_types::notable_html_attrs(element)` to get the curated list. Filter the resolved HTML props to only those in the notable list.
- For non-HTML layers (`ButtonBaseProps`, `ThemingProps`, etc.): add all their own props to `notable_inherited` — they are typically small and intentional.

## 8. Default value resolution

In the resolver, when building `ParsedProp`:

```rust
// Code default takes precedence over JSDoc @default
let code_default = mapping.param_defaults.get(&raw_prop.name);
let jsdoc_default = raw_prop.tags.get("default").or_else(|| raw_prop.tags.get("defaultValue"));

let default_value = match (code_default, jsdoc_default.map(|s| s.trim())) {
    (Some(code), Some(jsdoc)) if code.value.trim_matches('"') != jsdoc => {
        // Discrepancy: emit Info diagnostic, use code value
        diagnostics.push(Diagnostic { severity: DiagnosticSeverity::Info, code: DiagnosticCode::JsDocDefaultMismatch, ... });
        Some(DefaultValue { value: code.value.clone(), computed: code.computed })
    }
    (Some(code), _) => Some(DefaultValue { value: code.value.clone(), computed: code.computed }),
    (None, Some(jsdoc)) => Some(DefaultValue { value: jsdoc.to_owned(), computed: false }),
    (None, None) => None,
};
```

## 9. tsconfig path aliases

At `ResolutionContext::new()`, if `options.tsconfig_path` is set (or auto-detected), read it and configure `oxc_resolver` with `paths` and `baseUrl`:

```rust
let resolve_options = ResolveOptions {
    alias: read_tsconfig_paths(options.tsconfig_path.as_deref()),
    // ... existing options
};
```

Add `read_tsconfig_paths(tsconfig: Option<&Utf8Path>) -> Vec<(String, Vec<PathBuf>)>` that reads `compilerOptions.paths` from the tsconfig JSON using `serde_json::from_str`. Handle `extends` in tsconfig (follow one level deep).

## 10. `declare const X: ForwardRefExoticComponent<P>` component detection

The extractor (Phase 2a follow-up) adds detection of the pattern:
```ts
declare const Button: React.ForwardRefExoticComponent<ButtonProps & React.RefAttributes<HTMLButtonElement>>
```

The resolver needs no changes for this — it just handles `ComponentMapping` entries that come from `.d.ts` files the same as any other mapping. This is purely an extractor change.

---

# Agent: Pipeline (Phase 3b)
# Model: claude-sonnet-4-6
# Runs: After Phase 2 complete, parallel with Phase 3a
# Owns: crates/core/src/pipeline.rs

## Mission

Orchestrate the full extraction: discover files, parse in parallel with rayon,
merge GlobalSourceData, resolve in parallel, collect output.
Also manages the DTS cache and incremental watch state.

## pipeline.rs Structure

```rust
use std::sync::Arc;
use std::time::Instant;
use camino::{Utf8Path, Utf8PathBuf};
use dashmap::DashMap;
use arc_swap::ArcSwap;
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use ignore::WalkBuilder;

use crate::types::*;
use crate::extractor::parse_file;
use crate::resolver::{resolve_component, ResolutionContext};
use crate::cache::DtsCache;

/// Configuration for a single extraction run.
#[derive(Debug, Clone)]
pub struct PipelineOptions {
    /// Source directories to scan
    pub src_dirs: Vec<Utf8PathBuf>,
    /// Extra patterns to exclude (on top of built-in: stories, tests, snapshots)
    pub exclude_patterns: Vec<String>,
    /// Component name prefixes to skip
    pub exclude_prefixes: Vec<String>,
    /// React version — default: auto-detected
    pub react_version: crate::react_types::ReactVersion,
    /// Whether to resolve cross-package types
    pub cross_package: bool,
    /// PandaCSS generated output dir, if applicable
    pub pandacss_outdir: Option<Utf8PathBuf>,
    /// Extra function names to treat as cva-like variant functions
    pub variant_functions: Vec<String>,
    /// Skip HTML props filter
    pub skip_html_props: bool,
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self {
            src_dirs: vec![Utf8PathBuf::from("./src")],
            exclude_patterns: vec![],
            exclude_prefixes: vec![],
            react_version: crate::react_types::REACT_19,
            cross_package: true,
            pandacss_outdir: None,
            variant_functions: vec!["cva".into(), "tv".into(), "defineRecipe".into()],
            skip_html_props: false,
        }
    }
}

/// Stateless extraction — no incremental state.
/// Suitable for CLI and NAPI cold extraction.
pub fn extract(options: &PipelineOptions) -> ExtractionOutput {
    let start = Instant::now();
    let mut diagnostics = Vec::new();
    let cache = DtsCache::load_from_disk();
    
    // ── Phase 1: Discover all files
    let src_files = discover_files(&options.src_dirs, &options.exclude_patterns);
    
    // ── Phase 2: Parse all source files in parallel
    let source_data_vec: Vec<(Utf8PathBuf, SourceData)> = src_files
        .par_iter()
        .map(|path| {
            let source = std::fs::read_to_string(path)
                .unwrap_or_default();
            (path.clone(), parse_file(path, &source))
        })
        .collect();
    
    // ── Phase 3: Parse cross-package .d.ts files (demand-driven, cached)
    // Uses the import specifiers from Phase 2 + oxc_resolver
    // Handled lazily during resolution — not a separate phase
    
    // ── Phase 4: Merge into GlobalSourceData (sequential, fast)
    let mut global = GlobalSourceData::default();
    for (path, data) in source_data_vec {
        global.merge(&path, data);
    }
    let global = Arc::new(global);
    
    // ── Phase 5: Build resolution context
    let ctx = ResolutionContext::new(global.clone(), options);
    let ctx = Arc::new(ctx);
    
    // ── Phase 6: Resolve all components in parallel
    let results: Vec<(ComponentEntry, Vec<Diagnostic>)> = global
        .component_mappings
        .par_iter()
        .filter(|m| !should_skip_component(&m.component_name, &options.exclude_prefixes))
        .map(|mapping| resolve_component(mapping, &ctx))
        .collect();
    
    // ── Phase 7: Collect output
    let mut components = std::collections::BTreeMap::new();
    for (entry, diags) in results {
        components.insert(entry.display_name.clone(), entry);
        diagnostics.extend(diags);
    }
    
    // Save cache for next run
    cache.save_to_disk();
    
    ExtractionOutput {
        components,
        enums: collect_public_enums(&global),
        diagnostics,
        stats: ExtractionStats {
            components_extracted: components.len() as u32,
            duration_ms: start.elapsed().as_millis() as u64,
            files_parsed: src_files.len() as u32,
            ..Default::default()
        },
    }
}

fn discover_files(src_dirs: &[Utf8PathBuf], extra_excludes: &[String]) -> Vec<Utf8PathBuf> {
    let mut files = Vec::new();
    
    for dir in src_dirs {
        let walk = WalkBuilder::new(dir.as_std_path())
            .hidden(false)
            .git_ignore(true)
            .build();
        
        for entry in walk.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            
            if !matches!(ext, "ts" | "tsx") {
                continue;
            }
            
            let path_str = path.to_str().unwrap_or("");
            
            // Built-in excludes
            if path_str.contains(".stories.")
                || path_str.contains(".test.")
                || path_str.contains(".spec.")
                || path_str.contains("__snapshots__")
                || path_str.contains("node_modules")
            {
                continue;
            }
            
            // User excludes
            if extra_excludes.iter().any(|p| path_str.contains(p.as_str())) {
                continue;
            }
            
            if let Ok(utf8) = Utf8PathBuf::from_path_buf(path.to_owned()) {
                files.push(utf8);
            }
        }
    }
    
    files.sort(); // deterministic ordering
    files
}

fn should_skip_component(name: &str, exclude_prefixes: &[String]) -> bool {
    exclude_prefixes.iter().any(|p| name.starts_with(p.as_str()))
}

/// Session state for incremental watch-mode extraction.
/// Stored globally for NAPI session management.
pub struct WatchSession {
    pub options: PipelineOptions,
    /// Current GlobalSourceData — swapped atomically on updates
    pub global: ArcSwap<GlobalSourceData>,
    /// Reverse dependency graph: file → files that import it
    pub reverse_deps: DashMap<Utf8PathBuf, FxHashSet<Utf8PathBuf>>,
    /// Per-file SourceData cache
    pub source_cache: DashMap<Utf8PathBuf, SourceData>,
    /// Cached resolved components
    pub component_cache: DashMap<String, ComponentEntry>,
}

impl WatchSession {
    pub fn new(options: PipelineOptions) -> Self {
        Self {
            options,
            global: ArcSwap::new(Arc::new(GlobalSourceData::default())),
            reverse_deps: DashMap::new(),
            source_cache: DashMap::new(),
            component_cache: DashMap::new(),
        }
    }
    
    /// Handle a single file change — re-extract only affected components.
    pub fn update_file(&self, changed_file: &Utf8Path) -> IncrementalUpdate {
        // 1. Re-parse changed file
        // 2. Find all files that transitively import it (reverse_deps walk)
        // 3. Re-resolve only affected components
        // 4. Atomically swap GlobalSourceData for changed keys
        // 5. Return updated component entries
        todo!("implement incremental update")
    }
}

pub struct IncrementalUpdate {
    pub updated_components: Vec<ComponentEntry>,
    pub affected_files: Vec<Utf8PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    pub duration_ms: u64,
}
```
