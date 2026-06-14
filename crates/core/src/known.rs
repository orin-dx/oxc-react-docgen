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
        // vanilla-extract: RecipeVariants<typeof buttonRecipe>
        // tailwind-variants: VariantProps<typeof tv(...)>
        "VariantProps" | "RecipeVariantProps" | "RecipeVariants" => {
            resolve_cva_variant_props(args, global)
        }

        // ── MUI styling ─────────────────────────────────────────────────────
        // SxProps is a massive conditional type — surface as opaque
        "SxProps" | "SystemStyleObject" | "SystemCssProperties" => {
            Some(KnownPatternResult::Type(PropType::SxProps))
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

        // ── React Aria ───────────────────────────────────────────────────────
        // RenderProps<ButtonRenderProps> → simplify to scalar className/style
        "RenderProps" => Some(KnownPatternResult::Props(render_props())),
        // SlotProps → { slot?: string | null }
        "SlotProps" => Some(KnownPatternResult::Props(vec![slot_prop()])),

        // ── Chakra / Ark / Styled System ────────────────────────────────────
        // HTMLChakraProps<'button'> → same as ComponentPropsWithoutRef<'button'>
        "HTMLChakraProps" | "HTMLArkProps" | "HTMLStyledProps" => {
            html_attrs_from_first_arg(args)
        }

        // ThemingProps is runtime-dependent on the chakra theme
        "ThemingProps" => Some(KnownPatternResult::Type(PropType::Opaque {
            raw: "ThemingProps".into(),
            reason: OpaqueReason::RuntimeDependent {
                function_name: "chakra".into(),
            },
        })),

        // ── Mantine ─────────────────────────────────────────────────────────
        // StylesApiProps is runtime-dependent on createStyles
        "StylesApiProps" => Some(KnownPatternResult::Type(PropType::Opaque {
            raw: "StylesApiProps".into(),
            reason: OpaqueReason::RuntimeDependent {
                function_name: "createStyles".into(),
            },
        })),

        // MantineColor/Size/Radius are string aliases — let resolver handle as Named
        "MantineColor" | "MantineSize" | "MantineRadius" => None,

        // ── React standard ───────────────────────────────────────────────────
        "PropsWithChildren" => props_with_children(args),
        "PropsWithRef" => props_with_ref(args),

        // ComponentPropsWithoutRef<'button'> or ComponentPropsWithoutRef<typeof X>
        "ComponentPropsWithoutRef" | "ComponentProps" => component_props(args, false),
        "ComponentPropsWithRef" => component_props(args, true),

        // ElementRef<typeof X> → opaque Ref
        "ElementRef" => Some(KnownPatternResult::Type(PropType::Ref { element: None })),

        // ── Transparent utility types ────────────────────────────────────────
        // These are handled in the resolver's Omit/Pick logic,
        // but if seen as standalone names, pass through:
        "Partial" | "Required" | "Readonly" | "NonNullable" | "Omit" | "Pick" => None,

        // ── Unknown type names — caller should continue with normal resolution
        _ => None,
    }
}

