# Serializers, Integration Tests, and Critical Gotchas

---

# Agent: Serializers (Phase 3b extension)
# Model: claude-sonnet-4-6
# Add to: crates/core/src/pipeline.rs or crates/core/src/serializer.rs

## The Three Output Formats

All three formats are produced from the same `ExtractionOutput`.
No separate extraction — just different views.

```rust
// crates/core/src/serializer.rs

use serde_json::{json, Value};
use crate::types::*;

// ── Canonical — the native rich format ──────────────────────────────────────
// Just serde_json::to_string(&output) — ExtractionOutput derives Serialize

// ── RDT Compatible ──────────────────────────────────────────────────────────

pub fn to_rdt(output: &ExtractionOutput) -> Value {
    let components: serde_json::Map<String, Value> = output.components.iter()
        .map(|(name, entry)| (name.clone(), component_to_rdt(entry, output)))
        .collect();
    Value::Object(components)
}

fn component_to_rdt(entry: &ComponentEntry, output: &ExtractionOutput) -> Value {
    let props: serde_json::Map<String, Value> = entry.props.iter()
        .map(|(name, prop)| (name.clone(), prop_to_rdt(prop, output)))
        .collect();
    
    json!({
        "displayName": entry.display_name,
        "filePath": entry.file_path,
        "description": entry.description,
        "props": props,
        "methods": [],
        "tags": entry.tags,
    })
}

fn prop_to_rdt(prop: &ParsedProp, output: &ExtractionOutput) -> Value {
    json!({
        "name": prop.name,
        "required": prop.required,
        "description": prop.description,
        "defaultValue": prop.default_value.as_ref().map(|d| json!({ "value": d.value })),
        "type": prop_type_to_rdt(&prop.prop_type, output),
        "parent": prop.parent.as_ref().map(|p| json!({
            "name": p.name,
            "fileName": p.file_name,
        })),
        "declarations": prop.declarations.iter().map(|d| json!({
            "name": d.name,
            "fileName": d.file_name,
        })).collect::<Vec<_>>(),
        "tags": prop.tags,
    })
}

fn prop_type_to_rdt(pt: &PropType, output: &ExtractionOutput) -> Value {
    match pt {
        PropType::String => json!({ "name": "string" }),
        PropType::Number => json!({ "name": "number" }),
        PropType::Boolean => json!({ "name": "boolean" }),
        PropType::Null => json!({ "name": "null" }),
        PropType::Undefined => json!({ "name": "undefined" }),
        PropType::Any => json!({ "name": "any" }),
        PropType::Never => json!({ "name": "never" }),
        PropType::Unknown => json!({ "name": "unknown" }),
        PropType::Void => json!({ "name": "void" }),
        PropType::ReactNode => json!({ "name": "ReactNode" }),
        PropType::CssProperties => json!({ "name": "CSSProperties" }),
        PropType::ElementType => json!({ "name": "elementType" }),
        PropType::SxProps => json!({ "name": "SxProps", "raw": "SxProps<Theme>" }),
        PropType::Ref { element } => json!({
            "name": "other",
            "raw": element.as_deref().map(|e| format!("Ref<{}>", e)).unwrap_or_else(|| "Ref<unknown>".into()),
        }),
        PropType::EventHandler { event_type } => json!({
            "name": "func",
            "raw": format!("(e: {}) => void", event_type),
        }),
        PropType::StringLiteral(s) => json!({ "name": "enum", "value": [{"value": s, "description": ""}] }),
        PropType::NumberLiteral(n) => json!({ "name": "number", "raw": n.to_string() }),
        PropType::BoolLiteral(b) => json!({ "name": "boolean", "raw": b.to_string() }),
        
        // Pure literal unions → RDT "enum"
        PropType::LiteralUnion { members, .. } => json!({
            "name": "enum",
            "raw": members.join(" | "),
            "value": members.iter().map(|m| json!({"value": m, "description": ""})).collect::<Vec<_>>(),
        }),
        
        PropType::Union(members) if pt.is_literal_union() => {
            let values: Vec<_> = members.iter()
                .map(|m| json!({"value": m.raw_string(), "description": ""}))
                .collect();
            json!({
                "name": "enum",
                "raw": members.iter().map(|m| m.raw_string()).collect::<Vec<_>>().join(" | "),
                "value": values,
            })
        }
        
        PropType::Union(members) => json!({
            "name": "union",
            "raw": members.iter().map(|m| m.raw_string()).collect::<Vec<_>>().join(" | "),
            "value": members.iter().map(|m| prop_type_to_rdt(m, output)).collect::<Vec<_>>(),
        }),
        
        PropType::Array(inner) => json!({
            "name": "Array",
            "raw": format!("{}[]", inner.raw_string()),
        }),
        
        PropType::HtmlAttributes { element, .. } => json!({
            "name": "HTMLAttributes",
            "raw": format!("{}HTMLAttributes<HTML{}Element>", capitalize(element), capitalize(element)),
        }),
        
        PropType::Named { name, args } => json!({
            "name": name.as_str(),
            "raw": pt.raw_string(),
        }),
        
        PropType::Opaque { raw, .. } => json!({ "name": raw.as_str() }),
        
        _ => json!({ "name": pt.raw_string() }),
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

// ── Storybook __docgenInfo format ────────────────────────────────────────────

pub fn to_storybook_block(entry: &ComponentEntry) -> String {
    let json = serde_json::to_string(entry)
        .unwrap_or_default()
        // HTML-safe escaping
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    
    format!(
        "if (typeof {} !== 'undefined') {{\n  {}.__docgenInfo = {}\n}}",
        entry.display_name, entry.display_name, json
    )
}
```

