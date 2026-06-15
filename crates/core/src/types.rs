//! Shared data types for oxc-react-docgen.
//!
//! These types form the contract between pipeline stages.
//! Rules:
//! - `CompactString` for names/type strings (avoids heap alloc for short strings)
//! - `BTreeMap` for JSON-facing output (deterministic key ordering)
//! - `FxHashMap` for internal lookup maps (performance)
//! - All types are `Send + Sync` — required for rayon and NAPI

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ─── Type Aliases ────────────────────────────────────────────────────────────

/// Compact string used for names, type strings — avoids heap alloc under 24 bytes
pub type TypeName = CompactString;

// ─── CollectedType ───────────────────────────────────────────────────────────

/// A structured representation of a TypeScript type as collected from the AST.
/// This is the extractor's output — NOT yet resolved to a semantic PropType.
/// The resolver pattern-matches on this to produce PropType.
#[derive(Debug, Clone)]
pub enum CollectedType {
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
    BigInt,
    Symbol,

    // ── Literals
    StringLiteral(CompactString),
    NumberLiteral(f64),
    BoolLiteral(bool),

    // ── Structural
    Union(Vec<CollectedType>),
    Intersection(Vec<CollectedType>),
    Array(Box<CollectedType>),
    Tuple(Vec<CollectedType>),
    Object(Vec<CollectedObjectField>),

    // ── Named type reference (possibly generic, possibly not yet resolved)
    Named {
        name: CompactString,
        args: Vec<CollectedType>,
    },

    // ── `typeof X` — reference to the type of a value
    TypeOf(CompactString),

    // ── Indexed access: Type["key"] or Type[K]
    IndexedAccess {
        obj: Box<CollectedType>,
        key: Box<CollectedType>,
    },

    // ── Template literal: `compact-${Size}` → parts = [Str("compact-"), Named("Size", [])]
    TemplateLiteral(Vec<CollectedType>),

    // ── Function type: (props: P) => ReactNode
    Function {
        params: Vec<CollectedType>,
        return_type: Box<CollectedType>,
    },

    // ── Conditional: T extends U ? X : Y — cannot resolve statically, tag for typescript-go
    Conditional {
        check: Box<CollectedType>,
        extends_type: Box<CollectedType>,
        true_type: Box<CollectedType>,
        false_type: Box<CollectedType>,
    },

    // ── Mapped: { [K in Keys]: Value } — cannot resolve statically
    Mapped {
        key_type: Box<CollectedType>,
        value_type: Box<CollectedType>,
    },

    // ── Fallback for syntax we don't recognise
    Raw(String),
}

impl CollectedType {
    /// True if this type requires the TypeScript type checker to resolve.
    pub fn needs_type_checker(&self) -> bool {
        matches!(self, CollectedType::Conditional { .. } | CollectedType::Mapped { .. })
    }

    /// Produce a raw string representation for diagnostics and fallback display.
    pub fn to_raw_string(&self) -> String {
        match self {
            CollectedType::String => "string".into(),
            CollectedType::Number => "number".into(),
            CollectedType::Boolean => "boolean".into(),
            CollectedType::Null => "null".into(),
            CollectedType::Undefined => "undefined".into(),
            CollectedType::Any => "any".into(),
            CollectedType::Never => "never".into(),
            CollectedType::Unknown => "unknown".into(),
            CollectedType::Void => "void".into(),
            CollectedType::BigInt => "bigint".into(),
            CollectedType::Symbol => "symbol".into(),
            CollectedType::StringLiteral(s) => format!("\"{}\"", s),
            CollectedType::NumberLiteral(n) => n.to_string(),
            CollectedType::BoolLiteral(b) => b.to_string(),
            CollectedType::Union(members) => {
                members.iter().map(|m| m.to_raw_string()).collect::<Vec<_>>().join(" | ")
            }
            CollectedType::Intersection(members) => {
                members.iter().map(|m| m.to_raw_string()).collect::<Vec<_>>().join(" & ")
            }
            CollectedType::Array(inner) => format!("{}[]", inner.to_raw_string()),
            CollectedType::Named { name, args } if args.is_empty() => name.to_string(),
            CollectedType::Named { name, args } => format!(
                "{}<{}>",
                name,
                args.iter().map(|a| a.to_raw_string()).collect::<Vec<_>>().join(", ")
            ),
            CollectedType::TypeOf(name) => format!("typeof {}", name),
            CollectedType::IndexedAccess { obj, key } => {
                format!("{}[{}]", obj.to_raw_string(), key.to_raw_string())
            }
            CollectedType::TemplateLiteral(parts) => format!(
                "`{}`",
                parts
                    .iter()
                    .map(|p| match p {
                        CollectedType::StringLiteral(s) => s.to_string(),
                        other => format!("${{{}}}", other.to_raw_string()),
                    })
                    .collect::<Vec<_>>()
                    .join("")
            ),
            CollectedType::Function { params, return_type } => format!(
                "({}) => {}",
                params.iter().map(|p| p.to_raw_string()).collect::<Vec<_>>().join(", "),
                return_type.to_raw_string()
            ),
            CollectedType::Conditional { check, extends_type, true_type, false_type } => format!(
                "{} extends {} ? {} : {}",
                check.to_raw_string(),
                extends_type.to_raw_string(),
                true_type.to_raw_string(),
                false_type.to_raw_string()
            ),
            CollectedType::Mapped { key_type, value_type } => {
                format!("{{ [K in {}]: {} }}", key_type.to_raw_string(), value_type.to_raw_string())
            }
            CollectedType::Tuple(members) => format!(
                "[{}]",
                members.iter().map(|m| m.to_raw_string()).collect::<Vec<_>>().join(", ")
            ),
            CollectedType::Object(_) => "{ ... }".into(),
            CollectedType::Raw(s) => s.clone(),
        }
    }

