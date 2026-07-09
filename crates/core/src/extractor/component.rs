//! Component detection: FC annotation, forwardRef, HOC-wrapped, ForwardRefExoticComponent.

use std::collections::BTreeMap;

use oxc_ast::ast::*;

use crate::types::ComponentMapping;

use super::{is_pascal_case, SourceDataCollector};

/// Peel any wrapping `as X` casts down to the real expression underneath.
/// Needed because component libraries commonly cast a forwardRef/HOC call to a
/// hand-rolled wrapper type instead of relying on `@types/react`'s own generics
/// (Fluent UI's own source notes this is "required due to lack of distributive
/// union to support unions on @types/react").
fn unwrap_as_expression<'a, 'b>(expr: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut expr = expr;
    while let Expression::TSAsExpression(as_expr) = expr {
        expr = &as_expr.expression;
    }
    expr
}

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

    /// Try to detect: `const Button: FC<ButtonProps> = ({ variant = 'primary' }) => ...`
    pub(super) fn try_fc_annotation<'a>(
        &mut self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        let type_ann = decl.type_annotation.as_ref()?;
        let init_params = match decl.init.as_ref() {
            Some(Expression::ArrowFunctionExpression(afe)) => Some(&*afe.params),
            Some(Expression::FunctionExpression(fe)) => Some(&*fe.params),
            _ => None,
        };
        self.extract_props_from_type_annotation(
            &type_ann.type_annotation,
            name,
            decl.span.start,
            decl.span.end,
            init_params,
        )
    }

    pub(super) fn extract_props_from_type_annotation<'a>(
        &mut self,
        ty: &TSType<'a>,
        name: &str,
        span_start: u32,
        span_end: u32,
        init_params: Option<&FormalParameters<'a>>,
    ) -> Option<ComponentMapping> {
        match ty {
            TSType::TSTypeReference(tr) => {
                let type_name = self.extract_type_ref_name(tr);
                // Strip React. prefix for matching
                let bare_name = type_name.strip_prefix("React.").unwrap_or(&type_name);
                if !matches!(
                    bare_name,
                    "FC" | "FunctionComponent"
                        | "ComponentType"
                        | "VFC"
                        | "VoidFunctionComponent"
                        | "ForwardRefComponent"
                ) {
                    return None;
                }
                let (props_name, type_args) = self.extract_props_arg(&tr.type_arguments)?;
                let (description, tags) = self.find_jsdoc_with_tags(span_start);
                Some(ComponentMapping {
                    component_name: name.to_owned(),
                    props_type_name: props_name,
                    props_type_args: type_args,
                    file_path: self.file_path.clone(),
                    description,
                    tags,
                    span_start,
                    span_end,
                    param_defaults: init_params.map(|p| self.extract_param_defaults(p)).unwrap_or_default(),
                })
            }
            TSType::TSParenthesizedType(p) => {
                self.extract_props_from_type_annotation(&p.type_annotation, name, span_start, span_end, init_params)
            }
            _ => None,
        }
    }

    /// Try to detect: `const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(...)`
    pub(super) fn try_forward_ref<'a>(
        &mut self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        let init = decl.init.as_ref()?;
        let call = match unwrap_as_expression(init) {
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

        // The render function is forwardRef's call argument — its first parameter is
        // props (the second is `ref`, which never carries destructured defaults we care about).
        let render_params = match call.arguments.first() {
            Some(Argument::FunctionExpression(fe)) => Some(&fe.params),
            Some(Argument::ArrowFunctionExpression(afe)) => Some(&afe.params),
            _ => None,
        };
        let param_defaults = render_params.map(|p| self.extract_param_defaults(p)).unwrap_or_default();
        let (description, tags) = self.find_jsdoc_with_tags(decl.span.start);

        Some(ComponentMapping {
            component_name: name.to_owned(),
            props_type_name: props_name,
            props_type_args: type_args,
            file_path: self.file_path.clone(),
            description,
            tags,
            span_start: decl.span.start,
            span_end: decl.span.end,
            param_defaults,
        })
    }

    /// Try to detect: `const Button = anyHOC(function Button(props: ButtonProps) {...})`
    pub(super) fn try_hoc_wrapped<'a>(
        &mut self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        let init = decl.init.as_ref()?;
        let call = match unwrap_as_expression(init) {
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
                        let (description, tags) = self.find_jsdoc_with_tags(decl.span.start);
                        return Some(ComponentMapping {
                            component_name: name.to_owned(),
                            props_type_name: props_name,
                            props_type_args: type_args,
                            file_path: self.file_path.clone(),
                            description,
                            tags,
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
        let (description, tags) = self.find_jsdoc_with_tags(decl.span.start);

        Some(ComponentMapping {
            component_name: name.to_owned(),
            props_type_name: props_name,
            props_type_args: type_args,
            file_path: self.file_path.clone(),
            description,
            tags,
            span_start: decl.span.start,
            span_end: decl.span.end,
            param_defaults,
        })
    }

    /// Try to detect: `declare const Button: React.ForwardRefExoticComponent<ButtonProps & RefAttributes<E>>`
    ///
    /// Common in .d.ts files — no initializer, just a type annotation.
    pub(super) fn try_forward_ref_exotic_decl<'a>(
        &mut self,
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

    /// Rename an already-collected component mapping to its real public export name
    /// when the mapping's own identifier is immediately wrapped by an *unrecognized*
    /// call and reassigned to a new binding — e.g. Headless UI's real
    /// `export let ListboxButton = forwardRefWithAs(ButtonFn) as X`. `forwardRefWithAs`
    /// is a library-defined wrapper, not `React.forwardRef` itself, so `try_forward_ref`
    /// never matches it; meanwhile `ButtonFn` — a standalone top-level function
    /// declaration — was already independently collected as its own component by the
    /// `visit_function` Pattern 4 check, under the wrong (inner, implementation-only)
    /// name. Without this, the real export name never appears at all.
    pub(super) fn try_rename_identifier_wrapped_component<'a>(&mut self, decl: &VariableDeclarator<'a>, name: &str) {
        let Some(init) = decl.init.as_ref() else { return };
        let Expression::CallExpression(call) = unwrap_as_expression(init) else { return };
        let Some(Argument::Identifier(id)) = call.arguments.first() else { return };
        let inner_name = id.name.as_str();

        for mapping in &mut self.data.component_mappings {
            if mapping.component_name == inner_name {
                mapping.component_name = name.to_owned();
                return;
            }
        }
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
