# Agent: Types (Phase 1a)
# Model: claude-sonnet-4-6
# Runs: After Phase 0, parallel with Phase 1b (Fixtures)
# Owns: crates/core/src/types.rs ONLY

## Mission

Define every data type shared across the system. This is the contract every other agent depends on. Correctness and completeness here is more important than anything else. No logic — only type definitions.

## Acceptance Criteria

- `cargo build -p oxc-react-docgen-core` succeeds
- All types implement: Debug, Clone, PartialEq, Serialize, Deserialize
- No `std::collections::HashMap` anywhere — use `FxHashMap` or `BTreeMap`
- No `String` for type names in hot-path structs — use `CompactString`
- All public types have doc comments

## The Full types.rs

```rust
//! Shared data types for oxc-react-docgen.
//!
//! These types form the contract between pipeline stages.
//! Rules:
//! - `CompactString` for names/type strings (avoids heap alloc for short strings)
//! - `BTreeMap` for JSON-facing output (deterministic key ordering)
//! - `FxHashMap` for internal lookup maps (performance)
//! - All types are `Send + Sync` — required for rayon and NAPI

use camino::Utf8PathBuf;
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ─── Type Aliases ────────────────────────────────────────────────────────────

/// Compact string used for names, type strings — avoids heap alloc under 24 bytes
pub type TypeName = CompactString;

// ─── Core Output Types ───────────────────────────────────────────────────────

/// The complete extraction output — top-level return type of the pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionOutput {
    /// Extracted component entries, keyed by component name
    pub components: BTreeMap<String, ComponentEntry>,
    /// Enum definitions discovered during extraction
    pub enums: BTreeMap<String, Vec<EnumEntry>>,
    /// Non-fatal issues encountered during extraction
    pub diagnostics: Vec<Diagnostic>,
    /// Extraction statistics
    pub stats: ExtractionStats,
}

/// A single React component with its resolved props.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentEntry {
    /// Display name of the component
    pub display_name: String,
    /// Absolute path of the file containing the component
    pub file_path: Utf8PathBuf,
    /// Component-level JSDoc description
    pub description: String,
    /// Resolved props, keyed by prop name
    pub props: BTreeMap<String, ParsedProp>,
    /// HTML element this component renders as, if known
    pub html_element: Option<String>,
    /// HTML attribute keys omitted from inherited HTML attributes
    pub omitted_html_props: Vec<String>,
    /// Type names that could not be resolved (react-docgen compat)
    pub composes: Vec<String>,
    /// JSDoc @tags on the component (e.g. @deprecated, @since)
    pub tags: BTreeMap<String, String>,
    /// Always empty for functional components; present for RDT compat
    pub methods: Vec<()>,
}

/// A single resolved prop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedProp {
    /// Prop name
    pub name: String,
    /// Resolved semantic type
    #[serde(rename = "type")]
    pub prop_type: PropType,
    /// Whether the prop is required
    pub required: bool,
    /// Default value if known (from destructured params or JSDoc)
    pub default_value: Option<DefaultValue>,
    /// JSDoc description of the prop
    pub description: String,
    /// JSDoc @tags on the prop (@deprecated, @since, @see, etc.)
    pub tags: BTreeMap<String, String>,
    /// Interface/type where this prop was originally declared
    pub parent: Option<PropParent>,
    /// All declarations of this prop name (for overloads/merging)
    pub declarations: Vec<PropParent>,
}

/// Default value for a prop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultValue {
    /// String representation of the default value
    pub value: String,
    /// Whether this is a computed value we couldn't fully evaluate
    pub computed: bool,
}

/// Where a prop was declared — enables RDT propFilter compatibility.
/// The canonical RDT pattern: `prop => !prop.parent.fileName.includes('node_modules')`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropParent {
    /// Name of the interface/type alias that declared this prop
    pub name: String,
    /// Absolute path of the file — matches RDT's `fileName` field
    pub file_name: String,
}

// ─── PropType — The Canonical Semantic Type Representation ───────────────────

/// Semantic type of a prop. Rich enough to produce any output format.
/// Use match exhaustively — no `_` fallthrough in serializers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PropType {
    // ── Primitives
    String,
    Number,
    Boolean,
    Null,
    Undefined,
    Any,
    Never,
    Unknown,
    Void,

    // ── Literals
    StringLiteral(String),
    NumberLiteral(f64),
    BoolLiteral(bool),

    // ── Composites
    Union(Vec<PropType>),
    Intersection(Vec<PropType>),
    Array(Box<PropType>),
    Tuple(Vec<PropType>),
    Object(Vec<ObjectField>),

    // ── Named reference (post-resolution)
    Named {
        name: TypeName,
        /// Generic args, already resolved to PropType
        args: Vec<PropType>,
    },

    // ── React-specific terminals
    /// ReactNode, ReactElement, JSX.Element — all treated the same for docgen
    ReactNode,
    /// React.CSSProperties
    CssProperties,
    /// Event handler: (e: MouseEvent) => void etc.
    EventHandler {
        /// "MouseEvent", "ChangeEvent<HTMLInputElement>", etc.
        event_type: String,
    },
    /// Ref<T> / RefObject<T> / ForwardedRef<T>
    Ref {
        /// Inner element type if known
        element: Option<String>,
    },
    /// React.ElementType — component can render as any element
    /// Used for: `as?: ElementType`, `component?: ElementType`
    ElementType,

    // ── HTML attribute inheritance
    /// All attributes of an HTML element, minus omitted keys.
    /// Produced when we see ComponentPropsWithoutRef<'button'> etc.
    HtmlAttributes {
        element: String,
        omitted: Vec<String>,
    },

    // ── Variant systems (statically resolved)
    /// Result of CvaVariantPropsHandler — the wrapper is fully dissolved.
    /// Each member is a string literal value.
    LiteralUnion {
        members: Vec<String>,
        /// true if this prop has a defaultVariant
        has_default: bool,
    },

    // ── Known opaque patterns (cannot/should not be expanded)
    /// MUI SxProps, SystemStyleObject etc. — complex, not user-facing props
    SxProps,

    // ── Unresolvable — graceful degradation
    Opaque {
        /// Original type string as written in source
        raw: String,
        reason: OpaqueReason,
    },
}

impl PropType {
    /// True if this type is a pure literal union (all members are literals).
    /// Used by serializers to choose between "enum" and "union" in RDT output.
    pub fn is_literal_union(&self) -> bool {
        match self {
            PropType::Union(members) => members.iter().all(|m| {
                matches!(
                    m,
                    PropType::StringLiteral(_)
                        | PropType::NumberLiteral(_)
                        | PropType::BoolLiteral(_)
                )
            }),
            PropType::LiteralUnion { .. } => true,
            _ => false,
        }
    }

    /// Raw type string for display (e.g. in RDT PropItemType.raw)
    pub fn raw_string(&self) -> String {
        match self {
            PropType::String => "string".into(),
            PropType::Number => "number".into(),
            PropType::Boolean => "boolean".into(),
            PropType::Null => "null".into(),
            PropType::Undefined => "undefined".into(),
            PropType::Any => "any".into(),
            PropType::Never => "never".into(),
            PropType::Unknown => "unknown".into(),
            PropType::Void => "void".into(),
            PropType::StringLiteral(s) => format!("\"{}\"", s),
            PropType::NumberLiteral(n) => n.to_string(),
            PropType::BoolLiteral(b) => b.to_string(),
            PropType::Union(members) => {
                members.iter().map(|m| m.raw_string()).collect::<Vec<_>>().join(" | ")
            }
            PropType::Intersection(members) => {
                members.iter().map(|m| m.raw_string()).collect::<Vec<_>>().join(" & ")
            }
            PropType::Array(inner) => format!("{}[]", inner.raw_string()),
            PropType::LiteralUnion { members, .. } => members.join(" | "),
            PropType::Named { name, args } if args.is_empty() => name.to_string(),
            PropType::Named { name, args } => {
                let args_str = args.iter().map(|a| a.raw_string()).collect::<Vec<_>>().join(", ");
                format!("{}<{}>", name, args_str)
            }
            PropType::ReactNode => "ReactNode".into(),
            PropType::CssProperties => "CSSProperties".into(),
            PropType::EventHandler { event_type } => format!("({}: {}) => void", "e", event_type),
            PropType::Ref { element: Some(e) } => format!("Ref<{}>", e),
            PropType::Ref { element: None } => "Ref<unknown>".into(),
            PropType::ElementType => "ElementType".into(),
            PropType::HtmlAttributes { element, .. } => {
                format!("{}HTMLAttributes", element_to_type_name(element))
            }
            PropType::SxProps => "SxProps".into(),
            PropType::Opaque { raw, .. } => raw.clone(),
            PropType::Object(_) => "object".into(),
            PropType::Tuple(_) => "tuple".into(),
            PropType::CssProperties => "CSSProperties".into(),
        }
    }
}

fn element_to_type_name(element: &str) -> &'static str {
    match element {
        "button" => "Button",
        "input" => "Input",
        "textarea" => "Textarea",
        "select" => "Select",
        "a" => "Anchor",
        "form" => "Form",
        "label" => "Label",
        "img" => "Img",
        "div" | "span" | "p" | "h1" | "h2" | "h3" | _ => "HTML",
    }
}

/// Why a type could not be resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpaqueReason {
    /// TypeScript conditional type — needs type checker
    ConditionalType,
    /// TypeScript mapped type — needs type checker
    MappedType,
    /// Module augmentation result — needs type checker
    ModuleAugmentation,
    /// Runtime-dependent: cva(), tv(), etc. where args not statically visible
    RuntimeDependent { function_name: String },
    /// Import could not be resolved to a file
    UnresolvableImport { specifier: String },
    /// PandaCSS styled-system not generated yet
    PandaCodegenMissing,
    /// Maximum resolution depth exceeded (circular or too deep)
    DepthExceeded,
}

/// A field in an object type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectField {
    pub name: String,
    pub prop_type: PropType,
    pub required: bool,
    pub description: String,
}

// ─── Enum Types ───────────────────────────────────────────────────────────────

/// An enum-like constant discovered during extraction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumEntry {
    pub name: String,
    pub value: EnumValue,
    pub description: String,
}

/// The value of an enum entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum EnumValue {
    String(String),
    Number(f64),
    Bool(bool),
}

// ─── Source Data (collected during Phase 0-2) ────────────────────────────────

/// Raw data collected from parsing a single file.
/// Owned data only — no AST references. Safe for rayon + arc.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceData {
    /// Interfaces found in this file.
    /// Key: "${absolute_file_path}:${interface_name}"
    pub interfaces: FxHashMap<String, CollectedInterface>,

    /// Type aliases found in this file.
    /// Key: "${absolute_file_path}:${type_name}"
    pub type_aliases: FxHashMap<String, CollectedTypeAlias>,

    /// Enum-like values found in this file.
    /// Key: "${absolute_file_path}:${name}"
    pub enums: FxHashMap<String, Vec<EnumEntry>>,

    /// Component → prop type mappings found in this file.
    /// Only populated for .tsx files.
    pub component_mappings: Vec<ComponentMapping>,

    /// Import bindings in this file — local name → source
    pub imports: Vec<ImportBinding>,

    /// Re-exports from this file
    pub exports: Vec<LexedExport>,
}

/// A collected interface declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedInterface {
    /// Scoped key: "${file_path}:${name}"
    pub scoped_key: String,
    pub name: TypeName,
    pub file_path: Utf8PathBuf,
    pub props: Vec<RawProp>,
    pub extends: Vec<ExtendsRef>,
    pub description: String,
    pub tags: BTreeMap<String, String>,
}

/// A raw prop — types not yet resolved to PropType.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawProp {
    pub name: String,
    /// Raw type string — e.g. "ButtonHTMLAttributes<HTMLButtonElement>"
    pub raw_type: String,
    pub required: bool,
    pub description: String,
    pub tags: BTreeMap<String, String>,
    /// Byte span in source file — used for miette diagnostics
    pub span_start: u32,
    pub span_end: u32,
}

/// An extends reference in an interface declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtendsRef {
    /// interface Foo extends Bar — Bar defined in SAME file
    SameFile { name: TypeName, type_args: Vec<String> },
    /// interface Foo extends Bar — Bar came from an import
    Imported {
        local_name: TypeName,
        type_args: Vec<String>,
        /// The import specifier this name came from, if determinable
        source_specifier: Option<String>,
    },
    /// interface Foo extends ButtonHTMLAttributes<HTMLButtonElement>
    /// Recognized as baked-in React/DOM type — no file lookup needed
    Builtin {
        name: TypeName,
        /// Resolved HTML element: "button", "input", etc.
        element: Option<String>,
        type_args: Vec<String>,
    },
}

/// A collected type alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollectedTypeAlias {
    Omit { base_name: TypeName, omitted_keys: Vec<String>, file_path: Utf8PathBuf },
    Pick { base_name: TypeName, picked_keys: Vec<String>, file_path: Utf8PathBuf },
    Partial { base_name: TypeName, file_path: Utf8PathBuf },
    Required { base_name: TypeName, file_path: Utf8PathBuf },
    Union { members: Vec<String>, file_path: Utf8PathBuf },
    Intersection { members: Vec<String>, file_path: Utf8PathBuf },
    LiteralUnion { members: Vec<String>, file_path: Utf8PathBuf },
    /// e.g. type Size = SomeOtherType — transparent alias
    Passthrough { target_name: TypeName, type_args: Vec<String>, file_path: Utf8PathBuf },
}

/// A component → prop type mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentMapping {
    pub component_name: String,
    pub props_type_name: TypeName,
    pub props_type_args: Vec<String>,
    pub file_path: Utf8PathBuf,
    pub description: String,
    pub tags: BTreeMap<String, String>,
    /// Byte span for diagnostics
    pub span_start: u32,
    pub span_end: u32,
}

// ─── Import/Export Types ──────────────────────────────────────────────────────

/// An import binding found in a source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportBinding {
    /// Local name as used in this file (after `as` rename)
    pub local_name: TypeName,
    /// Original exported name
    pub exported_name: TypeName,
    /// Import specifier: "@radix-ui/react-button", "./types", etc.
    pub specifier: String,
    /// true for `import type { ... }`
    pub is_type_only: bool,
}

/// An export found in a source file — classified for re-export tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LexedExport {
    /// `export { Foo }` or `export { Foo as Bar }` from "./types"
    ReExportNamed {
        local_name: String,
        source_name: String,
        source_specifier: String,
        is_type_only: bool,
    },
    /// `export * from "./types"`
    ReExportAll {
        source_specifier: String,
        is_type_only: bool,
    },
    /// `export * as Ns from "./types"`
    ReExportNamespace {
        namespace: String,
        source_specifier: String,
    },
    /// `export interface Foo { }` / `export type Bar = ...` / `export const X`
    LocalDeclaration {
        name: String,
        is_type_only: bool,
    },
}

// ─── Diagnostics ─────────────────────────────────────────────────────────────

/// A non-fatal issue discovered during extraction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub help: Option<String>,
    pub code: DiagnosticCode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// Machine-readable diagnostic codes for programmatic consumers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiagnosticCode {
    UnresolvableImport,
    OpaqueType,
    PandaCodegenMissing,
    MaxDepthExceeded,
    ComponentDetectionFailed,
    BarrelResolutionFailed,
    Unknown,
}

// ─── Statistics ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionStats {
    pub components_extracted: u32,
    pub components_skipped: u32,
    pub files_parsed: u32,
    pub dts_files_parsed: u32,
    pub dts_cache_hits: u32,
    pub duration_ms: u64,
    pub tier1_count: u32,
    pub tier3_count: u32,
    pub opaque_count: u32,
}

// ─── Global Source Data ───────────────────────────────────────────────────────

/// Merged source data from all files — the shared resolution context.
/// Built once, then read by all parallel resolution workers.
/// Uses Arc in pipeline — clone is cheap.
#[derive(Debug, Default, Clone)]
pub struct GlobalSourceData {
    /// All interfaces across all files.
    /// Key: "${absolute_file_path}:${name}" — always scoped, never bare
    pub interfaces: FxHashMap<String, CollectedInterface>,

    /// All type aliases across all files.
    /// Key: "${absolute_file_path}:${name}"
    pub type_aliases: FxHashMap<String, CollectedTypeAlias>,

    /// All enum-like definitions across all files.
    /// Key: "${absolute_file_path}:${name}"
    pub enums: FxHashMap<String, Vec<EnumEntry>>,

    /// Import resolution map: file → [ImportBinding]
    pub import_map: FxHashMap<Utf8PathBuf, Vec<ImportBinding>>,

    /// Re-export map: file → [LexedExport]
    pub re_export_map: FxHashMap<Utf8PathBuf, Vec<LexedExport>>,

    /// All component mappings discovered
    pub component_mappings: Vec<ComponentMapping>,
}

impl GlobalSourceData {
    /// Merge a single file's SourceData into the global data.
    pub fn merge(&mut self, file_path: &Utf8Path, data: SourceData) {
        self.interfaces.extend(data.interfaces);
        self.type_aliases.extend(data.type_aliases);
        self.enums.extend(data.enums);
        self.import_map.insert(file_path.to_owned(), data.imports);
        self.re_export_map.insert(file_path.to_owned(), data.exports);
        self.component_mappings.extend(data.component_mappings);
    }
}
```

## CRITICAL: Use camino's Utf8Path

All path operations use `camino::{Utf8Path, Utf8PathBuf}` — never `std::path::Path`. This eliminates `.to_str().unwrap()` patterns throughout the codebase.

## What NOT to Do

- Do not use `std::collections::HashMap` — always `FxHashMap`
- Do not use `String` for type/prop names in hot-path structs — use `CompactString`/`TypeName`
- Do not put logic in this file — only type definitions and simple inherent methods
- Do not add `Display` impls here — that belongs in the serializer or CLI
- Do not use `Box<dyn Error>` — use typed errors via `thiserror`
- Do not add `#[allow(dead_code)]` — if something is defined, it must be used