    fn to_json_value(&self) -> serde_json::Value {
        match self {
            // Primitives: serialize as short string tags
            CollectedType::String    => serde_json::json!("str"),
            CollectedType::Number    => serde_json::json!("num"),
            CollectedType::Boolean   => serde_json::json!("bool"),
            CollectedType::Null      => serde_json::json!("null"),
            CollectedType::Undefined => serde_json::json!("undef"),
            CollectedType::Any       => serde_json::json!("any"),
            CollectedType::Never     => serde_json::json!("never"),
            CollectedType::Unknown   => serde_json::json!("unknown"),
            CollectedType::Void      => serde_json::json!("void"),
            CollectedType::BigInt    => serde_json::json!("bigint"),
            CollectedType::Symbol    => serde_json::json!("symbol"),
            // Literals
            CollectedType::StringLiteral(s) => serde_json::json!({"sl": s.as_str()}),
            CollectedType::NumberLiteral(n) => serde_json::json!({"nl": n}),
            CollectedType::BoolLiteral(b)   => serde_json::json!({"bl": b}),
            // Named: {"n": name, "a": [args...]}
            CollectedType::Named { name, args } => serde_json::json!({
                "n": name.as_str(),
                "a": args.iter().map(|a| a.to_json_value()).collect::<Vec<_>>()
            }),
            // Union: {"u": [members...]}
            CollectedType::Union(members) => serde_json::json!({
                "u": members.iter().map(|m| m.to_json_value()).collect::<Vec<_>>()
            }),
            // Intersection: {"i": [members...]}
            CollectedType::Intersection(members) => serde_json::json!({
                "i": members.iter().map(|m| m.to_json_value()).collect::<Vec<_>>()
            }),
            // Array: {"arr": inner}
            CollectedType::Array(inner) => serde_json::json!({"arr": inner.to_json_value()}),
            // Tuple: {"tup": [members...]}
            CollectedType::Tuple(members) => serde_json::json!({
                "tup": members.iter().map(|m| m.to_json_value()).collect::<Vec<_>>()
            }),
            // Object: {"obj": [{name, t, req, desc}...]}
            CollectedType::Object(fields) => serde_json::json!({
                "obj": fields.iter().map(|f| serde_json::json!({
                    "name": f.name,
                    "t": f.collected_type.to_json_value(),
                    "req": f.required,
                    "desc": f.description,
                })).collect::<Vec<_>>()
            }),
            // TypeOf: {"to": name}
            CollectedType::TypeOf(name) => serde_json::json!({"to": name.as_str()}),
            // IndexedAccess: {"ia": {o, k}}
            CollectedType::IndexedAccess { obj, key } => serde_json::json!({
                "ia": {"o": obj.to_json_value(), "k": key.to_json_value()}
            }),
            // TemplateLiteral: {"tl": [parts...]}
            CollectedType::TemplateLiteral(parts) => serde_json::json!({
                "tl": parts.iter().map(|p| p.to_json_value()).collect::<Vec<_>>()
            }),
            // Function: {"fn": {p: [params], r: return_type}}
            CollectedType::Function { params, return_type } => serde_json::json!({
                "fn": {
                    "p": params.iter().map(|p| p.to_json_value()).collect::<Vec<_>>(),
                    "r": return_type.to_json_value()
                }
            }),
            // Conditional: {"cond": {c, e, t, f}}
            CollectedType::Conditional { check, extends_type, true_type, false_type } => serde_json::json!({
                "cond": {
                    "c": check.to_json_value(),
                    "e": extends_type.to_json_value(),
                    "t": true_type.to_json_value(),
                    "f": false_type.to_json_value(),
                }
            }),
            // Mapped: {"mapped": {k, v}}
            CollectedType::Mapped { key_type, value_type } => serde_json::json!({
                "mapped": {"k": key_type.to_json_value(), "v": value_type.to_json_value()}
            }),
            // Raw fallback: {"raw": s}
            CollectedType::Raw(s) => serde_json::json!({"raw": s}),
        }
    }

