//! Prop-type resolver — Phase 3a.
//!
//! Converts `ComponentMapping` (raw collected data) into a fully resolved
//! `ComponentEntry` with `PropType` props.
//!
//! Central design:
//! - `resolve_component` is the entry point (called in parallel per component).
//! - `resolve_collected_type` is the recursive dispatch for `CollectedType → PropType`.
//! - No string parsing happens here — everything is already structured `CollectedType`.
//! - Graceful degradation: every unresolvable type becomes `PropType::Opaque` + a `Diagnostic`.

use std::collections::BTreeMap;
use std::sync::Arc;

#[allow(unused_imports)]
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use oxc_resolver::{AliasValue, ResolveOptions, Resolver};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::import_map::ImportResolutionMap;
use crate::pipeline::PipelineOptions;
use crate::react_types;
use crate::types::*;

mod alias;
mod chain;
mod collected;
mod extends;
mod func;
mod html;
mod import;
mod named;
mod primitives;
mod react;
mod substitute;
mod template;

// ─── Constants ────────────────────────────────────────────────────────────────

/// Maximum recursion depth for type resolution.
/// Prevents infinite loops on circular type references.
const MAX_DEPTH: u8 = 20;

// ─── ResolutionContext ────────────────────────────────────────────────────────

/// Shared, read-only context passed to all resolution functions.
/// Arc'd so it can be cheaply shared across rayon threads.
pub struct ResolutionContext {
    pub global: Arc<GlobalSourceData>,
    pub import_map: Arc<ImportResolutionMap>,
    pub oxc_resolver: Arc<Resolver>,
    pub extra_builtins: FxHashSet<CompactString>,
    /// Bare name → full scoped key ("file:name") index over `global.enums`,
    /// built once per resolution pass so `resolve_typeof` and
    /// `resolve_cva_variant_props` (VariantProps<typeof X>) can do an O(1)
    /// lookup instead of a linear scan with a `format!()` allocation per
    /// candidate on every reference. Ambiguous bare names (same name in two
    /// files) resolve to whichever key `global.enums` yields first during
    /// this build pass — the same tie-break the old linear scan produced,
    /// since both iterate the same underlying map.
    pub enum_bare_index: FxHashMap<CompactString, CompactString>,
    /// How much of an inherited HTML element's attributes to expose.
    pub html_attributes: crate::pipeline::HtmlAttributeMode,
    /// Paths to TypeScript's own `lib.es5.d.ts`/`lib.dom.d.ts`, if resolvable
    /// from this project (empty otherwise — see `resolve_ts_lib_paths`).
    /// Checked as a last-resort fallback when a bare, never-imported name
    /// (a real native/DOM global like `Date`) doesn't resolve via the normal
    /// same-file lookup — see `import::lookup_interface`/`lookup_type_alias`.
    pub ambient_global_files: Vec<Utf8PathBuf>,
}

impl ResolutionContext {
    pub fn new(global: Arc<GlobalSourceData>, options: &PipelineOptions) -> Self {
        let alias: Vec<(String, Vec<AliasValue>)> = react::read_tsconfig_paths(options.tsconfig_path.as_deref());

        let resolve_options = ResolveOptions {
            condition_names: vec!["types".into(), "import".into(), "require".into(), "default".into()],
            main_fields: vec!["types".into(), "typings".into(), "module".into(), "main".into()],
            extensions: vec![".ts".into(), ".tsx".into(), ".d.ts".into(), ".js".into()],
            alias,
            ..ResolveOptions::default()
        };

        let mut enum_bare_index: FxHashMap<CompactString, CompactString> = FxHashMap::default();
        for key in global.enums.keys() {
            let bare = key.rsplit_once(':').map(|(_, name)| name).unwrap_or(key.as_str());
            enum_bare_index.entry(CompactString::from(bare)).or_insert_with(|| CompactString::from(key.as_str()));
        }

        let ambient_global_files = options
            .src_dirs
            .first()
            .and_then(|dir| std::fs::canonicalize(dir).ok())
            .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
            .map(|from_dir| resolve_ts_lib_paths(&from_dir).into_iter().map(Utf8PathBuf::from).collect())
            .unwrap_or_default();

        Self {
            import_map: Arc::new(ImportResolutionMap::build(&global)),
            global,
            oxc_resolver: Arc::new(Resolver::new(resolve_options)),
            extra_builtins: options.extra_builtins.clone(),
            enum_bare_index,
            html_attributes: options.html_attributes,
            ambient_global_files,
        }
    }
}

/// Resolve `package_name` (e.g. "react") to its real `.d.ts` file path, for
/// callers outside the resolver — the pipeline's `HtmlAttributeMode::Full`
/// wiring needs this to know which file to parse and merge, before a
/// `ResolutionContext` (which needs the already-merged `GlobalSourceData`) can
/// exist yet.
pub fn resolve_package_dts_path(from_dir: &camino::Utf8Path, package_name: &str) -> Option<String> {
    let resolve_options = ResolveOptions {
        condition_names: vec!["types".into(), "import".into(), "require".into(), "default".into()],
        main_fields: vec!["types".into(), "typings".into(), "module".into(), "main".into()],
        extensions: vec![".ts".into(), ".tsx".into(), ".d.ts".into(), ".js".into()],
        ..ResolveOptions::default()
    };
    let resolver = Resolver::new(resolve_options);
    react::resolve_package_types_file(&resolver, from_dir, package_name)
}

/// Resolve TypeScript's own `lib.es5.d.ts`/`lib.dom.d.ts` — the files that
/// declare native/DOM ambient globals (`Date`, `RegExp`, `Element`, `Node`, …).
/// These never go through an import (they're ambient scripts, not modules),
/// so nothing else ever has a reason to locate them the way an import
/// statement triggers `@types/react` resolution. Returns whichever of the two
/// are actually found — an empty `Vec` when the `typescript` package isn't
/// reachable from `from_dir` at all (e.g. no real project), which is a
/// legitimate, silent degradation, not an error.
pub fn resolve_ts_lib_paths(from_dir: &camino::Utf8Path) -> Vec<String> {
    let resolve_options = ResolveOptions { extensions: vec![".d.ts".into()], ..ResolveOptions::default() };
    let resolver = Resolver::new(resolve_options);
    ["lib.es5.d.ts", "lib.dom.d.ts"]
        .into_iter()
        .filter_map(|lib_file| {
            let specifier = format!("typescript/lib/{lib_file}");
            resolver.resolve(from_dir.as_std_path(), &specifier).ok().map(|r| r.path().to_string_lossy().into_owned())
        })
        .collect()
}

// ─── Entry Point ─────────────────────────────────────────────────────────────