---

# Agent: Integration Tests (Phase 6)
# Model: claude-sonnet-4-6
# Runs: After Phase 5 complete
# Owns: tests/ directory at workspace root

## The Differential Test Suite

```rust
// tests/integration/rdt_compat.rs
//
// Verify that our output is compatible with RDT for the standard propFilter pattern.
// This is the critical compat test for existing Storybook users.

use oxc_react_docgen_core::pipeline::{extract, PipelineOptions};
use oxc_react_docgen_core::serializer::to_rdt;
use insta::assert_json_snapshot;

#[test]
fn shadcn_button_rdt_compat() {
    let options = PipelineOptions {
        src_dirs: vec!["../../fixtures/shadcn".into()],
        ..Default::default()
    };
    let output = extract(&options);
    let rdt = to_rdt(&output);
    
    // Snapshot test — any change to RDT output requires explicit review
    assert_json_snapshot!("shadcn_button_rdt", rdt);
    
    // The canonical RDT propFilter pattern must work
    let button = &rdt["Button"]["props"];
    let html_props_in_node_modules: Vec<_> = button.as_object().unwrap()
        .values()
        .filter(|p| {
            p["parent"]["fileName"]
                .as_str()
                .map(|f| f.contains("node_modules"))
                .unwrap_or(false)
        })
        .collect();
    
    // HTML props should have node_modules in their parent.fileName
    // This is what RDT users filter on
    assert!(!html_props_in_node_modules.is_empty(),
        "HTML props must have parent.fileName pointing to node_modules/@types/react");
}

#[test]
fn radix_button_rdt_compat() {
    let options = PipelineOptions {
        src_dirs: vec!["../../fixtures/radix".into()],
        ..Default::default()
    };
    let output = extract(&options);
    let rdt = to_rdt(&output);
    assert_json_snapshot!("radix_button_rdt", rdt);
}

#[test]
fn mui_button_rdt_compat() {
    let options = PipelineOptions {
        src_dirs: vec!["../../fixtures/mui".into()],
        ..Default::default()
    };
    let output = extract(&options);
    let rdt = to_rdt(&output);
    assert_json_snapshot!("mui_button_rdt", rdt);
}
```

## Benchmarks

```rust
// crates/core/benches/extraction.rs

fn main() { divan::main(); }

#[divan::bench(args = [
    "fixtures/shadcn/button.tsx",
    "fixtures/radix/button.d.ts",
    "fixtures/react-aria/Button.d.ts",
    "fixtures/mui/Button.d.ts",
])]
fn parse_single_file(bencher: divan::Bencher, fixture: &str) {
    let source = std::fs::read_to_string(
        format!("../../{}", fixture)
    ).unwrap();
    let path = camino::Utf8Path::new(fixture);
    
    bencher.bench(|| {
        oxc_react_docgen_core::extractor::parse_file(path, divan::black_box(&source))
    });
}

#[divan::bench]
fn full_pipeline_shadcn(bencher: divan::Bencher) {
    let options = oxc_react_docgen_core::pipeline::PipelineOptions {
        src_dirs: vec!["../../fixtures/shadcn".into()],
        cross_package: false,
        ..Default::default()
    };
    
    bencher.bench(|| {
        oxc_react_docgen_core::pipeline::extract(divan::black_box(&options))
    });
}

// SLO: full_pipeline must complete in < 5ms for 50 components
// SLO: parse_single_file must complete in < 10µs
```

---

# Critical Gotchas — Read By All Agents

## OXC API Surface (verify these at implementation time)

```rust
// Parser API — the entry point
use oxc_parser::Parser;
use oxc_allocator::Allocator;
use oxc_span::SourceType;

let allocator = Allocator::default();
// SourceType::from_path may return Err — handle it
let source_type = SourceType::from_path(path).unwrap_or_default();
let ret = Parser::new(&allocator, source, source_type).parse();
// ret.program: Program<'_>
// ret.errors: Vec<OxcDiagnostic>

// Comments are in ret.program.comments or via the parser's comment handler
// Check oxc_parser docs at implementation time — API may have changed from spec
```