    fn from_json_value(v: &serde_json::Value) -> Result<Self, String> {
        match v {
            serde_json::Value::String(s) => Ok(match s.as_str() {
                "str"     => CollectedType::String,
                "num"     => CollectedType::Number,
                "bool"    => CollectedType::Boolean,
                "null"    => CollectedType::Null,
                "undef"   => CollectedType::Undefined,
                "any"     => CollectedType::Any,
                "never"   => CollectedType::Never,
                "unknown" => CollectedType::Unknown,
                "void"    => CollectedType::Void,
                "bigint"  => CollectedType::BigInt,
                "symbol"  => CollectedType::Symbol,
                other     => CollectedType::Raw(other.to_string()),
            }),
            serde_json::Value::Object(map) => {
                if let Some(s) = map.get("sl").and_then(|v| v.as_str()) {
                    return Ok(CollectedType::StringLiteral(s.into()));
                }
                if let Some(n) = map.get("nl").and_then(|v| v.as_f64()) {
                    return Ok(CollectedType::NumberLiteral(n));
                }
                if let Some(b) = map.get("bl").and_then(|v| v.as_bool()) {
                    return Ok(CollectedType::BoolLiteral(b));
                }
                if let Some(name) = map.get("n").and_then(|v| v.as_str()) {
                    let args = map.get("a")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().map(Self::from_json_value).collect::<Result<Vec<_>, _>>())
                        .unwrap_or(Ok(vec![]))?;
                    return Ok(CollectedType::Named { name: name.into(), args });
                }
                if let Some(arr) = map.get("u").and_then(|v| v.as_array()) {
                    return Ok(CollectedType::Union(
                        arr.iter().map(Self::from_json_value).collect::<Result<_, _>>()?
                    ));
                }
                if let Some(arr) = map.get("i").and_then(|v| v.as_array()) {
                    return Ok(CollectedType::Intersection(
                        arr.iter().map(Self::from_json_value).collect::<Result<_, _>>()?
                    ));
                }
                if let Some(inner) = map.get("arr") {
                    return Ok(CollectedType::Array(Box::new(Self::from_json_value(inner)?)));
                }
                if let Some(arr) = map.get("tup").and_then(|v| v.as_array()) {
                    return Ok(CollectedType::Tuple(
                        arr.iter().map(Self::from_json_value).collect::<Result<_, _>>()?
                    ));
                }
                if let Some(arr) = map.get("obj").and_then(|v| v.as_array()) {
                    let fields: Vec<CollectedObjectField> = arr.iter().map(|f| {
                        let o = f.as_object().ok_or_else(|| "expected object for field".to_string())?;
                        Ok(CollectedObjectField {
                            name: o.get("name").and_then(|v| v.as_str()).unwrap_or("").to_owned(),
                            collected_type: Self::from_json_value(o.get("t").unwrap_or(&serde_json::Value::Null))?,
                            required: o.get("req").and_then(|v| v.as_bool()).unwrap_or(false),
                            description: o.get("desc").and_then(|v| v.as_str()).unwrap_or("").to_owned(),
                        })
                    }).collect::<Result<_, String>>()?;
                    return Ok(CollectedType::Object(fields));
                }
                if let Some(name) = map.get("to").and_then(|v| v.as_str()) {
                    return Ok(CollectedType::TypeOf(name.into()));
                }
                if let Some(inner) = map.get("ia") {
                    let obj = Self::from_json_value(inner.get("o").unwrap_or(&serde_json::Value::Null))?;
                    let key = Self::from_json_value(inner.get("k").unwrap_or(&serde_json::Value::Null))?;
                    return Ok(CollectedType::IndexedAccess { obj: Box::new(obj), key: Box::new(key) });
                }
                if let Some(arr) = map.get("tl").and_then(|v| v.as_array()) {
                    return Ok(CollectedType::TemplateLiteral(
                        arr.iter().map(Self::from_json_value).collect::<Result<_, _>>()?
                    ));
                }
                if let Some(inner) = map.get("fn") {
                    let params = inner.get("p").and_then(|v| v.as_array())
                        .map(|arr| arr.iter().map(Self::from_json_value).collect::<Result<Vec<_>, _>>())
                        .unwrap_or(Ok(vec![]))?;
                    let rt = Self::from_json_value(inner.get("r").unwrap_or(&serde_json::Value::Null))?;
                    return Ok(CollectedType::Function { params, return_type: Box::new(rt) });
                }
                if let Some(inner) = map.get("cond") {
                    let check = Self::from_json_value(inner.get("c").unwrap_or(&serde_json::Value::Null))?;
                    let ext   = Self::from_json_value(inner.get("e").unwrap_or(&serde_json::Value::Null))?;
                    let tt    = Self::from_json_value(inner.get("t").unwrap_or(&serde_json::Value::Null))?;
                    let ft    = Self::from_json_value(inner.get("f").unwrap_or(&serde_json::Value::Null))?;
                    return Ok(CollectedType::Conditional {
                        check: Box::new(check),
                        extends_type: Box::new(ext),
                        true_type: Box::new(tt),
                        false_type: Box::new(ft),
                    });
                }
                if let Some(inner) = map.get("mapped") {
                    let k = Self::from_json_value(inner.get("k").unwrap_or(&serde_json::Value::Null))?;
                    let vt = Self::from_json_value(inner.get("v").unwrap_or(&serde_json::Value::Null))?;
                    return Ok(CollectedType::Mapped { key_type: Box::new(k), value_type: Box::new(vt) });
                }
                if let Some(s) = map.get("raw").and_then(|v| v.as_str()) {
                    return Ok(CollectedType::Raw(s.to_owned()));
                }
                // Unknown shape — fall back to raw
                Ok(CollectedType::Raw(v.to_string()))
            }
            _ => Ok(CollectedType::Raw(v.to_string())),
        }
    }
}

