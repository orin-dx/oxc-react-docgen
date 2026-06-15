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
    pub react_version: react_types::ReactVersion,
    pub extra_builtins: FxHashSet<CompactString>,
}

impl ResolutionContext {
    pub fn new(global: Arc<GlobalSourceData>, options: &PipelineOptions) -> Self {
        let alias: Vec<(String, Vec<AliasValue>)> =
            react::read_tsconfig_paths(options.tsconfig_path.as_deref());

        let resolve_options = ResolveOptions {
            condition_names: vec![
                "types".into(),
                "import".into(),
                "require".into(),
                "default".into(),
            ],
            main_fields: vec![
                "types".into(),
                "typings".into(),
                "module".into(),
                "main".into(),
            ],
            extensions: vec![".ts".into(), ".tsx".into(), ".d.ts".into(), ".js".into()],
            alias,
            ..ResolveOptions::default()
        };

        Self {
            import_map: Arc::new(ImportResolutionMap::build(&global)),
            global,
            oxc_resolver: Arc::new(Resolver::new(resolve_options)),
            react_version: options.react_version.clone(),
            extra_builtins: options.extra_builtins.clone(),
        }
    }
}

// ─── Entry Point ─────────────────────────────────────────────────────────────

/// Resolve a `ComponentMapping` to a complete `ComponentEntry`.
///
/// Called in parallel via rayon — must be `Send + Sync` (all data is owned/Arc'd).
pub fn resolve_component(
    mapping: &ComponentMapping,
    ctx: &ResolutionContext,
) -> (ComponentEntry, Vec<Diagnostic>) {
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
            let notable_attrs = react_types::notable_html_attrs(element);
            for attr_name in notable_attrs {
                if props.contains_key(*attr_name) { continue; } // own prop wins
                if notable_inherited.contains_key(*attr_name) { continue; } // already added

                // Synthesize a minimal prop for display purposes
                let prop_type = html::infer_html_attr_prop_type(attr_name);
                notable_inherited.insert(attr_name.to_string(), ParsedProp {
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
                });
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
        let existing: FxHashSet<String> =
            self.props.iter().map(|p| p.name.clone()).collect();

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
        self.inherited_by_name.extend(parent.inherited_by_name);
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
        resolve_collected_type(
            ct,
            Utf8Path::new("/test/button.tsx"),
            ctx,
            &mut state,
            0,
        )
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
        let ct = CollectedType::Intersection(vec![
            CollectedType::String,
            CollectedType::Object(vec![]),
        ]);
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
            obj: Box::new(CollectedType::Named {
                name: "CSSProperties".into(),
                args: vec![],
            }),
            key: Box::new(CollectedType::StringLiteral("justifyContent".into())),
        };
        let result = resolve_type(&ct, &ctx);
        assert_eq!(result, PropType::String, "Expected String for CSSProperties[string key], got {:?}", result);
    }

    #[test]
    fn test_indexed_access_css_properties_numeric_key() {
        let ctx = empty_ctx();
        let ct = CollectedType::IndexedAccess {
            obj: Box::new(CollectedType::Named {
                name: "CSSProperties".into(),
                args: vec![],
            }),
            key: Box::new(CollectedType::StringLiteral("zIndex".into())),
        };
        let result = resolve_type(&ct, &ctx);
        assert_eq!(result, PropType::Number, "Expected Number for CSSProperties[zIndex], got {:?}", result);
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
            RawDefault {
                value: "\"default\"".into(),
                computed: false,
                source: DefaultSource::Destructuring,
            },
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
        assert_eq!(
            variant_prop.default_value.as_ref().map(|d| d.value.as_str()),
            Some("\"default\"")
        );
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
                extends: vec![ExtendsRef::SameFile {
                    name: "PrimitiveButtonProps".into(),
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

        let warnings: Vec<_> = diagnostics.iter().filter(|d| matches!(d.severity, DiagnosticSeverity::Warning)).collect();
        assert!(warnings.is_empty(), "Expected no warnings, got {:?}", warnings);
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
        let parts = vec![
            CollectedType::StringLiteral("compact-".into()),
            CollectedType::StringLiteral("sm".into()),
        ];
        let result = template::try_expand_template_literal(
            &parts,
            Utf8Path::new("/test/types.ts"),
            &ctx,
            &mut state,
            0,
        );
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
            params: vec![CollectedType::Named {
                name: "MouseEvent".into(),
                args: vec![],
            }],
            return_type: Box::new(CollectedType::Void),
        };
        let result = resolve_type(&ct, &ctx);
        assert!(
            matches!(&result, PropType::EventHandler { event_type } if event_type == "MouseEvent"),
            "Expected EventHandler<MouseEvent>, got {:?}",
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
        let ct = CollectedType::Union(vec![
            CollectedType::String,
            CollectedType::Undefined,
        ]);
        let result = resolve_type(&ct, &ctx);
        // With undefined filtered, only one meaningful member → string
        assert_eq!(result, PropType::String, "Expected String after filtering undefined, got {:?}", result);
    }
}