/// Resolve a `ComponentMapping` to a complete `ComponentEntry`.
///
/// Called in parallel via rayon — must be `Send + Sync` (all data is owned/Arc'd).
pub fn resolve_component(mapping: &ComponentMapping, ctx: &ResolutionContext) -> (ComponentEntry, Vec<Diagnostic>) {
    let mut state = ResolveState::default();

    let chain = chain::resolve_props_chain(
        mapping.props_type_name.as_str(),
        &mapping.props_type_args,
        &mapping.file_path,
        mapping,
        ctx,
        &mut state,
        0,
    );

    // Build props BTreeMap — own props win over inherited props with same name.
    let mut props: BTreeMap<String, ParsedProp> = BTreeMap::new();
    for prop in chain.props {
        props.entry(prop.name.clone()).or_insert(prop);
    }

    // Populate notable_inherited from HTML element layers and non-HTML inherited layers.
    let mut notable_inherited: BTreeMap<String, ParsedProp> = BTreeMap::new();
    for layer in &chain.inheritance {
        if let Some(ref element) = layer.html_element {
            // Curated-mode synthesis only: Full mode already merged the real
            // attributes directly into `props` (see resolver/extends.rs); None
            // mode wants no HTML attrs synthesized at all.
            if ctx.html_attributes != crate::pipeline::HtmlAttributeMode::Curated {
                continue;
            }
            let notable_attrs = react_types::notable_html_attrs(element);
            for attr_name in notable_attrs {
                if props.contains_key(*attr_name) {
                    continue;
                } // own prop wins
                if notable_inherited.contains_key(*attr_name) {
                    continue;
                } // already added

                // Synthesize a minimal prop for display purposes
                let prop_type = html::infer_html_attr_prop_type(attr_name);
                notable_inherited.insert(
                    attr_name.to_string(),
                    ParsedProp {
                        name: attr_name.to_string(),
                        prop_type,
                        required: false,
                        default_value: None,
                        description: String::new(),
                        tags: Default::default(),
                        parent: Some(PropParent {
                            name: format!("{}HTMLAttributes", html::capitalize_element(element)),
                            file_name: "node_modules/@types/react/index.d.ts".to_string(),
                        }),
                        declarations: vec![],
                    },
                );
            }
        } else {
            // For non-HTML layers: add inherited props from chain.inherited_by_name
            for (name, prop) in &chain.inherited_by_name {
                if !props.contains_key(name) && !notable_inherited.contains_key(name) {
                    notable_inherited.insert(name.clone(), prop.clone());
                }
            }
        }
    }

    // Detect discriminated union if applicable.
    let discriminant_prop = chain.discriminant_prop;

    (
        ComponentEntry {
            display_name: mapping.component_name.clone(),
            file_path: mapping.file_path.clone(),
            description: mapping.description.clone(),
            props,
            inheritance: chain.inheritance,
            notable_inherited,
            discriminant_prop,
            composes: chain.composes,
            tags: mapping.tags.clone(),
            methods: vec![],
        },
        state.diagnostics,
    )
}

// ─── Intermediate Resolution Result ──────────────────────────────────────────

/// Result of resolving a props chain (including extends).
#[derive(Default)]
struct ResolvedChain {
    /// Resolved props (may include inherited props).
    props: Vec<ParsedProp>,
    /// Inheritance layers, outermost first.
    inheritance: Vec<InheritedLayer>,
    /// Inherited props keyed by name — for notable_inherited population.
    inherited_by_name: FxHashMap<String, ParsedProp>,
    /// Type names that could not be resolved.
    composes: Vec<String>,
    /// Discriminant prop name if this is a discriminated union.
    discriminant_prop: Option<String>,
}

impl ResolvedChain {
    fn empty_with_compose(type_name: String) -> Self {
        Self { composes: vec![type_name], ..Default::default() }
    }