impl serde::Serialize for CollectedType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let v = self.to_json_value();
        v.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for CollectedType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let v = serde_json::Value::deserialize(deserializer)?;
        Self::from_json_value(&v).map_err(D::Error::custom)
    }
}

impl serde::Serialize for CollectedObjectField {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("CollectedObjectField", 4)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("collected_type", &self.collected_type)?;
        s.serialize_field("required", &self.required)?;
        s.serialize_field("description", &self.description)?;
        s.end()
    }
}

impl<'de> serde::Deserialize<'de> for CollectedObjectField {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct FieldVisitor;

        impl<'de> Visitor<'de> for FieldVisitor {
            type Value = CollectedObjectField;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct CollectedObjectField")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<CollectedObjectField, A::Error> {
                let mut name: Option<String> = None;
                let mut collected_type: Option<CollectedType> = None;
                let mut required: Option<bool> = None;
                let mut description: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => { name = Some(map.next_value()?); }
                        "collected_type" => { collected_type = Some(map.next_value()?); }
                        "required" => { required = Some(map.next_value()?); }
                        "description" => { description = Some(map.next_value()?); }
                        _ => { let _ = map.next_value::<serde::de::IgnoredAny>()?; }
                    }
                }

                Ok(CollectedObjectField {
                    name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                    collected_type: collected_type.ok_or_else(|| de::Error::missing_field("collected_type"))?,
                    required: required.ok_or_else(|| de::Error::missing_field("required"))?,
                    description: description.ok_or_else(|| de::Error::missing_field("description"))?,
                })
            }
        }

        deserializer.deserialize_struct(
            "CollectedObjectField",
            &["name", "collected_type", "required", "description"],
            FieldVisitor,
        )
    }
}