fn resolve_cva_variant_props(
    args: &[PropType],
    global: &GlobalSourceData,
) -> Option<KnownPatternResult> {
    // The arg is typeof buttonVariants — a Named type reference to a cva() call result.
    // We stored the cva() call variants in global.enums during extraction.
    // Look them up and return as individual props.
    //
    // If we can't find the variants (e.g. they're imported from elsewhere),
    // return an Opaque rather than failing.

    match args.first() {
        Some(PropType::Named { name, .. }) => {
            // Search all enum entries for a matching name.
            // The scoped key format is "${file_path}:${name}".
            // Since we don't know the file here, search by name suffix.
            let name_str = name.as_str();
            let found = global.enums.iter().find(|(key, _)| {
                key.ends_with(&format!(":{}", name_str))
                    || key.as_str() == name_str
            });

            match found {
                Some((_key, enum_entries)) => {
                    // Build one LiteralUnion prop per variant key found in enum entries.
                    // Group by variant key (the enum entry name acts as variant key).
                    // Each EnumEntry maps to a variant value; group all values for
                    // the same variant key into a LiteralUnion prop.
                    use std::collections::BTreeMap;
                    let mut variant_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

                    for entry in enum_entries {
                        let variant_value = match &entry.value {
                            EnumValue::String(s) => s.clone(),
                            EnumValue::Number(n) => n.to_string(),
                            EnumValue::Bool(b) => b.to_string(),
                        };
                        variant_map
                            .entry(entry.name.clone())
                            .or_default()
                            .push(variant_value);
                    }

                    if variant_map.is_empty() {
                        return Some(KnownPatternResult::Type(PropType::Opaque {
                            raw: format!("VariantProps<typeof {}>", name_str),
                            reason: OpaqueReason::RuntimeDependent {
                                function_name: "cva".into(),
                            },
                        }));
                    }

                    let props = variant_map
                        .into_iter()
                        .map(|(variant_key, values)| {
                            simple_prop(
                                &variant_key,
                                PropType::LiteralUnion {
                                    members: values,
                                    has_default: false,
                                },
                                false,
                                "",
                            )
                        })
                        .collect();

                    Some(KnownPatternResult::Props(props))
                }
                None => {
                    // Variants not found in global data — degrade to opaque
                    Some(KnownPatternResult::Type(PropType::Opaque {
                        raw: format!("VariantProps<typeof {}>", name_str),
                        reason: OpaqueReason::RuntimeDependent {
                            function_name: "cva".into(),
                        },
                    }))
                }
            }
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
    // React Aria RenderProps<T> → simplify to these two props
    // Omit the function overload — noise for docgen
    vec![
        simple_prop(
            "className",
            PropType::String,
            false,
            "CSS class name. Accepts a function receiving render state.",
        ),
        simple_prop(
            "style",
            PropType::CssProperties,
            false,
            "Inline styles. Accepts a function receiving render state.",
        ),
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
    args.first().map(|inner| KnownPatternResult::Alias {
        name: inner.raw_string(),
        args: vec![],
    })
}

fn props_with_ref(args: &[PropType]) -> Option<KnownPatternResult> {
    args.first().map(|inner| KnownPatternResult::Alias {
        name: inner.raw_string(),
        args: vec![],
    })
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
            Some(KnownPatternResult::Alias {
                name: name.to_string(),
                args: vec![],
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sx_props_is_opaque() {
        let result = resolve_known("SxProps", &[], &GlobalSourceData::default());
        assert!(matches!(result, Some(KnownPatternResult::Type(PropType::SxProps))));
    }

    #[test]
    fn test_component_props_without_ref_string_literal() {
        let args = vec![PropType::StringLiteral("button".into())];
        let result =
            resolve_known("ComponentPropsWithoutRef", &args, &GlobalSourceData::default());
        assert!(
            matches!(result, Some(KnownPatternResult::Type(PropType::HtmlAttributes { ref element, .. })) if element == "button")
        );
    }

    #[test]
    fn test_html_chakra_props() {
        let args = vec![PropType::StringLiteral("input".into())];
        let result = resolve_known("HTMLChakraProps", &args, &GlobalSourceData::default());
        assert!(
            matches!(result, Some(KnownPatternResult::Type(PropType::HtmlAttributes { ref element, .. })) if element == "input")
        );
    }

    #[test]
    fn test_props_with_children_aliases() {
        let args = vec![PropType::Named {
            name: "ButtonProps".into(),
            args: vec![],
        }];
        let result = resolve_known("PropsWithChildren", &args, &GlobalSourceData::default());
        assert!(
            matches!(result, Some(KnownPatternResult::Alias { ref name, .. }) if name == "ButtonProps")
        );
    }

    #[test]
    fn test_partial_returns_none() {
        let result = resolve_known("Partial", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_render_props_returns_two_props() {
        let result = resolve_known("RenderProps", &[], &GlobalSourceData::default());
        let Some(KnownPatternResult::Props(props)) = result else {
            panic!("expected Props result")
        };
        assert!(props.iter().any(|p| p.name == "className"));
        assert!(props.iter().any(|p| p.name == "style"));
    }

    #[test]
    fn test_system_style_object_is_sx_props() {
        let result = resolve_known("SystemStyleObject", &[], &GlobalSourceData::default());
        assert!(matches!(result, Some(KnownPatternResult::Type(PropType::SxProps))));
    }

    #[test]
    fn test_system_css_properties_is_sx_props() {
        let result = resolve_known("SystemCssProperties", &[], &GlobalSourceData::default());
        assert!(matches!(result, Some(KnownPatternResult::Type(PropType::SxProps))));
    }

    #[test]
    fn test_overridable_string_union_with_base_arg() {
        let args = vec![PropType::Union(vec![
            PropType::StringLiteral("contained".into()),
            PropType::StringLiteral("outlined".into()),
        ])];
        let result = resolve_known("OverridableStringUnion", &args, &GlobalSourceData::default());
        assert!(matches!(result, Some(KnownPatternResult::Type(PropType::Union(_)))));
    }

    #[test]
    fn test_overridable_string_union_no_args() {
        let result = resolve_known("OverridableStringUnion", &[], &GlobalSourceData::default());
        assert!(matches!(
            result,
            Some(KnownPatternResult::Type(PropType::Opaque {
                reason: OpaqueReason::ModuleAugmentation,
                ..
            }))
        ));
    }

    #[test]
    fn test_slot_props_returns_slot_prop() {
        let result = resolve_known("SlotProps", &[], &GlobalSourceData::default());
        let Some(KnownPatternResult::Props(props)) = result else {
            panic!("expected Props result")
        };
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, "slot");
        assert!(!props[0].required);
    }

    #[test]
    fn test_html_ark_props() {
        let args = vec![PropType::StringLiteral("div".into())];
        let result = resolve_known("HTMLArkProps", &args, &GlobalSourceData::default());
        assert!(
            matches!(result, Some(KnownPatternResult::Type(PropType::HtmlAttributes { ref element, .. })) if element == "div")
        );
    }

    #[test]
    fn test_html_styled_props() {
        let args = vec![PropType::StringLiteral("span".into())];
        let result = resolve_known("HTMLStyledProps", &args, &GlobalSourceData::default());
        assert!(
            matches!(result, Some(KnownPatternResult::Type(PropType::HtmlAttributes { ref element, .. })) if element == "span")
        );
    }

    #[test]
    fn test_theming_props_is_runtime_dependent() {
        let result = resolve_known("ThemingProps", &[], &GlobalSourceData::default());
        assert!(matches!(
            result,
            Some(KnownPatternResult::Type(PropType::Opaque {
                reason: OpaqueReason::RuntimeDependent { .. },
                ..
            }))
        ));
    }

    #[test]
    fn test_styles_api_props_is_runtime_dependent() {
        let result = resolve_known("StylesApiProps", &[], &GlobalSourceData::default());
        assert!(matches!(
            result,
            Some(KnownPatternResult::Type(PropType::Opaque {
                reason: OpaqueReason::RuntimeDependent { .. },
                ..
            }))
        ));
    }

    #[test]
    fn test_mantine_color_returns_none() {
        let result = resolve_known("MantineColor", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_mantine_size_returns_none() {
        let result = resolve_known("MantineSize", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_mantine_radius_returns_none() {
        let result = resolve_known("MantineRadius", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_props_with_ref_aliases() {
        let args = vec![PropType::Named {
            name: "InputProps".into(),
            args: vec![],
        }];
        let result = resolve_known("PropsWithRef", &args, &GlobalSourceData::default());
        assert!(
            matches!(result, Some(KnownPatternResult::Alias { ref name, .. }) if name == "InputProps")
        );
    }

    #[test]
    fn test_component_props_without_ref_named_type() {
        let args = vec![PropType::Named {
            name: "MyComponent".into(),
            args: vec![],
        }];
        let result =
            resolve_known("ComponentPropsWithoutRef", &args, &GlobalSourceData::default());
        assert!(
            matches!(result, Some(KnownPatternResult::Alias { ref name, .. }) if name == "MyComponent")
        );
    }

    #[test]
    fn test_component_props_with_ref() {
        let args = vec![PropType::StringLiteral("a".into())];
        let result = resolve_known("ComponentPropsWithRef", &args, &GlobalSourceData::default());
        assert!(
            matches!(result, Some(KnownPatternResult::Type(PropType::HtmlAttributes { ref element, .. })) if element == "a")
        );
    }

    #[test]
    fn test_element_ref_returns_ref() {
        let result = resolve_known("ElementRef", &[], &GlobalSourceData::default());
        assert!(matches!(
            result,
            Some(KnownPatternResult::Type(PropType::Ref { element: None }))
        ));
    }

    #[test]
    fn test_required_returns_none() {
        let result = resolve_known("Required", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_readonly_returns_none() {
        let result = resolve_known("Readonly", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_non_nullable_returns_none() {
        let result = resolve_known("NonNullable", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_omit_returns_none() {
        let result = resolve_known("Omit", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_pick_returns_none() {
        let result = resolve_known("Pick", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_unknown_type_returns_none() {
        let result = resolve_known("SomeRandomType", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_recipe_variant_props_no_args_is_opaque() {
        let result = resolve_known("RecipeVariantProps", &[], &GlobalSourceData::default());
        assert!(matches!(
            result,
            Some(KnownPatternResult::Type(PropType::Opaque {
                reason: OpaqueReason::RuntimeDependent { .. },
                ..
            }))
        ));
    }

    #[test]
    fn test_variant_props_named_not_in_global_is_opaque() {
        let args = vec![PropType::Named {
            name: "buttonVariants".into(),
            args: vec![],
        }];
        let result = resolve_known("VariantProps", &args, &GlobalSourceData::default());
        assert!(matches!(
            result,
            Some(KnownPatternResult::Type(PropType::Opaque {
                reason: OpaqueReason::RuntimeDependent { .. },
                ..
            }))
        ));
    }

    #[test]
    fn test_variant_props_found_in_global_returns_props() {
        use rustc_hash::FxHashMap;

        let mut enums: FxHashMap<String, Vec<EnumEntry>> = FxHashMap::default();
        enums.insert(
            "/src/button.ts:buttonVariants".to_string(),
            vec![
                EnumEntry {
                    name: "variant".into(),
                    value: EnumValue::String("default".into()),
                    description: String::new(),
                },
                EnumEntry {
                    name: "variant".into(),
                    value: EnumValue::String("destructive".into()),
                    description: String::new(),
                },
                EnumEntry {
                    name: "size".into(),
                    value: EnumValue::String("sm".into()),
                    description: String::new(),
                },
            ],
        );

        let global = GlobalSourceData {
            enums,
            ..Default::default()
        };

        let args = vec![PropType::Named {
            name: "buttonVariants".into(),
            args: vec![],
        }];
        let result = resolve_known("VariantProps", &args, &global);
        let Some(KnownPatternResult::Props(props)) = result else {
            panic!("expected Props result, got something else")
        };
        // Should have two variant keys: "variant" and "size"
        assert_eq!(props.len(), 2);
        assert!(props.iter().any(|p| p.name == "variant"));
        assert!(props.iter().any(|p| p.name == "size"));
    }

    #[test]
    fn test_component_props_no_args_returns_none() {
        let result = resolve_known("ComponentProps", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_html_chakra_props_no_args_returns_none() {
        let result = resolve_known("HTMLChakraProps", &[], &GlobalSourceData::default());
        assert!(result.is_none());
    }
}