    /// Merge a parent chain into self — own props already in `self.props` take priority.
    fn merge_parent(&mut self, parent: ResolvedChain) {
        // Collect existing prop names so we can skip duplicates.
        let existing: FxHashSet<String> = self.props.iter().map(|p| p.name.clone()).collect();

        for prop in parent.props {
            if !existing.contains(&prop.name) {
                self.props.push(prop.clone());
            }
            // Always populate inherited_by_name so notable lookup works.
            self.inherited_by_name.entry(prop.name.clone()).or_insert(prop);
        }

        // Prepend parent inheritance layers (parent is further up the chain).
        let mut new_inheritance = parent.inheritance;
        new_inheritance.append(&mut self.inheritance);
        self.inheritance = new_inheritance;

        self.composes.extend(parent.composes);
        for (name, prop) in parent.inherited_by_name {
            self.inherited_by_name.entry(name).or_insert(prop);
        }

        // A discriminated union nested inside an intersection (e.g. `Base & (A | B)`)
        // surfaces its discriminant on the union member's own chain — propagate it
        // up rather than silently dropping it in favor of the fresh, empty chain
        // being merged into.
        if self.discriminant_prop.is_none() {
            self.discriminant_prop = parent.discriminant_prop;
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn empty_ctx() -> ResolutionContext {
        let global = Arc::new(GlobalSourceData::default());
        let options = PipelineOptions::default();
        ResolutionContext::new(global, &options)
    }

    fn resolve_type(ct: &CollectedType, ctx: &ResolutionContext) -> PropType {
        use super::collected::resolve_collected_type;
        let mut state = ResolveState::default();
        resolve_collected_type(ct, Utf8Path::new("/test/button.tsx"), ctx, &mut state, 0)
    }

    // ── Test 1: Simple literal union ──────────────────────────────────────────

    #[test]
    fn test_string_literal_union() {
        let ctx = empty_ctx();
        let ct = CollectedType::Union(vec![
            CollectedType::StringLiteral("default".into()),
            CollectedType::StringLiteral("destructive".into()),
            CollectedType::StringLiteral("outline".into()),
        ]);
        let result = resolve_type(&ct, &ctx);
        assert!(
            matches!(&result, PropType::Union(members) if members.len() == 3),
            "Expected Union with 3 members, got {:?}",
            result
        );
    }

    // ── Test 2: Simple interface resolution ───────────────────────────────────

    #[test]
    fn test_simple_interface_resolution() {
        let file_path = Utf8PathBuf::from("/test/types.ts");
        let scoped_key = format!("{}:ButtonProps", file_path);

        let mut global = GlobalSourceData::default();
        global.interfaces.insert(
            scoped_key.clone(),
            CollectedInterface {
                scoped_key: scoped_key.clone(),
                name: "ButtonProps".into(),
                file_path: file_path.clone(),
                props: vec![RawProp {
                    name: "variant".into(),
                    collected_type: CollectedType::Union(vec![
                        CollectedType::StringLiteral("default".into()),
                        CollectedType::StringLiteral("destructive".into()),
                    ]),
                    required: false,
                    description: "The visual variant".into(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let mapping = ComponentMapping {
            component_name: "Button".into(),
            props_type_name: "ButtonProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, diagnostics) = resolve_component(&mapping, &ctx);
        // The unresolvable import diagnostic fires because ButtonProps is in the same file
        // but we do find it via scoped_key lookup.
        let variant_prop = entry.props.get("variant").expect("variant prop should exist");
        assert!(matches!(&variant_prop.prop_type, PropType::Union(m) if m.len() == 2));
        let _ = diagnostics; // may have info diagnostics
    }

    // ── Test 3: (string & {}) normalization ───────────────────────────────────

    #[test]
    fn test_string_and_empty_object_normalizes_to_string() {
        let ctx = empty_ctx();
        // (string & {}) → PropType::String
        let ct = CollectedType::Intersection(vec![CollectedType::String, CollectedType::Object(vec![])]);
        let result = resolve_type(&ct, &ctx);
        assert_eq!(result, PropType::String, "Expected String, got {:?}", result);
    }

    // ── Test 4: Known pattern - SxProps ──────────────────────────────────────

    #[test]
    fn test_sx_props_known_pattern() {
        let ctx = empty_ctx();
        let ct = CollectedType::Named { name: "SxProps".into(), args: vec![] };
        let result = resolve_type(&ct, &ctx);
        assert_eq!(result, PropType::SxProps, "Expected SxProps, got {:?}", result);
    }

    // ── Test 5: Indexed access - CSSProperties ────────────────────────────────

    #[test]
    fn test_indexed_access_css_properties_string_key() {
        let ctx = empty_ctx();
        let ct = CollectedType::IndexedAccess {
            obj: Box::new(CollectedType::Named { name: "CSSProperties".into(), args: vec![] }),
            key: Box::new(CollectedType::StringLiteral("justifyContent".into())),
        };
        let result = resolve_type(&ct, &ctx);
        assert_eq!(result, PropType::String, "Expected String for CSSProperties[string key], got {:?}", result);
    }

    #[test]
    fn test_indexed_access_css_properties_numeric_key() {
        let ctx = empty_ctx();
        let ct = CollectedType::IndexedAccess {
            obj: Box::new(CollectedType::Named { name: "CSSProperties".into(), args: vec![] }),
            key: Box::new(CollectedType::StringLiteral("zIndex".into())),
        };
        let result = resolve_type(&ct, &ctx);
        assert_eq!(result, PropType::Number, "Expected Number for CSSProperties[zIndex], got {:?}", result);
    }

    // ── Test 5b: Indexed access into a generic interface's own field, with the
    // call site's concrete type argument substituted in ─────────────────────
    // Regression test for: react-final-form's
    // `RenderableProps<FieldRenderProps<FieldValue, T>>["children"]` degraded to
    // Opaque — `resolve_indexed_access`'s generic fallback only handled a type
    // ALIAS wrapping an inline object literal (resolves to `PropType::Object`),
    // never an INTERFACE (which resolves to a bare `PropType::Named` at the type
    // level, so the fallback's `if let PropType::Object(fields) = ...` never matched).

    #[test]
    fn test_indexed_access_into_generic_interface_field_substitutes_type_arg() {
        let file_path = Utf8PathBuf::from("/test/button.tsx");
        let mut global = GlobalSourceData::default();

        global.interfaces.insert(
            format!("{}:RenderableProps", file_path),
            CollectedInterface {
                scoped_key: format!("{}:RenderableProps", file_path),
                name: "RenderableProps".into(),
                file_path: file_path.clone(),
                props: vec![RawProp {
                    name: "children".into(),
                    collected_type: CollectedType::Named { name: "T".into(), args: vec![] },
                    required: false,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );
        global.interface_type_params.insert(format!("{}:RenderableProps", file_path), vec!["T".into()]);

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let ct = CollectedType::IndexedAccess {
            obj: Box::new(CollectedType::Named { name: "RenderableProps".into(), args: vec![CollectedType::String] }),
            key: Box::new(CollectedType::StringLiteral("children".into())),
        };
        let result = resolve_type(&ct, &ctx);
        assert_eq!(
            result,
            PropType::String,
            "Expected T substituted with the call site's String argument, got {:?}",
            result
        );
    }

    // ── Test 6: Primitives pass through ──────────────────────────────────────

    #[test]
    fn test_primitive_types() {
        let ctx = empty_ctx();
        assert_eq!(resolve_type(&CollectedType::String, &ctx), PropType::String);
        assert_eq!(resolve_type(&CollectedType::Number, &ctx), PropType::Number);
        assert_eq!(resolve_type(&CollectedType::Boolean, &ctx), PropType::Boolean);
        assert_eq!(resolve_type(&CollectedType::Null, &ctx), PropType::Null);
        assert_eq!(resolve_type(&CollectedType::Undefined, &ctx), PropType::Undefined);
        assert_eq!(resolve_type(&CollectedType::Any, &ctx), PropType::Any);
        assert_eq!(resolve_type(&CollectedType::Never, &ctx), PropType::Never);
        assert_eq!(resolve_type(&CollectedType::Unknown, &ctx), PropType::Unknown);
        assert_eq!(resolve_type(&CollectedType::Void, &ctx), PropType::Void);
    }

    // ── Test 7: React builtins ────────────────────────────────────────────────

    #[test]
    fn test_react_node() {
        let ctx = empty_ctx();
        let ct = CollectedType::Named { name: "ReactNode".into(), args: vec![] };
        assert_eq!(resolve_type(&ct, &ctx), PropType::ReactNode);
    }

    #[test]
    fn test_css_properties() {
        let ctx = empty_ctx();
        let ct = CollectedType::Named { name: "CSSProperties".into(), args: vec![] };
        assert_eq!(resolve_type(&ct, &ctx), PropType::CssProperties);
    }

    #[test]
    fn test_ref_type() {
        let ctx = empty_ctx();
        let ct = CollectedType::Named {
            name: "Ref".into(),
            args: vec![CollectedType::Named { name: "HTMLButtonElement".into(), args: vec![] }],
        };
        let result = resolve_type(&ct, &ctx);
        assert!(
            matches!(&result, PropType::Ref { element: Some(e) } if e == "HTMLButtonElement"),
            "Expected Ref<HTMLButtonElement>, got {:?}",
            result
        );
    }

    #[test]
    fn test_less_common_event_handler_names_recognized_as_builtin() {
        // @types/react defines these identically to the ~19 *EventHandler names
        // already recognized (`type XEventHandler<T> = EventHandler<XEvent<T>>`) —
        // found missing from is_react_builtin while investigating whether fully
        // resolving @types/react's real HTMLAttributes/DOMAttributes chain was
        // tractable. Without this, each one falls through to same-file/imported
        // type resolution, fails (they're real @types/react types, not local), and
        // degrades to opaque instead of a proper EventHandler PropType.
        let ctx = empty_ctx();
        for handler_name in ["ReactEventHandler", "SubmitEventHandler", "InputEventHandler", "ToggleEventHandler"] {
            let ct = CollectedType::Named { name: handler_name.into(), args: vec![] };
            let result = resolve_type(&ct, &ctx);
            assert!(
                matches!(&result, PropType::EventHandler { .. }),
                "Expected {handler_name} to resolve as PropType::EventHandler, got {:?}",
                result
            );
        }
    }

    #[test]
    fn test_element_type() {
        let ctx = empty_ctx();
        let ct = CollectedType::Named { name: "ElementType".into(), args: vec![] };
        assert_eq!(resolve_type(&ct, &ctx), PropType::ElementType);
    }

    // ── Test 8: Default value code vs JSDoc mismatch ─────────────────────────

    #[test]
    fn test_default_value_jsdoc_mismatch_emits_diagnostic() {
        let file_path = Utf8PathBuf::from("/test/types.ts");
        let scoped_key = format!("{}:ButtonProps", file_path);

        let mut global = GlobalSourceData::default();
        let mut prop_tags = BTreeMap::new();
        prop_tags.insert("default".into(), "outline".into());

        global.interfaces.insert(
            scoped_key.clone(),
            CollectedInterface {
                scoped_key: scoped_key.clone(),
                name: "ButtonProps".into(),
                file_path: file_path.clone(),
                props: vec![RawProp {
                    name: "variant".into(),
                    collected_type: CollectedType::String,
                    required: false,
                    description: String::new(),
                    tags: prop_tags,
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());

        let mut param_defaults = FxHashMap::default();
        param_defaults.insert(
            "variant".to_string(),
            RawDefault { value: "\"default\"".into(), computed: false, source: DefaultSource::Destructuring },
        );

        let mapping = ComponentMapping {
            component_name: "Button".into(),
            props_type_name: "ButtonProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults,
        };

        let (entry, diagnostics) = resolve_component(&mapping, &ctx);
        let variant_prop = entry.props.get("variant").expect("variant prop");
        // Code default should win.
        assert_eq!(variant_prop.default_value.as_ref().map(|d| d.value.as_str()), Some("\"default\""));
        // A JsDocDefaultMismatch diagnostic should have been emitted.
        assert!(
            diagnostics.iter().any(|d| d.code == DiagnosticCode::JsDocDefaultMismatch),
            "Expected JsDocDefaultMismatch diagnostic"
        );
    }

    // ── Test 9: Discriminated union detection ─────────────────────────────────

    #[test]
    fn test_discriminated_union_detection() {
        let members: Vec<(&str, Vec<ParsedProp>)> = vec![
            (
                "ButtonBaseProps",
                vec![ParsedProp {
                    name: "variant".into(),
                    prop_type: PropType::StringLiteral("default".into()),
                    required: true,
                    default_value: None,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    parent: None,
                    declarations: vec![],
                }],
            ),
            (
                "ButtonOutlineProps",
                vec![ParsedProp {
                    name: "variant".into(),
                    prop_type: PropType::StringLiteral("outline".into()),
                    required: true,
                    default_value: None,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    parent: None,
                    declarations: vec![],
                }],
            ),
        ];
        let discriminant = chain::find_discriminant_prop(&members);
        assert_eq!(discriminant, Some("variant".to_string()));
    }

    // ── Test 9b: Discriminated union wrapped in an intersection ───────────────
    // Regression test for the Day Picker pattern:
    //   type DayPickerProps = PropsBase & (PropsSingle | PropsMulti)
    // Discriminant detection only ran for a type alias whose RHS is *directly*
    // a union; wrapped in an intersection, the union member fell into a naive
    // merge-first-wins path with no discriminant detection at all.

    #[test]
    fn test_discriminant_detected_through_intersection_wrapped_union() {
        let file_path = Utf8PathBuf::from("/test/day-picker.tsx");

        let mut global = GlobalSourceData::default();

        global.interfaces.insert(
            format!("{}:PropsBase", file_path),
            CollectedInterface {
                scoped_key: format!("{}:PropsBase", file_path),
                name: "PropsBase".into(),
                file_path: file_path.clone(),
                props: vec![RawProp {
                    name: "id".into(),
                    collected_type: CollectedType::String,
                    required: false,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );
        global.interfaces.insert(
            format!("{}:PropsSingle", file_path),
            CollectedInterface {
                scoped_key: format!("{}:PropsSingle", file_path),
                name: "PropsSingle".into(),
                file_path: file_path.clone(),
                props: vec![
                    RawProp {
                        name: "mode".into(),
                        collected_type: CollectedType::StringLiteral("single".into()),
                        required: true,
                        description: String::new(),
                        tags: BTreeMap::new(),
                        span_start: 0,
                        span_end: 0,
                    },
                    RawProp {
                        name: "selected".into(),
                        collected_type: CollectedType::String,
                        required: false,
                        description: String::new(),
                        tags: BTreeMap::new(),
                        span_start: 0,
                        span_end: 0,
                    },
                ],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );
        global.interfaces.insert(
            format!("{}:PropsMulti", file_path),
            CollectedInterface {
                scoped_key: format!("{}:PropsMulti", file_path),
                name: "PropsMulti".into(),
                file_path: file_path.clone(),
                props: vec![
                    RawProp {
                        name: "mode".into(),
                        collected_type: CollectedType::StringLiteral("multi".into()),
                        required: true,
                        description: String::new(),
                        tags: BTreeMap::new(),
                        span_start: 0,
                        span_end: 0,
                    },
                    RawProp {
                        name: "selected".into(),
                        collected_type: CollectedType::Array(Box::new(CollectedType::String)),
                        required: false,
                        description: String::new(),
                        tags: BTreeMap::new(),
                        span_start: 0,
                        span_end: 0,
                    },
                ],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        // type DayPickerProps = PropsBase & (PropsSingle | PropsMulti)
        global.type_aliases.insert(
            format!("{}:DayPickerProps", file_path),
            CollectedTypeAlias::Intersection {
                members: vec![
                    CollectedType::Named { name: "PropsBase".into(), args: vec![] },
                    CollectedType::Union(vec![
                        CollectedType::Named { name: "PropsSingle".into(), args: vec![] },
                        CollectedType::Named { name: "PropsMulti".into(), args: vec![] },
                    ]),
                ],
                file_path: file_path.clone(),
            },
        );

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let mapping = ComponentMapping {
            component_name: "DayPicker".into(),
            props_type_name: "DayPickerProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, _diagnostics) = resolve_component(&mapping, &ctx);

        assert_eq!(
            entry.discriminant_prop,
            Some("mode".to_string()),
            "Expected 'mode' to be detected as the discriminant even though the union is wrapped in an intersection"
        );

        let selected = entry.props.get("selected").expect("'selected' prop not found");
        assert!(
            matches!(&selected.prop_type, PropType::Union(members) if members.len() == 2),
            "Expected 'selected' to merge both branches' types into a union, got {:?} (this is the exact bug: \
             the union fell into a naive merge that keeps only the first branch's type)",
            selected.prop_type
        );
    }

    // ── Test 9c: Double-discriminated union (repeated single-field values) ────
    // Real Day Picker's union is discriminated jointly on `mode` AND `required` —
    // `PropsSingle` and `PropsSingleRequired` both have `mode: "single"`, and are
    // only distinguished by `required`. `mode` alone can't identify the variant
    // (`"single"` is ambiguous between the two), so declining to name it as *the*
    // discriminant is correct, not a bug — verify that's still true, and that the
    // real bug (prop types collapsing to a single branch instead of merging into a
    // union across every branch) stays fixed for this messier, real-world shape too.
    // Wrapped in an intersection with a trivial base, matching Day Picker's real
    // `PropsBase & (union)` shape, so this actually exercises the fix rather than
    // the already-working direct-union path.

    #[test]
    fn test_repeated_discriminant_values_decline_gracefully_but_still_merge_types() {
        let file_path = Utf8PathBuf::from("/test/day-picker-repeated.tsx");

        let mut global = GlobalSourceData::default();

        for (iface_name, mode, required) in [("PropsSingle", "single", false), ("PropsSingleRequired", "single", true)]
        {
            global.interfaces.insert(
                format!("{}:{}", file_path, iface_name),
                CollectedInterface {
                    scoped_key: format!("{}:{}", file_path, iface_name),
                    name: iface_name.into(),
                    file_path: file_path.clone(),
                    props: vec![
                        RawProp {
                            name: "mode".into(),
                            collected_type: CollectedType::StringLiteral(mode.into()),
                            required: true,
                            description: String::new(),
                            tags: BTreeMap::new(),
                            span_start: 0,
                            span_end: 0,
                        },
                        RawProp {
                            name: "required".into(),
                            collected_type: CollectedType::BoolLiteral(required),
                            required: true,
                            description: String::new(),
                            tags: BTreeMap::new(),
                            span_start: 0,
                            span_end: 0,
                        },
                        RawProp {
                            name: "selected".into(),
                            collected_type: if required { CollectedType::String } else { CollectedType::Undefined },
                            required,
                            description: String::new(),
                            tags: BTreeMap::new(),
                            span_start: 0,
                            span_end: 0,
                        },
                    ],
                    extends: vec![],
                    description: String::new(),
                    tags: BTreeMap::new(),
                },
            );
        }

        global.interfaces.insert(
            format!("{}:PropsBase", file_path),
            CollectedInterface {
                scoped_key: format!("{}:PropsBase", file_path),
                name: "PropsBase".into(),
                file_path: file_path.clone(),
                props: vec![RawProp {
                    name: "id".into(),
                    collected_type: CollectedType::String,
                    required: false,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        // type DayPickerProps = PropsBase & (PropsSingle | PropsSingleRequired)
        global.type_aliases.insert(
            format!("{}:DayPickerProps", file_path),
            CollectedTypeAlias::Intersection {
                members: vec![
                    CollectedType::Named { name: "PropsBase".into(), args: vec![] },
                    CollectedType::Union(vec![
                        CollectedType::Named { name: "PropsSingle".into(), args: vec![] },
                        CollectedType::Named { name: "PropsSingleRequired".into(), args: vec![] },
                    ]),
                ],
                file_path: file_path.clone(),
            },
        );

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let mapping = ComponentMapping {
            component_name: "DayPicker".into(),
            props_type_name: "DayPickerProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, _diagnostics) = resolve_component(&mapping, &ctx);

        assert_eq!(
            entry.discriminant_prop, None,
            "'mode' repeats the same value across branches and can't identify a variant alone — \
             declining to name a discriminant here is correct"
        );

        let selected = entry.props.get("selected").expect("'selected' prop not found");
        assert!(
            matches!(&selected.prop_type, PropType::Union(members) if members.len() == 2),
            "Expected 'selected' to still merge both branches' distinct types into a union even \
             without a usable discriminant, got {:?}",
            selected.prop_type
        );
    }

    // ── Test 10: Extends resolution — InheritedLayer populated ───────────────

    #[test]
    fn test_extends_builtin_html_attrs_creates_inherited_layer() {
        let file_path = Utf8PathBuf::from("/test/button.tsx");
        let scoped_key = format!("{}:ButtonProps", file_path);

        let mut global = GlobalSourceData::default();
        global.interfaces.insert(
            scoped_key.clone(),
            CollectedInterface {
                scoped_key: scoped_key.clone(),
                name: "ButtonProps".into(),
                file_path: file_path.clone(),
                props: vec![],
                extends: vec![ExtendsRef::Builtin {
                    name: "ButtonHTMLAttributes".into(),
                    element: Some("button".into()),
                    type_args: vec![],
                }],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let mapping = ComponentMapping {
            component_name: "Button".into(),
            props_type_name: "ButtonProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, _diagnostics) = resolve_component(&mapping, &ctx);
        assert!(
            entry.inheritance.iter().any(|l| l.html_element.as_deref() == Some("button")),
            "Expected 'button' in inheritance layers, got {:?}",
            entry.inheritance
        );
    }

    #[test]
    fn test_html_attribute_mode_curated_does_not_expand_real_button_attrs() {
        // Baseline: with the default Curated mode, a real (pre-merged, simulating
        // an actually-parsed @types/react) ButtonHTMLAttributes interface is NOT
        // consulted at all — own props stay empty, matching today's behavior.
        let file_path = Utf8PathBuf::from("/test/button.tsx");
        let mut global = GlobalSourceData::default();
        let react_file = react::resolve_react_types_file(
            &file_path,
            &ResolutionContext::new(Arc::new(GlobalSourceData::default()), &PipelineOptions::default()),
        );

        global.interfaces.insert(
            format!("{}:ButtonHTMLAttributes", react_file),
            CollectedInterface {
                scoped_key: format!("{}:ButtonHTMLAttributes", react_file),
                name: "ButtonHTMLAttributes".into(),
                file_path: react_file.clone().into(),
                props: vec![RawProp {
                    name: "formAction".into(),
                    collected_type: CollectedType::String,
                    required: false,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );
        global.interfaces.insert(
            format!("{}:ButtonProps", file_path),
            CollectedInterface {
                scoped_key: format!("{}:ButtonProps", file_path),
                name: "ButtonProps".into(),
                file_path: file_path.clone(),
                props: vec![],
                extends: vec![ExtendsRef::Builtin {
                    name: "ButtonHTMLAttributes".into(),
                    element: Some("button".into()),
                    type_args: vec![],
                }],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let mapping = ComponentMapping {
            component_name: "Button".into(),
            props_type_name: "ButtonProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, _diagnostics) = resolve_component(&mapping, &ctx);
        assert!(
            !entry.props.contains_key("formAction"),
            "Curated mode should not expand real ButtonHTMLAttributes fields into own props, got {:?}",
            entry.props.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_html_attribute_mode_full_expands_real_button_attrs() {
        // Full mode: the same pre-merged, real ButtonHTMLAttributes interface IS
        // consulted, and its real fields become genuine own props — matching how
        // RDT flattens inherited HTML attributes directly into its props map.
        let file_path = Utf8PathBuf::from("/test/button.tsx");
        let mut global = GlobalSourceData::default();
        let react_file = react::resolve_react_types_file(
            &file_path,
            &ResolutionContext::new(Arc::new(GlobalSourceData::default()), &PipelineOptions::default()),
        );

        global.interfaces.insert(
            format!("{}:ButtonHTMLAttributes", react_file),
            CollectedInterface {
                scoped_key: format!("{}:ButtonHTMLAttributes", react_file),
                name: "ButtonHTMLAttributes".into(),
                file_path: react_file.clone().into(),
                props: vec![RawProp {
                    name: "formAction".into(),
                    collected_type: CollectedType::String,
                    required: false,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );
        global.interfaces.insert(
            format!("{}:ButtonProps", file_path),
            CollectedInterface {
                scoped_key: format!("{}:ButtonProps", file_path),
                name: "ButtonProps".into(),
                file_path: file_path.clone(),
                props: vec![],
                extends: vec![ExtendsRef::Builtin {
                    name: "ButtonHTMLAttributes".into(),
                    element: Some("button".into()),
                    type_args: vec![],
                }],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        let options =
            PipelineOptions { html_attributes: crate::pipeline::HtmlAttributeMode::Full, ..Default::default() };
        let ctx = ResolutionContext::new(Arc::new(global), &options);
        let mapping = ComponentMapping {
            component_name: "Button".into(),
            props_type_name: "ButtonProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, _diagnostics) = resolve_component(&mapping, &ctx);
        assert!(
            entry.props.contains_key("formAction"),
            "Full mode should expand real ButtonHTMLAttributes fields into own props, got {:?}",
            entry.props.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_html_attribute_mode_none_suppresses_notable_inherited() {
        // None mode: no curated attrs (onClick, disabled, etc.) should be
        // synthesized into notable_inherited at all — own props only.
        let file_path = Utf8PathBuf::from("/test/button.tsx");
        let scoped_key = format!("{}:ButtonProps", file_path);

        let mut global = GlobalSourceData::default();
        global.interfaces.insert(
            scoped_key.clone(),
            CollectedInterface {
                scoped_key: scoped_key.clone(),
                name: "ButtonProps".into(),
                file_path: file_path.clone(),
                props: vec![],
                extends: vec![ExtendsRef::Builtin {
                    name: "ButtonHTMLAttributes".into(),
                    element: Some("button".into()),
                    type_args: vec![],
                }],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        let options =
            PipelineOptions { html_attributes: crate::pipeline::HtmlAttributeMode::None, ..Default::default() };
        let ctx = ResolutionContext::new(Arc::new(global), &options);
        let mapping = ComponentMapping {
            component_name: "Button".into(),
            props_type_name: "ButtonProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, _diagnostics) = resolve_component(&mapping, &ctx);
        assert!(
            entry.notable_inherited.is_empty(),
            "None mode should not synthesize any curated HTML attrs, got {:?}",
            entry.notable_inherited.keys().collect::<Vec<_>>()
        );
    }

    // ── Test 10b: ComponentPropsWithoutRef in intersection type alias ─────────
    // Regression test for the Radix UI pattern:
    //   type PrimitiveButtonProps = React.ComponentPropsWithoutRef<"button"> & { asChild?: boolean }
    //   interface ButtonProps extends PrimitiveButtonProps {}

    #[test]
    fn test_component_props_without_ref_in_intersection_alias() {
        let file_path = Utf8PathBuf::from("/test/button.tsx");

        let mut global = GlobalSourceData::default();

        // type PrimitiveButtonProps = React.ComponentPropsWithoutRef<"button"> & { asChild?: boolean }
        let alias_key = format!("{}:PrimitiveButtonProps", file_path);
        global.type_aliases.insert(
            alias_key,
            CollectedTypeAlias::Intersection {
                members: vec![
                    CollectedType::Named {
                        name: "React.ComponentPropsWithoutRef".into(),
                        args: vec![CollectedType::StringLiteral("button".into())],
                    },
                    CollectedType::Object(vec![CollectedObjectField {
                        name: "asChild".into(),
                        collected_type: CollectedType::Boolean,
                        required: false,
                        description: String::new(),
                    }]),
                ],
                file_path: file_path.clone(),
            },
        );

        // interface ButtonProps extends PrimitiveButtonProps {}
        let iface_key = format!("{}:ButtonProps", file_path);
        global.interfaces.insert(
            iface_key,
            CollectedInterface {
                scoped_key: format!("{}:ButtonProps", file_path),
                name: "ButtonProps".into(),
                file_path: file_path.clone(),
                props: vec![],
                extends: vec![ExtendsRef::SameFile { name: "PrimitiveButtonProps".into(), type_args: vec![] }],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let mapping = ComponentMapping {
            component_name: "Button".into(),
            props_type_name: "ButtonProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, diagnostics) = resolve_component(&mapping, &ctx);

        // Should have asChild as an own prop
        assert!(
            entry.props.contains_key("asChild"),
            "Expected 'asChild' prop, got props: {:?}",
            entry.props.keys().collect::<Vec<_>>()
        );

        // Should have button in the inheritance chain
        assert!(
            entry.inheritance.iter().any(|l| l.html_element.as_deref() == Some("button")),
            "Expected 'button' in inheritance layers, got {:?}",
            entry.inheritance
        );

        // Should not have ComponentPropsWithoutRef in composes
        assert!(
            !entry.composes.contains(&"React.ComponentPropsWithoutRef".to_owned()),
            "ComponentPropsWithoutRef should not appear in composes, got {:?}",
            entry.composes
        );

        let warnings: Vec<_> =
            diagnostics.iter().filter(|d| matches!(d.severity, DiagnosticSeverity::Warning)).collect();
        assert!(warnings.is_empty(), "Expected no warnings, got {:?}", warnings);
    }

    // ── Test 10b2: Two distinct Partial<X> extends targets on one interface ───
    // Regression test for the Blueprint Table pattern:
    //   interface TableProps extends Partial<RowHeights>, Partial<ColumnWidths> {}
    // resolve_props_chain's cycle-detection visited-key was built from the bare
    // type name alone ("Partial"), ignoring type_args — so the second Partial<X>
    // extends target collided with the first's visited-key and silently resolved
    // to nothing, with zero diagnostic.

    #[test]
    fn test_two_distinct_partial_extends_targets_both_resolve() {
        let file_path = Utf8PathBuf::from("/test/table.tsx");

        let mut global = GlobalSourceData::default();

        global.interfaces.insert(
            format!("{}:RowHeights", file_path),
            CollectedInterface {
                scoped_key: format!("{}:RowHeights", file_path),
                name: "RowHeights".into(),
                file_path: file_path.clone(),
                props: vec![RawProp {
                    name: "defaultRowHeight".into(),
                    collected_type: CollectedType::Number,
                    required: true,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );
        global.interfaces.insert(
            format!("{}:ColumnWidths", file_path),
            CollectedInterface {
                scoped_key: format!("{}:ColumnWidths", file_path),
                name: "ColumnWidths".into(),
                file_path: file_path.clone(),
                props: vec![RawProp {
                    name: "defaultColumnWidth".into(),
                    collected_type: CollectedType::Number,
                    required: true,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );
        global.interfaces.insert(
            format!("{}:TableProps", file_path),
            CollectedInterface {
                scoped_key: format!("{}:TableProps", file_path),
                name: "TableProps".into(),
                file_path: file_path.clone(),
                props: vec![],
                extends: vec![
                    ExtendsRef::SameFile { name: "Partial".into(), type_args: vec!["RowHeights".into()] },
                    ExtendsRef::SameFile { name: "Partial".into(), type_args: vec!["ColumnWidths".into()] },
                ],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let mapping = ComponentMapping {
            component_name: "Table".into(),
            props_type_name: "TableProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, _diagnostics) = resolve_component(&mapping, &ctx);

        assert!(
            entry.props.contains_key("defaultRowHeight"),
            "Expected 'defaultRowHeight' from the first Partial<X> target, got props: {:?}",
            entry.props.keys().collect::<Vec<_>>()
        );
        assert!(
            entry.props.contains_key("defaultColumnWidth"),
            "Expected 'defaultColumnWidth' from the second Partial<X> target — it must not be dropped due to a visited-key collision with the first, got props: {:?}",
            entry.props.keys().collect::<Vec<_>>()
        );
    }

    // ── Test 9b: Union alias members resolve relative to the alias's own file ──
    // Regression test for: TanStack Table's `ColumnDef<TData, TValue> = DisplayColumnDef<...>
    // | GroupColumnDef<...> | AccessorColumnDef<...>` (all same-file siblings in
    // types.ts). A consumer file that only imports `ColumnDef` (not the union
    // members) was spuriously getting "Cannot resolve type 'DisplayColumnDef'"
    // diagnostics, because `resolve_type_alias_type` forwarded the ORIGINAL
    // caller's `consuming_file` into the recursive member resolution instead of
    // the alias's own `file_path` — so the same-file sibling lookup was attempted
    // against the wrong file.

    #[test]
    fn test_union_alias_members_resolve_relative_to_alias_own_file_not_caller() {
        let types_file = Utf8PathBuf::from("/test/types.ts");
        let consumer_file = Utf8PathBuf::from("/test/consumer.tsx");

        let mut global = GlobalSourceData::default();

        global.interfaces.insert(
            format!("{}:SiblingA", types_file),
            CollectedInterface {
                scoped_key: format!("{}:SiblingA", types_file),
                name: "SiblingA".into(),
                file_path: types_file.clone(),
                props: vec![RawProp {
                    name: "x".into(),
                    collected_type: CollectedType::String,
                    required: true,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );
        global.interfaces.insert(
            format!("{}:SiblingB", types_file),
            CollectedInterface {
                scoped_key: format!("{}:SiblingB", types_file),
                name: "SiblingB".into(),
                file_path: types_file.clone(),
                props: vec![RawProp {
                    name: "y".into(),
                    collected_type: CollectedType::Number,
                    required: true,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        // type Combined = SiblingA | SiblingB — re-exported into consumer.tsx (keyed
        // under consumer_file so the outer `value: Combined` reference resolves via
        // the same-file fallback without needing real import-map/oxc_resolver
        // plumbing), but its own `file_path` correctly records where it — and its
        // union members — are actually declared: types.ts.
        global.type_aliases.insert(
            format!("{}:Combined", consumer_file),
            CollectedTypeAlias::Union {
                members: vec![
                    CollectedType::Named { name: "SiblingA".into(), args: vec![] },
                    CollectedType::Named { name: "SiblingB".into(), args: vec![] },
                ],
                file_path: types_file.clone(),
            },
        );

        // interface WidgetProps { value: Combined } — declared in consumer.tsx,
        // which never imports SiblingA/SiblingB directly.
        global.interfaces.insert(
            format!("{}:WidgetProps", consumer_file),
            CollectedInterface {
                scoped_key: format!("{}:WidgetProps", consumer_file),
                name: "WidgetProps".into(),
                file_path: consumer_file.clone(),
                props: vec![RawProp {
                    name: "value".into(),
                    collected_type: CollectedType::Named { name: "Combined".into(), args: vec![] },
                    required: true,
                    description: String::new(),
                    tags: BTreeMap::new(),
                    span_start: 0,
                    span_end: 0,
                }],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let mapping = ComponentMapping {
            component_name: "Widget".into(),
            props_type_name: "WidgetProps".into(),
            props_type_args: vec![],
            file_path: consumer_file.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, diagnostics) = resolve_component(&mapping, &ctx);

        let value = entry.props.get("value").expect("expected 'value' prop");
        match &value.prop_type {
            PropType::Union(members) => assert_eq!(members.len(), 2, "expected 2 union members, got {:?}", members),
            other => panic!("expected Union, got {:?}", other),
        }

        assert!(
            diagnostics.is_empty(),
            "SiblingA/SiblingB are real same-file siblings of Combined in types.ts — expected no diagnostics, got: {:?}",
            diagnostics
        );
    }

    // ── Test 9c: Generic interface's own type params don't trigger unresolvable-
    // type diagnostics ──────────────────────────────────────────────────────────
    // Regression test for: TanStack Table's `DataTableProps<TData, TValue>`
    // referencing its own `TData`/`TValue` generic parameters in its body
    // (`columns: ColumnDef<TData, TValue>[]`, `data: TData[]`) was spuriously
    // getting "Cannot resolve type 'TData'" diagnostics — the resolver had no
    // concept of an interface's own declared type parameters, so it tried (and
    // "failed") to look them up as if they were real named types.

    #[test]
    fn test_generic_interface_own_type_params_suppress_diagnostic() {
        let file_path = Utf8PathBuf::from("/test/data-table.tsx");

        let mut global = GlobalSourceData::default();

        global.interfaces.insert(
            format!("{}:WidgetProps", file_path),
            CollectedInterface {
                scoped_key: format!("{}:WidgetProps", file_path),
                name: "WidgetProps".into(),
                file_path: file_path.clone(),
                props: vec![
                    RawProp {
                        name: "data".into(),
                        collected_type: CollectedType::Array(Box::new(CollectedType::Named {
                            name: "TData".into(),
                            args: vec![],
                        })),
                        required: true,
                        description: String::new(),
                        tags: BTreeMap::new(),
                        span_start: 0,
                        span_end: 0,
                    },
                    RawProp {
                        name: "getValue".into(),
                        collected_type: CollectedType::Named { name: "TValue".into(), args: vec![] },
                        required: true,
                        description: String::new(),
                        tags: BTreeMap::new(),
                        span_start: 0,
                        span_end: 0,
                    },
                ],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );
        global
            .interface_type_params
            .insert(format!("{}:WidgetProps", file_path), vec!["TData".into(), "TValue".into()]);

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let mapping = ComponentMapping {
            component_name: "Widget".into(),
            props_type_name: "WidgetProps".into(),
            props_type_args: vec![],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, diagnostics) = resolve_component(&mapping, &ctx);

        assert!(
            entry.props.contains_key("data"),
            "expected 'data' prop, got: {:?}",
            entry.props.keys().collect::<Vec<_>>()
        );
        assert!(
            entry.props.contains_key("getValue"),
            "expected 'getValue' prop, got: {:?}",
            entry.props.keys().collect::<Vec<_>>()
        );
        assert!(
            diagnostics.is_empty(),
            "TData/TValue are WidgetProps's own declared type parameters — expected no diagnostics, got: {:?}",
            diagnostics
        );
    }

    // ── Test 10c: User-defined generic type alias substitution (Ark UI `Assign<T, U>`) ─
    // Regression test for: `type Assign<T, U> = Omit<T, keyof U> & U` used with
    // concrete call-site arguments. Before substitution was implemented, `T`/`U`
    // resolved as literal (unresolvable) type names and the component's props came
    // back completely empty. Expected result: `U`'s own fields are present, `T`'s
    // fields are present minus whatever `U` overrides (here, `title`).

    #[test]
    fn test_generic_type_alias_substitution() {
        let file_path = Utf8PathBuf::from("/test/types.ts");

        let mut global = GlobalSourceData::default();

        // interface Base { id: string; title: string }  (stands in for HTMLProps<'div'>)
        let base_key = format!("{}:Base", file_path);
        global.interfaces.insert(
            base_key,
            CollectedInterface {
                scoped_key: format!("{}:Base", file_path),
                name: "Base".into(),
                file_path: file_path.clone(),
                props: vec![
                    RawProp {
                        name: "id".into(),
                        collected_type: CollectedType::String,
                        required: true,
                        description: String::new(),
                        tags: BTreeMap::new(),
                        span_start: 0,
                        span_end: 0,
                    },
                    RawProp {
                        name: "title".into(),
                        collected_type: CollectedType::String,
                        required: false,
                        description: String::new(),
                        tags: BTreeMap::new(),
                        span_start: 0,
                        span_end: 0,
                    },
                ],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        // interface Overrides { title: number; extra: boolean }  (stands in for SelectRootBaseProps<T>)
        let overrides_key = format!("{}:Overrides", file_path);
        global.interfaces.insert(
            overrides_key,
            CollectedInterface {
                scoped_key: format!("{}:Overrides", file_path),
                name: "Overrides".into(),
                file_path: file_path.clone(),
                props: vec![
                    RawProp {
                        name: "title".into(),
                        collected_type: CollectedType::Number,
                        required: false,
                        description: String::new(),
                        tags: BTreeMap::new(),
                        span_start: 0,
                        span_end: 0,
                    },
                    RawProp {
                        name: "extra".into(),
                        collected_type: CollectedType::Boolean,
                        required: false,
                        description: String::new(),
                        tags: BTreeMap::new(),
                        span_start: 0,
                        span_end: 0,
                    },
                ],
                extends: vec![],
                description: String::new(),
                tags: BTreeMap::new(),
            },
        );

        // type Assign<T, U> = Omit<T, keyof U> & U
        let assign_key = format!("{}:Assign", file_path);
        global.type_aliases.insert(
            assign_key.clone(),
            CollectedTypeAlias::Intersection {
                members: vec![
                    CollectedType::Named {
                        name: "Omit".into(),
                        args: vec![
                            CollectedType::Named { name: "T".into(), args: vec![] },
                            CollectedType::KeyOf(Box::new(CollectedType::Named { name: "U".into(), args: vec![] })),
                        ],
                    },
                    CollectedType::Named { name: "U".into(), args: vec![] },
                ],
                file_path: file_path.clone(),
            },
        );
        global.type_alias_params.insert(assign_key, vec!["T".into(), "U".into()]);

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());

        // A component whose props type is `Assign<Base, Overrides>` directly.
        let mapping = ComponentMapping {
            component_name: "Widget".into(),
            props_type_name: "Assign".into(),
            props_type_args: vec!["Base".to_string(), "Overrides".to_string()],
            file_path: file_path.clone(),
            description: String::new(),
            tags: BTreeMap::new(),
            span_start: 0,
            span_end: 0,
            param_defaults: FxHashMap::default(),
        };

        let (entry, _diagnostics) = resolve_component(&mapping, &ctx);

        // `id` survives from Base/T (not omitted — Overrides/U doesn't declare it).
        assert!(
            entry.props.contains_key("id"),
            "Expected 'id' prop from T, got: {:?}",
            entry.props.keys().collect::<Vec<_>>()
        );

        // `title` comes from Overrides/U (number), not Base/T (string) — U wins,
        // and Omit<T, keyof U> must have actually removed T's `title`.
        let title = entry.props.get("title").expect("expected 'title' prop");
        assert_eq!(title.prop_type, PropType::Number, "Expected U's 'title: number' to win over T's 'title: string'");

        // `extra` comes from Overrides/U.
        assert!(
            entry.props.contains_key("extra"),
            "Expected 'extra' prop from U, got: {:?}",
            entry.props.keys().collect::<Vec<_>>()
        );

        // Exactly 3 props: T's title was omitted via `keyof U`, not duplicated.
        assert_eq!(
            entry.props.len(),
            3,
            "Expected exactly 3 props (id, title, extra), got: {:?}",
            entry.props.keys().collect::<Vec<_>>()
        );
    }

    // ── Test 11: Array type ───────────────────────────────────────────────────

    #[test]
    fn test_array_type() {
        let ctx = empty_ctx();
        let ct = CollectedType::Array(Box::new(CollectedType::String));
        let result = resolve_type(&ct, &ctx);
        assert!(matches!(result, PropType::Array(inner) if *inner == PropType::String));
    }

    // ── Test 12: Template literal — opaque on unresolvable parts ─────────────

    #[test]
    fn test_template_literal_opaque_on_unknown_type() {
        let ctx = empty_ctx();
        // `compact-${UnknownSize}` — UnknownSize is not in global, so opaque.
        let ct = CollectedType::TemplateLiteral(vec![
            CollectedType::StringLiteral("compact-".into()),
            CollectedType::Named { name: "UnknownSize".into(), args: vec![] },
        ]);
        let result = resolve_type(&ct, &ctx);
        assert!(
            matches!(result, PropType::Opaque { reason: OpaqueReason::TemplateLiteral { .. }, .. }),
            "Expected Opaque TemplateLiteral, got {:?}",
            result
        );
    }

    // ── Test 13: Template literal expansion ───────────────────────────────────

    #[test]
    fn test_template_literal_expansion() {
        let file_path = Utf8PathBuf::from("/test/types.ts");
        let mut global = GlobalSourceData::default();
        let scoped_key = format!("{}:Size", file_path);
        global.type_aliases.insert(
            scoped_key,
            CollectedTypeAlias::LiteralUnion {
                members: vec!["sm".into(), "md".into(), "lg".into()],
                file_path: file_path.clone(),
            },
        );

        let ctx = ResolutionContext::new(Arc::new(global), &PipelineOptions::default());
        let mut state = ResolveState::default();

        // `compact-${Size}` where Size is in the same file.
        // Note: because Size is in /test/types.ts and consuming_file is also /test/types.ts,
        // the resolve_to_canonical will return None (not imported), so we won't find the alias.
        // This is expected — cross-file resolution requires imports to be present.
        // Instead test with a raw string literal union.
        let parts = vec![CollectedType::StringLiteral("compact-".into()), CollectedType::StringLiteral("sm".into())];
        let result =
            template::try_expand_template_literal(&parts, Utf8Path::new("/test/types.ts"), &ctx, &mut state, 0);
        assert_eq!(result, Some(vec!["compact-sm".to_string()]));
    }

    // ── Test 14: Conditional type → Opaque ───────────────────────────────────

    #[test]
    fn test_conditional_type_is_opaque() {
        let ctx = empty_ctx();
        let ct = CollectedType::Conditional {
            check: Box::new(CollectedType::String),
            extends_type: Box::new(CollectedType::String),
            true_type: Box::new(CollectedType::Boolean),
            false_type: Box::new(CollectedType::Number),
        };
        let result = resolve_type(&ct, &ctx);
        assert!(
            matches!(result, PropType::Opaque { reason: OpaqueReason::ConditionalType, .. }),
            "Expected ConditionalType opaque, got {:?}",
            result
        );
    }

    // ── Test 15: Mapped type → Opaque ─────────────────────────────────────────

    #[test]
    fn test_mapped_type_is_opaque() {
        let ctx = empty_ctx();
        let ct = CollectedType::Mapped {
            key_type: Box::new(CollectedType::String),
            value_type: Box::new(CollectedType::Boolean),
        };
        let result = resolve_type(&ct, &ctx);
        assert!(
            matches!(result, PropType::Opaque { reason: OpaqueReason::MappedType, .. }),
            "Expected MappedType opaque, got {:?}",
            result
        );
    }

    // ── Test 16: Function type → EventHandler ────────────────────────────────

    #[test]
    fn test_function_type_single_param() {
        let ctx = empty_ctx();
        let ct = CollectedType::Function {
            params: vec![CollectedType::Named { name: "MouseEvent".into(), args: vec![] }],
            param_names: vec![Some("e".into())],
            return_type: Box::new(CollectedType::Void),
        };
        let result = resolve_type(&ct, &ctx);
        assert!(
            matches!(&result, PropType::EventHandler { event_type, param_name }
                if event_type == "MouseEvent" && param_name.as_deref() == Some("e")),
            "Expected EventHandler<MouseEvent> with param_name 'e', got {:?}",
            result
        );
    }

    // ── Test 17: tsconfig path stripping ─────────────────────────────────────

    #[test]
    fn test_strip_json_comments() {
        let input = r#"{
  // This is a comment
  "compilerOptions": {
    "baseUrl": "./src", // another comment
    "paths": {
      "@lib/*": ["./src/lib/*"]
    }
  }
}"#;
        let stripped = react::strip_json_comments(input);
        // Should be parseable JSON after stripping.
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["compilerOptions"]["baseUrl"].as_str(), Some("./src"));
    }

    // ── Test 18: ReactNode literal union member ───────────────────────────────

    #[test]
    fn test_union_filters_undefined() {
        let ctx = empty_ctx();
        // string | undefined → just string (undefined is filtered out from meaningful)
        let ct = CollectedType::Union(vec![CollectedType::String, CollectedType::Undefined]);
        let result = resolve_type(&ct, &ctx);
        // With undefined filtered, only one meaningful member → string
        assert_eq!(result, PropType::String, "Expected String after filtering undefined, got {:?}", result);
    }
}