/// An object field as collected from the AST (not yet resolved).
#[derive(Debug, Clone)]
pub struct CollectedObjectField {
    pub name: String,
    pub collected_type: CollectedType,
    pub required: bool,
    pub description: String,
}

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

/// One step in a component's resolved inheritance chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InheritedLayer {
    /// Type name as declared: "ButtonHTMLAttributes", "ButtonBaseProps", "ThemingProps"
    pub type_name: String,
    /// Absolute path of the file this came from.
    /// "node_modules/@types/react/..." for HTML attrs; project-local path otherwise.
    pub file_name: String,
    /// Props explicitly removed at this layer via Omit<Base, K>.
    pub omitted: Vec<String>,
    /// If this is an HTML element attributes type, the element name: "button", "input", etc.
    pub html_element: Option<String>,
    /// Number of props this layer contributes (for display).
    pub total_props: u32,
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
    /// Full resolved inheritance chain, outermost first.
    pub inheritance: Vec<InheritedLayer>,
    /// Curated notable props from inherited layers (not in self.props).
    pub notable_inherited: BTreeMap<String, ParsedProp>,
    /// If props type is a discriminated union, the name of the discriminant prop (e.g. "variant").
    pub discriminant_prop: Option<String>,
    /// Type names that could not be resolved (react-docgen compat)
    pub composes: Vec<String>,
    /// JSDoc @tags on the component (e.g. @deprecated, @since)
    pub tags: BTreeMap<String, String>,
    /// Always empty for functional components; present for RDT compat
    pub methods: Vec<()>,
}

impl ComponentEntry {
    /// Shortcut: the HTML element this component renders as, if any.
    /// Derived from the inheritance chain.
    pub fn html_element(&self) -> Option<&str> {
        self.inheritance.iter().find_map(|l| l.html_element.as_deref())
    }
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
#[derive(Debug, Clone, PartialEq)]
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
            PropType::Tuple(_) => "tuple".into(),
            PropType::Object(_) => "object".into(),
            PropType::LiteralUnion { members, .. } => {
                members.iter().map(|m| format!(r#""{}""#, m)).collect::<Vec<_>>().join(" | ")
            }
            PropType::Named { name, args } if args.is_empty() => name.to_string(),
            PropType::Named { name, args } => {
                let args_str =
                    args.iter().map(|a| a.raw_string()).collect::<Vec<_>>().join(", ");
                format!("{}<{}>", name, args_str)
            }
            PropType::ReactNode => "ReactNode".into(),
            PropType::CssProperties => "CSSProperties".into(),
            PropType::EventHandler { event_type } => {
                format!("({}: {}) => void", "e", event_type)
            }
            PropType::Ref { element: Some(e) } => format!("Ref<{}>", e),
            PropType::Ref { element: None } => "Ref<unknown>".into(),
            PropType::ElementType => "ElementType".into(),
            PropType::HtmlAttributes { element, .. } => {
                format!("{}HTMLAttributes", element_to_type_name(element))
            }
            PropType::SxProps => "SxProps".into(),
            PropType::Opaque { raw, .. } => raw.clone(),
        }
    }
}

impl serde::Serialize for PropType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_tagged_value().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for PropType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let v = serde_json::Value::deserialize(deserializer)?;
        Self::from_tagged_value(&v).map_err(D::Error::custom)
    }
}