## oxc_resolver API

```rust
use oxc_resolver::{Resolver, ResolveOptions, ResolveError};

let resolver = Resolver::new(ResolveOptions {
    condition_names: vec!["types".into(), "import".into(), "require".into()],
    extensions: vec![".ts".into(), ".tsx".into(), ".d.ts".into()],
    ..Default::default()
});

// resolve() takes &Path (not Utf8Path) for the directory
match resolver.resolve(from_dir.as_std_path(), specifier) {
    Ok(resolution) => {
        let path: &std::path::Path = resolution.path();
        // Convert to Utf8PathBuf with error handling
    }
    Err(ResolveError::NotFound(_)) => { /* specifier not found */ }
    Err(e) => { /* other error */ }
}
```

## NAPI Thread Safety

```rust
// SESSIONS must be globally accessible across NAPI calls.
// Use LazyLock<DashMap<...>> — DashMap is Send + Sync.
// Never use std::sync::Mutex<HashMap<...>> — can deadlock under NAPI.

static SESSIONS: LazyLock<DashMap<u32, Arc<WatchSession>>> = LazyLock::new(DashMap::new);

// WatchSession fields must all be Send + Sync:
// ✓ DashMap (Send + Sync)
// ✓ ArcSwap (Send + Sync)
// ✓ Arc<GlobalSourceData> (Send + Sync if GlobalSourceData: Send + Sync)
// ✗ Rc, RefCell, raw pointers — never use in WatchSession
```

## BTreeMap vs FxHashMap — The Rule

```
FxHashMap: internal lookup, hot paths, GlobalSourceData.interfaces/type_aliases
BTreeMap:  any struct that becomes JSON output (deterministic key ordering!)

Why: serde_json serializes maps with key ordering preserved.
     Different key orders = different JSON = flaky snapshot tests.
     BTreeMap in output structs = always alphabetical = stable.
```

## CompactString Gotcha

```rust
// CompactString stores strings ≤24 bytes inline (no heap alloc).
// Most type names ("ButtonProps", "FC", "string") are ≤24 bytes — free.
// Be careful with very long type names in HashMap keys:
// "packages/ui/src/Button.tsx:ButtonProps" is > 24 bytes — heap allocated.
// This is fine — CompactString still works, just with a heap alloc.
// Don't try to force everything into 24 bytes.
```

## Vite plugin `enforce: 'pre'`

```typescript
// REQUIRED: enforce: 'pre' ensures we see the ORIGINAL TypeScript source.
// Without it, Vite may transform the file first (esbuild strips type annotations)
// and we'd have nothing to analyze.
// This is correct because we only READ the source — we don't transform TS.
```

## JSON Across NAPI Boundary

```rust
// Return JSON strings, not complex NAPI objects.
// NAPI type marshalling for complex nested structures is fragile.
// JSON.parse on 1MB is ~5ms — negligible for our use case.
// The TypeScript consumer does: const output = JSON.parse(napi.extractAll(options))

// CORRECT
#[napi]
pub fn extract_all(options: JsExtractOptions) -> NapiResult<String> {
    let output = extract(&options.into());
    serde_json::to_string(&output).map_err(...)
}

// WRONG — complex NAPI types are slow to marshal and error-prone
#[napi]
pub fn extract_all(options: JsExtractOptions) -> NapiResult<JsExtractionOutput> {
    // Don't do this
}
```

## RDT propFilter Compatibility

```
The #1 RDT usage pattern is:
  propFilter: (prop) => !prop.parent?.fileName.includes('node_modules')

For this to work, HTML/DOM props inherited from @types/react MUST have:
  parent.fileName containing "node_modules/@types/react/..."

This means:
- When resolving ButtonHTMLAttributes, the PropParent must point to
  the actual @types/react .d.ts file path, not our baked-in table.
- Use oxc_resolver to find @types/react location, parse it on first use,
  cache the result. Don't hardcode paths.
- If @types/react is not installed, fall back to our baked-in table
  with a synthetic parent.fileName of "node_modules/@types/react/index.d.ts".
```

## Never Degrade Silently

```rust
// Always emit a Diagnostic for unresolvable types.
// The user should know what couldn't be resolved and why.
// PropType::Opaque always pairs with a Diagnostic in the output.

// WRONG — silent failure
if ctx.global.interfaces.get(&key).is_none() {
    return ResolvedChain::default();  // silently empty
}

// CORRECT — fail loudly but gracefully
if ctx.global.interfaces.get(&key).is_none() {
    diagnostics.push(Diagnostic {
        severity: DiagnosticSeverity::Warning,
        message: format!("Cannot resolve type '{}' — it will appear as opaque", type_name),
        help: Some("Check that the package is installed and its types are resolvable".into()),
        code: DiagnosticCode::UnresolvableImport,
        ..
    });
    return ResolvedChain::empty_with_compose(type_name.to_owned());
}
```
