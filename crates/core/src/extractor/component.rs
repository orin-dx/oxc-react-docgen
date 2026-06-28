//! Component detection: FC annotation, forwardRef, HOC-wrapped, ForwardRefExoticComponent.

use std::collections::BTreeMap;

use oxc_ast::ast::*;

use crate::types::ComponentMapping;

use super::{is_pascal_case, SourceDataCollector};

impl<'src> SourceDataCollector<'src> {
    // ─── Component detection helpers ──────────────────────────────────────────

    /// Try to extract a PascalCase name from a VariableDeclarator's binding.
    pub(super) fn extract_pascal_name<'a>(&self, decl: &VariableDeclarator<'a>) -> Option<String> {
        match &decl.id {
            BindingPattern::BindingIdentifier(id) => {
                let name = id.name.as_str();
                if is_pascal_case(name) {
                    Some(name.to_owned())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Try to detect: `const Button: FC<ButtonProps> = ...`
    pub(super) fn try_fc_annotation<'a>(&self, decl: &VariableDeclarator<'a>, name: &str) -> Option<ComponentMapping> {
        let type_ann = decl.type_annotation.as_ref()?;
        self.extract_props_from_type_annotation(&type_ann.type_annotation, name, decl.span.start, decl.span.end)
    }

    pub(super) fn extract_props_from_type_annotation<'a>(
        &self,
        ty: &TSType<'a>,
        name: &str,
        span_start: u32,
        span_end: u32,
    ) -> Option<ComponentMapping> {
        match ty {
            TSType::TSTypeReference(tr) => {
                let type_name = self.extract_type_ref_name(tr);
                // Strip React. prefix for matching
                let bare_name = type_name.strip_prefix("React.").unwrap_or(&type_name);
                if !matches!(bare_name, "FC" | "FunctionComponent" | "ComponentType" | "VFC" | "VoidFunctionComponent")
                {
                    return None;
                }
                let (props_name, type_args) = self.extract_props_arg(&tr.type_arguments)?;
                Some(ComponentMapping {
                    component_name: name.to_owned(),
                    props_type_name: props_name,
                    props_type_args: type_args,
                    file_path: self.file_path.clone(),
                    description: self.find_jsdoc(span_start),
                    tags: self.extract_jsdoc_tags(span_start),
                    span_start,
                    span_end,
                    param_defaults: Default::default(),
                })
            }
            TSType::TSParenthesizedType(p) => {
                self.extract_props_from_type_annotation(&p.type_annotation, name, span_start, span_end)
            }
            _ => None,
        }
    }

    /// Try to detect: `const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(...)`
    pub(super) fn try_forward_ref<'a>(&self, decl: &VariableDeclarator<'a>, name: &str) -> Option<ComponentMapping> {
        let init = decl.init.as_ref()?;
        let call = match init {
            Expression::CallExpression(ce) => ce,
            _ => return None,
        };
        let callee_name = self.extract_callee_name(call)?;

        if !matches!(callee_name.as_str(), "forwardRef" | "React.forwardRef") {
            return None;
        }

        // forwardRef<RefType, PropsType>(fn) — PropsType is the second type parameter
        let type_params = call.type_arguments.as_ref()?;
        if type_params.params.len() < 2 {
            return None;
        }
        let props_type = &type_params.params[1];
        let (props_name, type_args) = self.extract_type_name_from_type(props_type)?;

        Some(ComponentMapping {
            component_name: name.to_owned(),
            props_type_name: props_name,
            props_type_args: type_args,
            file_path: self.file_path.clone(),
            description: self.find_jsdoc(decl.span.start),
            tags: self.extract_jsdoc_tags(decl.span.start),
            span_start: decl.span.start,
            span_end: decl.span.end,
            param_defaults: Default::default(),
        })
    }

    /// Try to detect: `const Button = anyHOC(function Button(props: ButtonProps) {...})`
    pub(super) fn try_hoc_wrapped<'a>(&self, decl: &VariableDeclarator<'a>, name: &str) -> Option<ComponentMapping> {
        let init = decl.init.as_ref()?;
        let call = match init {
            Expression::CallExpression(ce) => ce,
            _ => return None,
        };

        // First arg should be a function with a typed props param — or a forwardRef() call.
        let first_arg = call.arguments.first()?;

        // Handle memo(forwardRef<Ref, Props>(...)) — inner call is forwardRef
        if let Argument::CallExpression(inner) = first_arg {
            if let Some(inner_name) = self.extract_callee_name(inner) {
                if matches!(inner_name.as_str(), "forwardRef" | "React.forwardRef") {
                    let type_params = inner.type_arguments.as_ref()?;
                    if type_params.params.len() >= 2 {
                        let (props_name, type_args) = self.extract_type_name_from_type(&type_params.params[1])?;
                        return Some(ComponentMapping {
                            component_name: name.to_owned(),
                            props_type_name: props_name,
                            props_type_args: type_args,
                            file_path: self.file_path.clone(),
                            description: self.find_jsdoc(decl.span.start),
                            tags: self.extract_jsdoc_tags(decl.span.start),
                            span_start: decl.span.start,
                            span_end: decl.span.end,
                            param_defaults: Default::default(),
                        });
                    }
                }
            }
            return None;
        }

        let (fn_name, params) = match first_arg {
            Argument::FunctionExpression(fe) => (fe.id.as_ref().map(|id| id.name.as_str()), &fe.params),
            Argument::ArrowFunctionExpression(afe) => (None, &afe.params),
            _ => return None,
        };

        // Must be PascalCase — not anonymous utility functions
        if let Some(fn_name_str) = fn_name {
            if !is_pascal_case(fn_name_str) {
                return None;
            }
        }

        // Extract props type from first parameter annotation
        let first_param = params.items.first()?;
        let type_ann = first_param.type_annotation.as_ref()?;
        let (props_name, type_args) = self.extract_type_name_from_type(&type_ann.type_annotation)?;

        let param_defaults = self.extract_param_defaults(params);

        Some(ComponentMapping {
            component_name: name.to_owned(),
            props_type_name: props_name,
            props_type_args: type_args,
            file_path: self.file_path.clone(),
            description: self.find_jsdoc(decl.span.start),
            tags: self.extract_jsdoc_tags(decl.span.start),
            span_start: decl.span.start,
            span_end: decl.span.end,
            param_defaults,
        })
    }

    /// Try to detect: `declare const Button: React.ForwardRefExoticComponent<ButtonProps & RefAttributes<E>>`
    ///
    /// Common in .d.ts files — no initializer, just a type annotation.
    pub(super) fn try_forward_ref_exotic_decl<'a>(
        &self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        let type_ann = decl.type_annotation.as_ref()?;
        let ct = self.ts_type_to_collected(&type_ann.type_annotation);

        // Look for ForwardRefExoticComponent<P & RefAttributes<E>>
        // or ForwardRefExoticComponent<P>
        let (type_name, args) = match &ct {
            crate::types::CollectedType::Named { name, args } => (name.as_str(), args.as_slice()),
            _ => return None,
        };

        if !matches!(type_name, "ForwardRefExoticComponent" | "React.ForwardRefExoticComponent") {
            return None;
        }

        let first_arg = args.first()?;

        // Extract P from P & RefAttributes<E>
        let props_type = match first_arg {
            crate::types::CollectedType::Intersection(members) => {
                // Find the member that is NOT RefAttributes/RefAttributes<E>
                members
                    .iter()
                    .find(|m| {
                        !matches!(m, crate::types::CollectedType::Named { name, .. }
                            if matches!(name.as_str(), "RefAttributes" | "React.RefAttributes"))
                    })
                    .unwrap_or(first_arg)
            }
            other => other,
        };

        let (props_name, props_args) = match props_type {
            crate::types::CollectedType::Named { name, args } => (name.clone(), args.clone()),
            _ => return None,
        };

        // Convert args to strings for ComponentMapping (resolver will re-parse)
        let props_type_args: Vec<String> = props_args.iter().map(|a| a.to_raw_string()).collect();

        Some(ComponentMapping {
            component_name: name.to_owned(),
            props_type_name: props_name,
            props_type_args,
            file_path: self.file_path.clone(),
            description: self.find_jsdoc(decl.span.start),
            tags: BTreeMap::new(),
            span_start: decl.span.start,
            span_end: decl.span.end,
            param_defaults: rustc_hash::FxHashMap::default(),
        })
    }

    /// Extract the callee name of a call expression (simple ident or member expr).
    pub(super) fn extract_callee_name<'a>(&self, call: &CallExpression<'a>) -> Option<String> {
        match &call.callee {
            Expression::Identifier(id) => Some(id.name.as_str().to_owned()),
            Expression::StaticMemberExpression(me) => {
                Some(format!("{}.{}", self.expression_to_ident_name(&me.object), me.property.name.as_str()))
            }
            _ => None,
        }
    }
}