impl PropType {
    fn to_tagged_value(&self) -> serde_json::Value {
        match self {
            // Unit variants (primitives)
            PropType::String    => serde_json::json!({"kind": "string"}),
            PropType::Number    => serde_json::json!({"kind": "number"}),
            PropType::Boolean   => serde_json::json!({"kind": "boolean"}),
            PropType::Null      => serde_json::json!({"kind": "null"}),
            PropType::Undefined => serde_json::json!({"kind": "undefined"}),
            PropType::Any       => serde_json::json!({"kind": "any"}),
            PropType::Never     => serde_json::json!({"kind": "never"}),
            PropType::Unknown   => serde_json::json!({"kind": "unknown"}),
            PropType::Void      => serde_json::json!({"kind": "void"}),
            PropType::ReactNode    => serde_json::json!({"kind": "reactNode"}),
            PropType::CssProperties => serde_json::json!({"kind": "cssProperties"}),
            PropType::ElementType  => serde_json::json!({"kind": "elementType"}),
            PropType::SxProps      => serde_json::json!({"kind": "sxProps"}),
            // Newtype/tuple variants — inner is not a map, so wrap as "0"
            PropType::StringLiteral(s) => serde_json::json!({"kind": "stringLiteral", "0": s}),
            PropType::NumberLiteral(n) => serde_json::json!({"kind": "numberLiteral", "0": n}),
            PropType::BoolLiteral(b)   => serde_json::json!({"kind": "boolLiteral", "0": b}),
            PropType::Union(members) => serde_json::json!({
                "kind": "union",
                "0": members.iter().map(|m| m.to_tagged_value()).collect::<Vec<_>>()
            }),
            PropType::Intersection(members) => serde_json::json!({
                "kind": "intersection",
                "0": members.iter().map(|m| m.to_tagged_value()).collect::<Vec<_>>()
            }),
            PropType::Array(inner) => serde_json::json!({
                "kind": "array",
                "0": inner.to_tagged_value()
            }),
            PropType::Tuple(members) => serde_json::json!({
                "kind": "tuple",
                "0": members.iter().map(|m| m.to_tagged_value()).collect::<Vec<_>>()
            }),
            PropType::Object(fields) => serde_json::json!({
                "kind": "object",
                "0": fields.iter().map(|f| serde_json::json!({
                    "name": f.name,
                    "propType": f.prop_type.to_tagged_value(),
                    "required": f.required,
                    "description": f.description,
                })).collect::<Vec<_>>()
            }),
            // Struct variants — fields merge directly with kind
            PropType::Named { name, args } => serde_json::json!({
                "kind": "named",
                "name": name.as_str(),
                "args": args.iter().map(|a| a.to_tagged_value()).collect::<Vec<_>>()
            }),
            PropType::EventHandler { event_type } => serde_json::json!({
                "kind": "eventHandler",
                "eventType": event_type
            }),
            PropType::Ref { element } => serde_json::json!({
                "kind": "ref",
                "element": element
            }),
            PropType::HtmlAttributes { element, omitted } => serde_json::json!({
                "kind": "htmlAttributes",
                "element": element,
                "omitted": omitted
            }),
            PropType::LiteralUnion { members, has_default } => serde_json::json!({
                "kind": "literalUnion",
                "members": members,
                "hasDefault": has_default
            }),
            PropType::Opaque { raw, reason } => {
                let reason_val = match reason {
                    OpaqueReason::ConditionalType => serde_json::json!({"type": "conditionalType"}),
                    OpaqueReason::MappedType => serde_json::json!({"type": "mappedType"}),
                    OpaqueReason::ModuleAugmentation => serde_json::json!({"type": "moduleAugmentation"}),
                    OpaqueReason::RuntimeDependent { function_name } => serde_json::json!({"type": "runtimeDependent", "functionName": function_name}),
                    OpaqueReason::UnresolvableImport { specifier } => serde_json::json!({"type": "unresolvableImport", "specifier": specifier}),
                    OpaqueReason::PandaCodegenMissing => serde_json::json!({"type": "pandaCodegenMissing"}),
                    OpaqueReason::DepthExceeded => serde_json::json!({"type": "depthExceeded"}),
                    OpaqueReason::IndexedAccess { expression } => serde_json::json!({"type": "indexedAccess", "expression": expression}),
                    OpaqueReason::TemplateLiteral { expression } => serde_json::json!({"type": "templateLiteral", "expression": expression}),
                };
                serde_json::json!({"kind": "opaque", "raw": raw, "reason": reason_val})
            }
        }
    }

    fn from_tagged_value(v: &serde_json::Value) -> Result<Self, String> {
        let kind = v.get("kind").and_then(|k| k.as_str())
            .ok_or_else(|| "missing 'kind' field in PropType JSON".to_string())?;
        match kind {
            "string"    => Ok(PropType::String),
            "number"    => Ok(PropType::Number),
            "boolean"   => Ok(PropType::Boolean),
            "null"      => Ok(PropType::Null),
            "undefined" => Ok(PropType::Undefined),
            "any"       => Ok(PropType::Any),
            "never"     => Ok(PropType::Never),
            "unknown"   => Ok(PropType::Unknown),
            "void"      => Ok(PropType::Void),
            "reactNode" | "react_node" => Ok(PropType::ReactNode),
            "cssProperties" | "css_properties" => Ok(PropType::CssProperties),
            "elementType" | "element_type" => Ok(PropType::ElementType),
            "sxProps" | "sx_props" => Ok(PropType::SxProps),
            "stringLiteral" | "string_literal" => {
                let s = v["0"].as_str().unwrap_or("").to_owned();
                Ok(PropType::StringLiteral(s))
            }
            "numberLiteral" | "number_literal" => {
                let n = v["0"].as_f64().unwrap_or(0.0);
                Ok(PropType::NumberLiteral(n))
            }
            "boolLiteral" | "bool_literal" => {
                let b = v["0"].as_bool().unwrap_or(false);
                Ok(PropType::BoolLiteral(b))
            }
            "union" => {
                let members = v["0"].as_array()
                    .map(|a| a.iter().map(Self::from_tagged_value).collect::<Result<Vec<_>, _>>())
                    .unwrap_or(Ok(vec![]))?;
                Ok(PropType::Union(members))
            }
            "intersection" => {
                let members = v["0"].as_array()
                    .map(|a| a.iter().map(Self::from_tagged_value).collect::<Result<Vec<_>, _>>())
                    .unwrap_or(Ok(vec![]))?;
                Ok(PropType::Intersection(members))
            }
            "array" => {
                let inner = Self::from_tagged_value(&v["0"])?;
                Ok(PropType::Array(Box::new(inner)))
            }
            "tuple" => {
                let members = v["0"].as_array()
                    .map(|a| a.iter().map(Self::from_tagged_value).collect::<Result<Vec<_>, _>>())
                    .unwrap_or(Ok(vec![]))?;
                Ok(PropType::Tuple(members))
            }
            "object" => {
                let fields = v["0"].as_array()
                    .map(|a| a.iter().map(|f| {
                        Ok(ObjectField {
                            name: f["name"].as_str().unwrap_or("").to_owned(),
                            prop_type: Self::from_tagged_value(&f["propType"])?,
                            required: f["required"].as_bool().unwrap_or(false),
                            description: f["description"].as_str().unwrap_or("").to_owned(),
                        })
                    }).collect::<Result<Vec<_>, String>>())
                    .unwrap_or(Ok(vec![]))?;
                Ok(PropType::Object(fields))
            }
            "named" => {
                let name = v["name"].as_str().unwrap_or("").into();
                let args = v["args"].as_array()
                    .map(|a| a.iter().map(Self::from_tagged_value).collect::<Result<Vec<_>, _>>())
                    .unwrap_or(Ok(vec![]))?;
                Ok(PropType::Named { name, args })
            }
            "eventHandler" | "event_handler" => {
                let event_type = v["eventType"].as_str()
                    .or_else(|| v["event_type"].as_str())
                    .unwrap_or("").to_owned();
                Ok(PropType::EventHandler { event_type })
            }
            "ref" => {
                let element = v["element"].as_str().map(|s| s.to_owned());
                Ok(PropType::Ref { element })
            }
            "htmlAttributes" | "html_attributes" => {
                let element = v["element"].as_str().unwrap_or("div").to_owned();
                let omitted = v["omitted"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).map(|s| s.to_owned()).collect())
                    .unwrap_or_default();
                Ok(PropType::HtmlAttributes { element, omitted })
            }
            "literalUnion" | "literal_union" => {
                let members = v["members"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).map(|s| s.to_owned()).collect())
                    .unwrap_or_default();
                let has_default = v["hasDefault"].as_bool()
                    .or_else(|| v["has_default"].as_bool())
                    .unwrap_or(false);
                Ok(PropType::LiteralUnion { members, has_default })
            }
            "opaque" => {
                let raw = v["raw"].as_str().unwrap_or("").to_owned();
                let reason = match v["reason"]["type"].as_str().unwrap_or("depthExceeded") {
                    "conditionalType" => OpaqueReason::ConditionalType,
                    "mappedType" => OpaqueReason::MappedType,
                    "moduleAugmentation" => OpaqueReason::ModuleAugmentation,
                    "runtimeDependent" => OpaqueReason::RuntimeDependent {
                        function_name: v["reason"]["functionName"].as_str().unwrap_or("").to_owned(),
                    },
                    "unresolvableImport" => OpaqueReason::UnresolvableImport {
                        specifier: v["reason"]["specifier"].as_str().unwrap_or("").to_owned(),
                    },
                    "pandaCodegenMissing" => OpaqueReason::PandaCodegenMissing,
                    "indexedAccess" => OpaqueReason::IndexedAccess {
                        expression: v["reason"]["expression"].as_str().unwrap_or("").to_owned(),
                    },
                    "templateLiteral" => OpaqueReason::TemplateLiteral {
                        expression: v["reason"]["expression"].as_str().unwrap_or("").to_owned(),
                    },
                    _ => OpaqueReason::DepthExceeded,
                };
                Ok(PropType::Opaque { raw, reason })
            }
            other => Ok(PropType::Opaque {
                raw: format!("unknown PropType kind: {}", other),
                reason: OpaqueReason::DepthExceeded,
            }),
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
        "div" | "span" | "p" | "h1" | "h2" | "h3" => "HTML",
        _ => "HTML",
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
    /// Indexed access type (Type["key"]) — enable typescript-go to resolve.
    IndexedAccess { expression: String },
    /// Template literal type — partially or fully unresolvable.
    TemplateLiteral { expression: String },
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
    /// Structured type collected from the AST — replaces raw string for reliable resolver access.
    pub collected_type: CollectedType,
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
    Omit { base: CollectedType, omitted_keys: Vec<String>, file_path: Utf8PathBuf },
    Pick { base: CollectedType, picked_keys: Vec<String>, file_path: Utf8PathBuf },
    Partial { base: CollectedType, file_path: Utf8PathBuf },
    Required { base: CollectedType, file_path: Utf8PathBuf },
    Union { members: Vec<CollectedType>, file_path: Utf8PathBuf },
    Intersection { members: Vec<CollectedType>, file_path: Utf8PathBuf },
    LiteralUnion { members: Vec<String>, file_path: Utf8PathBuf },
    /// e.g. type Size = SomeOtherType — transparent alias
    Passthrough { target: CollectedType, file_path: Utf8PathBuf },
}

/// The source of a default value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DefaultSource {
    /// Extracted from parameter destructuring: `({ size = 'md' }: Props) => ...`
    Destructuring,
    /// Extracted from `Component.defaultProps = { size: 'md' }` assignment.
    DefaultProps,
    /// Extracted from `/** @default 'md' */` JSDoc/TSDoc annotation.
    JsDoc,
}

/// A default value as collected by the extractor (before resolver processing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDefault {
    /// String representation of the default value (literal or source expression).
    pub value: String,
    /// True if `value` is a runtime expression we couldn't statically evaluate.
    pub computed: bool,
    /// Where this default came from.
    pub source: DefaultSource,
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
    /// Default values extracted from parameter destructuring or defaultProps.
    /// Key = prop name. Populated only for implementation files (.tsx/.ts), not .d.ts.
    pub param_defaults: FxHashMap<String, RawDefault>,
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
    /// JSDoc @default conflicts with code default value — code value was used.
    JsDocDefaultMismatch,
    /// Default value is a runtime expression that could not be statically evaluated.
    ComputedDefault,
    /// Indexed access type (Type["key"]) that could not be resolved from known tables.
    IndexedAccessOpaque,
    /// Template literal type that could not be statically expanded.
    TemplateLiteralOpaque,
    /// Callable component detected via call signature interface.
    CallableComponent,
    /// Discriminated union detected — props merged with discriminant surfaced.
    DiscriminatedUnion,
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
        for (key, iface) in data.interfaces {
            match self.interfaces.entry(key) {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    // Declaration merging: combine props and extends
                    let existing = e.get_mut();
                    existing.props.extend(iface.props);
                    existing.extends.extend(iface.extends);
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(iface);
                }
            }
        }
        self.type_aliases.extend(data.type_aliases);
        self.enums.extend(data.enums);
        self.import_map.insert(file_path.to_owned(), data.imports);
        self.re_export_map.insert(file_path.to_owned(), data.exports);
        self.component_mappings.extend(data.component_mappings);
    }

    /// Remove all entries contributed by `file_path`. Called before re-merging an updated file.
    pub fn remove_file(&mut self, file_path: &Utf8Path) {
        let prefix = format!("{}:", file_path);
        self.interfaces.retain(|k, _| !k.starts_with(&prefix));
        self.type_aliases.retain(|k, _| !k.starts_with(&prefix));
        self.enums.retain(|k, _| !k.starts_with(&prefix));
        self.import_map.remove(file_path);
        self.re_export_map.remove(file_path);
        self.component_mappings.retain(|m| m.file_path != file_path);
    }
}
