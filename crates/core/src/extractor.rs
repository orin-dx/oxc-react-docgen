//! OXC AST extractor — Phase 2a.
//!
//! Walks the OXC AST for a single file and collects everything into a [`SourceData`].
//!
//! **The Absolute Rule:** No AST references escape `parse_file`.
//! The `Allocator` is created locally, and all arena memory is freed when `parse_file` returns.
//! [`SourceData`] contains only owned data (String, Vec, etc.).

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use oxc_allocator::{Allocator, Box as OxcBox};
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::scope::ScopeFlags;
use rustc_hash::FxHashSet;

use crate::types::{
    CollectedInterface, CollectedTypeAlias, ComponentMapping, EnumEntry, EnumValue, ExtendsRef,
    ImportBinding, LexedExport, RawProp, SourceData,
};

// ─── Entry Point ─────────────────────────────────────────────────────────────

/// Parse a single file and collect all extractable data.
///
/// Completely pure — no I/O, no side effects, no cross-file dependencies.
/// Safe to call in parallel from rayon workers.
pub fn parse_file(path: &Utf8Path, source: &str) -> SourceData {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_default();
    let ret = Parser::new(&allocator, source, source_type).parse();

    let is_tsx = source_type.is_jsx();
    let mut collector = SourceDataCollector::new(path, source, is_tsx);
    // Pass comments by cloning them into owned strings before the allocator drops.
    // The comments Vec lives in the arena; we extract them here.
    let comments: Vec<OwnedComment> = ret
        .program
        .comments
        .iter()
        .map(|c| OwnedComment {
            span_start: c.span.start,
            span_end: c.span.end,
            is_block: c.is_block(),
        })
        .collect();
    collector.comments = comments;
    collector.visit_program(&ret.program);
    collector.finish()
    // allocator drops here — ALL arena memory freed
}

// ─── Owned Comment (so nothing escapes the allocator) ────────────────────────

#[derive(Debug, Clone)]
struct OwnedComment {
    span_start: u32,
    span_end: u32,
    is_block: bool,
}

// ─── Collector ───────────────────────────────────────────────────────────────

struct SourceDataCollector<'src> {
    file_path: Utf8PathBuf,
    source: &'src str,
    is_tsx: bool,
    data: SourceData,
    /// All comments in the file (owned — not tied to allocator lifetime).
    comments: Vec<OwnedComment>,
    /// Names that came from imports — used to classify ExtendsRef.
    imported_names: FxHashSet<CompactString>,
}

impl<'src> SourceDataCollector<'src> {
    fn new(path: &Utf8Path, source: &'src str, is_tsx: bool) -> Self {
        Self {
            file_path: path.to_owned(),
            source,
            is_tsx,
            data: SourceData::default(),
            comments: Vec::new(),
            imported_names: FxHashSet::default(),
        }
    }

    fn scoped_key(&self, name: &str) -> String {
        format!("{}:{}", self.file_path, name)
    }

    fn finish(self) -> SourceData {
        self.data
    }

    // ─── Import source specifier lookup ──────────────────────────────────────

    fn find_import_specifier(&self, local_name: &str) -> Option<String> {
        self.data
            .imports
            .iter()
            .find(|imp| imp.local_name.as_str() == local_name)
            .map(|imp| imp.specifier.clone())
    }

    // ─── ExtendsRef classification ────────────────────────────────────────────

    fn classify_extends(&self, name: &str, type_args: Vec<String>) -> ExtendsRef {
        // Strip "React." prefix for lookup in builtin tables
        let lookup_name = name.strip_prefix("React.").unwrap_or(name);

        if let Some(element) = crate::react_types::html_element_for(lookup_name) {
            return ExtendsRef::Builtin {
                name: name.into(),
                element: Some(element.to_owned()),
                type_args,
            };
        }
        if crate::react_types::is_react_builtin(lookup_name) {
            return ExtendsRef::Builtin { name: name.into(), element: None, type_args };
        }
        // Check if imported
        if self.imported_names.contains(name) {
            return ExtendsRef::Imported {
                local_name: name.into(),
                type_args,
                source_specifier: self.find_import_specifier(name),
            };
        }
        ExtendsRef::SameFile { name: name.into(), type_args }
    }

    // ─── TSTypeName → String ──────────────────────────────────────────────────

    fn ts_type_name_str<'a>(&self, name: &TSTypeName<'a>) -> String {
        match name {
            TSTypeName::IdentifierReference(id) => id.name.as_str().to_owned(),
            TSTypeName::QualifiedName(q) => {
                format!("{}.{}", self.ts_type_name_str(&q.left), q.right.name.as_str())
            }
            TSTypeName::ThisExpression(_) => "this".to_owned(),
        }
    }

    // ─── TSTypeParameterInstantiation → Vec<String> ──────────────────────────

    fn extract_type_args<'a>(
        &self,
        type_params: &Option<OxcBox<'a, TSTypeParameterInstantiation<'a>>>,
    ) -> Vec<String> {
        match type_params {
            Some(tp) => tp.params.iter().map(|p| self.ts_type_to_string(p)).collect(),
            None => vec![],
        }
    }

    // ─── TSType → raw string representation ──────────────────────────────────

    fn ts_type_to_string<'a>(&self, ty: &TSType<'a>) -> String {
        match ty {
            TSType::TSStringKeyword(_) => "string".to_owned(),
            TSType::TSNumberKeyword(_) => "number".to_owned(),
            TSType::TSBooleanKeyword(_) => "boolean".to_owned(),
            TSType::TSNullKeyword(_) => "null".to_owned(),
            TSType::TSUndefinedKeyword(_) => "undefined".to_owned(),
            TSType::TSAnyKeyword(_) => "any".to_owned(),
            TSType::TSNeverKeyword(_) => "never".to_owned(),
            TSType::TSUnknownKeyword(_) => "unknown".to_owned(),
            TSType::TSVoidKeyword(_) => "void".to_owned(),
            TSType::TSObjectKeyword(_) => "object".to_owned(),
            TSType::TSBigIntKeyword(_) => "bigint".to_owned(),
            TSType::TSSymbolKeyword(_) => "symbol".to_owned(),
            TSType::TSTypeReference(tr) => {
                let name = self.ts_type_name_str(&tr.type_name);
                let args = self.extract_type_args(&tr.type_arguments);
                if args.is_empty() {
                    name
                } else {
                    format!("{}<{}>", name, args.join(", "))
                }
            }
            TSType::TSUnionType(u) => {
                u.types.iter().map(|t| self.ts_type_to_string(t)).collect::<Vec<_>>().join(" | ")
            }
            TSType::TSIntersectionType(i) => {
                i.types.iter().map(|t| self.ts_type_to_string(t)).collect::<Vec<_>>().join(" & ")
            }
            TSType::TSArrayType(a) => format!("{}[]", self.ts_type_to_string(&a.element_type)),
            TSType::TSLiteralType(lit) => match &lit.literal {
                TSLiteral::StringLiteral(s) => format!("\"{}\"", s.value.as_str()),
                TSLiteral::NumericLiteral(n) => n.value.to_string(),
                TSLiteral::BooleanLiteral(b) => b.value.to_string(),
                TSLiteral::UnaryExpression(u) => {
                    format!("{}{}", u.operator.as_str(), self.expression_to_string(&u.argument))
                }
                _ => "literal".to_owned(),
            },
            TSType::TSFunctionType(_) => "(...args: any[]) => any".to_owned(),
            TSType::TSTupleType(_) => "any[]".to_owned(),
            TSType::TSTypeLiteral(tl) => {
                // Inline object type: { key: Type }
                let members: Vec<String> = tl
                    .members
                    .iter()
                    .filter_map(|sig| match sig {
                        TSSignature::TSPropertySignature(ps) => {
                            let key = ps.key.static_name()?;
                            let ty_str = ps
                                .type_annotation
                                .as_ref()
                                .map(|ta| self.ts_type_to_string(&ta.type_annotation))
                                .unwrap_or_else(|| "any".to_owned());
                            Some(format!(
                                "{}{}: {}",
                                key,
                                if ps.optional { "?" } else { "" },
                                ty_str
                            ))
                        }
                        _ => None,
                    })
                    .collect();
                format!("{{ {} }}", members.join("; "))
            }
            TSType::TSParenthesizedType(p) => {
                format!("({})", self.ts_type_to_string(&p.type_annotation))
            }
            TSType::TSThisType(_) => "this".to_owned(),
            TSType::TSTypeOperatorType(op) => {
                format!("{} {}", op.operator.to_str(), self.ts_type_to_string(&op.type_annotation))
            }
            _ => "unknown".to_owned(),
        }
    }

    fn expression_to_string<'a>(&self, expr: &Expression<'a>) -> String {
        match expr {
            Expression::NumericLiteral(n) => n.value.to_string(),
            Expression::StringLiteral(s) => format!("\"{}\"", s.value.as_str()),
            Expression::BooleanLiteral(b) => b.value.to_string(),
            Expression::NullLiteral(_) => "null".to_owned(),
            _ => "unknown".to_owned(),
        }
    }

    // ─── TSTypeReference helpers ──────────────────────────────────────────────

    /// Extract the name part of a TSTypeReference (for checking FC, forwardRef, etc.)
    fn extract_type_ref_name<'a>(&self, tr: &TSTypeReference<'a>) -> String {
        self.ts_type_name_str(&tr.type_name)
    }

    /// Extract the first type argument name from a type ref's type params.
    ///
    /// Used for `FC<ButtonProps>` → "ButtonProps".
    /// Handles `PropsWithChildren<P>` and `Readonly<P>` wrappers.
    fn extract_props_arg<'a>(
        &self,
        type_params: &Option<OxcBox<'a, TSTypeParameterInstantiation<'a>>>,
    ) -> Option<(CompactString, Vec<String>)> {
        let tp = type_params.as_ref()?;
        let first = tp.params.first()?;
        self.extract_type_name_from_type(first)
    }

    /// Get the (name, type_args) of a TSType if it's a simple named reference.
    ///
    /// Unwraps single-layer wrappers like `PropsWithChildren<P>` and `Readonly<P>`.
    fn extract_type_name_from_type<'a>(
        &self,
        ty: &TSType<'a>,
    ) -> Option<(CompactString, Vec<String>)> {
        match ty {
            TSType::TSTypeReference(tr) => {
                let name = self.extract_type_ref_name(tr);
                // Unwrap transparent wrappers
                if matches!(name.as_str(), "PropsWithChildren" | "Readonly" | "Required") {
                    if let Some(tp) = &tr.type_arguments {
                        if let Some(inner) = tp.params.first() {
                            return self.extract_type_name_from_type(inner);
                        }
                    }
                }
                let args = self.extract_type_args(&tr.type_arguments);
                Some((name.into(), args))
            }
            TSType::TSParenthesizedType(p) => {
                self.extract_type_name_from_type(&p.type_annotation)
            }
            _ => None,
        }
    }

    // ─── TSInterfaceHeritage → ExtendsRef ────────────────────────────────────

    fn collect_extends<'a>(&self, ext: &TSInterfaceHeritage<'a>) -> ExtendsRef {
        let name = self.expression_to_ident_name(&ext.expression);
        let type_args = self.extract_type_args(&ext.type_arguments);
        self.classify_extends(&name, type_args)
    }

    fn expression_to_ident_name<'a>(&self, expr: &Expression<'a>) -> String {
        match expr {
            Expression::Identifier(id) => id.name.as_str().to_owned(),
            Expression::StaticMemberExpression(me) => {
                format!(
                    "{}.{}",
                    self.expression_to_ident_name(&me.object),
                    me.property.name.as_str()
                )
            }
            _ => "unknown".to_owned(),
        }
    }

    // ─── Property Signature collection ───────────────────────────────────────

    fn collect_property_signature<'a>(&self, sig: &TSSignature<'a>) -> Option<RawProp> {
        match sig {
            TSSignature::TSPropertySignature(ps) => {
                let name = ps.key.static_name()?.to_string();
                let raw_type = ps
                    .type_annotation
                    .as_ref()
                    .map(|ta| self.ts_type_to_string(&ta.type_annotation))
                    .unwrap_or_else(|| "any".to_owned());

                let description = self.find_jsdoc(ps.span.start);
                let tags = self.extract_jsdoc_tags(ps.span.start);

                Some(RawProp {
                    name,
                    raw_type,
                    required: !ps.optional,
                    description,
                    tags,
                    span_start: ps.span.start,
                    span_end: ps.span.end,
                })
            }
            TSSignature::TSMethodSignature(ms) => {
                let name = ms.key.static_name()?.to_string();
                let description = self.find_jsdoc(ms.span.start);
                let tags = self.extract_jsdoc_tags(ms.span.start);

                Some(RawProp {
                    name,
                    raw_type: "(...args: any[]) => any".to_owned(),
                    required: !ms.optional,
                    description,
                    tags,
                    span_start: ms.span.start,
                    span_end: ms.span.end,
                })
            }
            _ => None,
        }
    }

    // ─── TypeAlias classification ─────────────────────────────────────────────

    fn classify_type_alias<'a>(
        &self,
        _name: &str,
        ty: &TSType<'a>,
    ) -> Option<CollectedTypeAlias> {
        let fp = self.file_path.clone();
        match ty {
            TSType::TSTypeReference(tr) => {
                let ref_name = self.extract_type_ref_name(tr);
                match ref_name.as_str() {
                    "Omit" => {
                        let tp = tr.type_arguments.as_ref()?;
                        if tp.params.len() < 2 {
                            return None;
                        }
                        let (base_name, _) = self.extract_type_name_from_type(&tp.params[0])?;
                        let omitted_keys = self.collect_string_union_keys(&tp.params[1]);
                        Some(CollectedTypeAlias::Omit { base_name, omitted_keys, file_path: fp })
                    }
                    "Pick" => {
                        let tp = tr.type_arguments.as_ref()?;
                        if tp.params.len() < 2 {
                            return None;
                        }
                        let (base_name, _) = self.extract_type_name_from_type(&tp.params[0])?;
                        let picked_keys = self.collect_string_union_keys(&tp.params[1]);
                        Some(CollectedTypeAlias::Pick { base_name, picked_keys, file_path: fp })
                    }
                    "Partial" => {
                        let tp = tr.type_arguments.as_ref()?;
                        let (base_name, _) =
                            self.extract_type_name_from_type(tp.params.first()?)?;
                        Some(CollectedTypeAlias::Partial { base_name, file_path: fp })
                    }
                    "Required" => {
                        let tp = tr.type_arguments.as_ref()?;
                        let (base_name, _) =
                            self.extract_type_name_from_type(tp.params.first()?)?;
                        Some(CollectedTypeAlias::Required { base_name, file_path: fp })
                    }
                    _ => {
                        // Simple passthrough: `type Size = SomeOtherType`
                        let args = self.extract_type_args(&tr.type_arguments);
                        Some(CollectedTypeAlias::Passthrough {
                            target_name: ref_name.into(),
                            type_args: args,
                            file_path: fp,
                        })
                    }
                }
            }
            TSType::TSUnionType(u) => {
                // Check if all members are string/number literals → LiteralUnion
                let all_literals = u.types.iter().all(|t| {
                    matches!(
                        t,
                        TSType::TSLiteralType(_)
                            | TSType::TSUndefinedKeyword(_)
                            | TSType::TSNullKeyword(_)
                    )
                });
                let members: Vec<String> =
                    u.types.iter().map(|t| self.ts_type_to_string(t)).collect();

                if all_literals
                    && u.types.iter().all(|t| {
                        matches!(
                            t,
                            TSType::TSLiteralType(_)
                                | TSType::TSUndefinedKeyword(_)
                                | TSType::TSNullKeyword(_)
                        )
                    })
                {
                    // Check if all are string literals
                    let all_string = u.types.iter().all(|t| match t {
                        TSType::TSLiteralType(lit) => {
                            matches!(lit.literal, TSLiteral::StringLiteral(_))
                        }
                        TSType::TSUndefinedKeyword(_) | TSType::TSNullKeyword(_) => true,
                        _ => false,
                    });
                    if all_string {
                        return Some(CollectedTypeAlias::LiteralUnion { members, file_path: fp });
                    }
                }
                Some(CollectedTypeAlias::Union { members, file_path: fp })
            }
            TSType::TSIntersectionType(i) => {
                let members: Vec<String> =
                    i.types.iter().map(|t| self.ts_type_to_string(t)).collect();
                Some(CollectedTypeAlias::Intersection { members, file_path: fp })
            }
            TSType::TSParenthesizedType(p) => {
                self.classify_type_alias(_name, &p.type_annotation)
            }
            _ => None,
        }
    }

    /// Collect the string literal keys from a type like `'key1' | 'key2'`.
    fn collect_string_union_keys<'a>(&self, ty: &TSType<'a>) -> Vec<String> {
        match ty {
            TSType::TSLiteralType(lit) => match &lit.literal {
                TSLiteral::StringLiteral(s) => vec![s.value.as_str().to_owned()],
                _ => vec![],
            },
            TSType::TSUnionType(u) => {
                u.types.iter().flat_map(|t| self.collect_string_union_keys(t)).collect()
            }
            _ => vec![],
        }
    }

    // ─── Component detection helpers ──────────────────────────────────────────

    /// Try to extract a PascalCase name from a VariableDeclarator's binding.
    fn extract_pascal_name<'a>(&self, decl: &VariableDeclarator<'a>) -> Option<String> {
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
    fn try_fc_annotation<'a>(
        &self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        let type_ann = decl.type_annotation.as_ref()?;
        self.extract_props_from_type_annotation(&type_ann.type_annotation, name, decl.span.start, decl.span.end)
    }

    fn extract_props_from_type_annotation<'a>(
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
                if !matches!(
                    bare_name,
                    "FC"
                        | "FunctionComponent"
                        | "ComponentType"
                        | "VFC"
                        | "VoidFunctionComponent"
                ) {
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
                })
            }
            TSType::TSParenthesizedType(p) => {
                self.extract_props_from_type_annotation(&p.type_annotation, name, span_start, span_end)
            }
            _ => None,
        }
    }

    /// Try to detect: `const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(...)`
    fn try_forward_ref<'a>(
        &self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
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
        })
    }

    /// Try to detect: `const Button = anyHOC(function Button(props: ButtonProps) {...})`
    fn try_hoc_wrapped<'a>(
        &self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        let init = decl.init.as_ref()?;
        let call = match init {
            Expression::CallExpression(ce) => ce,
            _ => return None,
        };

        // First arg should be a function with a typed props param.
        let first_arg = call.arguments.first()?;
        let (fn_name, params) = match first_arg {
            Argument::FunctionExpression(fe) => {
                (fe.id.as_ref().map(|id| id.name.as_str()), &fe.params)
            }
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
        let (props_name, type_args) =
            self.extract_type_name_from_type(&type_ann.type_annotation)?;

        Some(ComponentMapping {
            component_name: name.to_owned(),
            props_type_name: props_name,
            props_type_args: type_args,
            file_path: self.file_path.clone(),
            description: self.find_jsdoc(decl.span.start),
            tags: self.extract_jsdoc_tags(decl.span.start),
            span_start: decl.span.start,
            span_end: decl.span.end,
        })
    }

    /// Extract the callee name of a call expression (simple ident or member expr).
    fn extract_callee_name<'a>(&self, call: &CallExpression<'a>) -> Option<String> {
        match &call.callee {
            Expression::Identifier(id) => Some(id.name.as_str().to_owned()),
            Expression::StaticMemberExpression(me) => Some(format!(
                "{}.{}",
                self.expression_to_ident_name(&me.object),
                me.property.name.as_str()
            )),
            _ => None,
        }
    }

    // ─── `const` enum (as-const objects) collection ──────────────────────────

    fn try_collect_const_enum<'a>(&mut self, decl: &VariableDeclarator<'a>) {
        // Only collect `const X = { ... } as const` patterns
        let name = match &decl.id {
            BindingPattern::BindingIdentifier(id) => id.name.as_str().to_owned(),
            _ => return,
        };

        let init = match &decl.init {
            Some(e) => e,
            None => return,
        };

        // Handle `{ ... } as const` (TSAsExpression) or `<const>{ ... }` (TSTypeAssertion)
        let obj_expr = match init {
            Expression::TSAsExpression(tsa) => &tsa.expression,
            Expression::TSTypeAssertion(ta) => &ta.expression,
            _ => return,
        };

        let obj = match obj_expr {
            Expression::ObjectExpression(o) => o,
            _ => return,
        };

        let entries: Vec<EnumEntry> = obj
            .properties
            .iter()
            .filter_map(|prop| match prop {
                ObjectPropertyKind::ObjectProperty(op) => {
                    let key = op.key.static_name()?.to_string();
                    let value = self.expression_to_enum_value(&op.value)?;
                    let desc = self.find_jsdoc(op.span.start);
                    Some(EnumEntry { name: key, value, description: desc })
                }
                _ => None,
            })
            .collect();

        if !entries.is_empty() {
            let key = self.scoped_key(&name);
            self.data.enums.insert(key, entries);
        }
    }

    fn expression_to_enum_value<'a>(&self, expr: &Expression<'a>) -> Option<EnumValue> {
        match expr {
            Expression::StringLiteral(s) => Some(EnumValue::String(s.value.as_str().to_owned())),
            Expression::NumericLiteral(n) => Some(EnumValue::Number(n.value)),
            Expression::BooleanLiteral(b) => Some(EnumValue::Bool(b.value)),
            Expression::UnaryExpression(u) if u.operator == UnaryOperator::UnaryNegation => {
                match &u.argument {
                    Expression::NumericLiteral(n) => Some(EnumValue::Number(-n.value)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    // ─── `displayName` scanning ───────────────────────────────────────────────

    /// Scan `Button.displayName = "Button"` and update the matching component mapping.
    fn try_scan_display_name<'a>(&mut self, stmt: &ExpressionStatement<'a>) {
        let assign = match &stmt.expression {
            Expression::AssignmentExpression(ae) => ae,
            _ => return,
        };

        // Left side must be `X.displayName`
        let (obj_name, prop_name) = match &assign.left {
            AssignmentTarget::StaticMemberExpression(sme) => {
                let obj = self.expression_to_ident_name(&sme.object);
                let prop = sme.property.name.as_str().to_owned();
                (obj, prop)
            }
            _ => return,
        };

        if prop_name != "displayName" {
            return;
        }

        // Right side must be a string literal
        let display_name = match &assign.right {
            Expression::StringLiteral(s) => s.value.as_str().to_owned(),
            _ => return,
        };

        // Update matching component mapping
        for mapping in &mut self.data.component_mappings {
            if mapping.component_name == obj_name {
                mapping.component_name = display_name.clone();
                return;
            }
        }
    }

    // ─── JSDoc extraction ─────────────────────────────────────────────────────

    /// Find JSDoc comment immediately preceding the given byte offset.
    /// Returns empty string if none found.
    fn find_jsdoc(&self, span_start: u32) -> String {
        const PROXIMITY_THRESHOLD: u32 = 120; // bytes — enough for blank lines + decorator

        let comment = self.comments.iter().rev().find(|c| {
            c.is_block && c.span_end <= span_start && span_start - c.span_end <= PROXIMITY_THRESHOLD
        });

        match comment {
            Some(c) => {
                let raw = &self.source[c.span_start as usize..c.span_end as usize];
                parse_jsdoc_text(raw)
            }
            None => String::new(),
        }
    }

    /// Extract JSDoc @tags from the comment preceding the given byte offset.
    fn extract_jsdoc_tags(&self, span_start: u32) -> BTreeMap<String, String> {
        const PROXIMITY_THRESHOLD: u32 = 120;

        let comment = self.comments.iter().rev().find(|c| {
            c.is_block && c.span_end <= span_start && span_start - c.span_end <= PROXIMITY_THRESHOLD
        });

        match comment {
            Some(c) => {
                let raw = &self.source[c.span_start as usize..c.span_end as usize];
                extract_jsdoc_tags(raw)
            }
            None => BTreeMap::new(),
        }
    }
}

// ─── Visit implementation ─────────────────────────────────────────────────────

impl<'a, 'src> Visit<'a> for SourceDataCollector<'src> {
    fn visit_import_declaration(&mut self, node: &ImportDeclaration<'a>) {
        let specifier = node.source.value.as_str().to_owned();
        let is_type_only = node.import_kind.is_type();

        if let Some(specifiers) = &node.specifiers {
            for spec in specifiers {
                match spec {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => {
                        let local = s.local.name.as_str();
                        let imported = s.imported.name();
                        let imported_str = imported.as_str();
                        self.imported_names.insert(local.into());
                        self.data.imports.push(ImportBinding {
                            local_name: local.into(),
                            exported_name: imported_str.into(),
                            specifier: specifier.clone(),
                            is_type_only: is_type_only || s.import_kind.is_type(),
                        });
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                        let local = s.local.name.as_str();
                        self.imported_names.insert(local.into());
                        self.data.imports.push(ImportBinding {
                            local_name: local.into(),
                            exported_name: "default".into(),
                            specifier: specifier.clone(),
                            is_type_only,
                        });
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                        let local = s.local.name.as_str();
                        self.imported_names.insert(local.into());
                        self.data.imports.push(ImportBinding {
                            local_name: local.into(),
                            exported_name: "*".into(),
                            specifier: specifier.clone(),
                            is_type_only,
                        });
                    }
                }
            }
        }

        // Don't walk children — we've handled everything
    }

    fn visit_export_named_declaration(&mut self, node: &ExportNamedDeclaration<'a>) {
        if let Some(source) = &node.source {
            // Re-exports: `export { X } from "./y"`
            let src = source.value.as_str().to_owned();
            for spec in &node.specifiers {
                self.data.exports.push(LexedExport::ReExportNamed {
                    // local_name is what we call it here; source_name is the original
                    local_name: spec.exported.name().as_str().to_owned(),
                    source_name: spec.local.name().as_str().to_owned(),
                    source_specifier: src.clone(),
                    is_type_only: node.export_kind.is_type()
                        || spec.export_kind.is_type(),
                });
            }
        } else {
            // Local exports: `export interface Foo / export type Bar / export const X`
            if let Some(decl) = &node.declaration {
                let decl_name = declaration_name(decl);
                if let Some(name) = decl_name {
                    self.data.exports.push(LexedExport::LocalDeclaration {
                        name: name.to_owned(),
                        is_type_only: node.export_kind.is_type(),
                    });
                }
            }
            // Also handle `export { Foo }` without a source (local)
            for spec in &node.specifiers {
                self.data.exports.push(LexedExport::LocalDeclaration {
                    name: spec.exported.name().as_str().to_owned(),
                    is_type_only: node.export_kind.is_type() || spec.export_kind.is_type(),
                });
            }
        }

        // Walk the declaration so sub-visitors (interface, type alias, etc.) fire
        walk::walk_export_named_declaration(self, node);
    }

    fn visit_export_all_declaration(&mut self, node: &ExportAllDeclaration<'a>) {
        let src = node.source.value.as_str().to_owned();
        if let Some(ns) = &node.exported {
            // `export * as Ns from "./y"`
            self.data.exports.push(LexedExport::ReExportNamespace {
                namespace: ns.name().as_str().to_owned(),
                source_specifier: src,
            });
        } else {
            // `export * from "./y"`
            self.data.exports.push(LexedExport::ReExportAll {
                source_specifier: src,
                is_type_only: node.export_kind.is_type(),
            });
        }
    }

    fn visit_ts_interface_declaration(&mut self, node: &TSInterfaceDeclaration<'a>) {
        let name = node.id.name.as_str();
        let key = self.scoped_key(name);

        let extends: Vec<ExtendsRef> =
            node.extends.iter().map(|ext| self.collect_extends(ext)).collect();

        let props: Vec<RawProp> =
            node.body.body.iter().filter_map(|sig| self.collect_property_signature(sig)).collect();

        let description = self.find_jsdoc(node.span.start);
        let tags = self.extract_jsdoc_tags(node.span.start);

        self.data.interfaces.insert(
            key.clone(),
            CollectedInterface {
                scoped_key: key,
                name: name.into(),
                file_path: self.file_path.clone(),
                props,
                extends,
                description,
                tags,
            },
        );

        // Don't walk children — we've extracted everything we need
    }

    fn visit_ts_type_alias_declaration(&mut self, node: &TSTypeAliasDeclaration<'a>) {
        let name = node.id.name.as_str();
        let key = self.scoped_key(name);

        if let Some(alias) = self.classify_type_alias(name, &node.type_annotation) {
            self.data.type_aliases.insert(key, alias);
        }
    }

    fn visit_ts_enum_declaration(&mut self, node: &TSEnumDeclaration<'a>) {
        let enum_name = node.id.name.as_str();
        let key = self.scoped_key(enum_name);

        let entries: Vec<EnumEntry> = node
            .body
            .members
            .iter()
            .filter_map(|member| {
                let name = match &member.id {
                    TSEnumMemberName::Identifier(id) => id.name.as_str().to_owned(),
                    TSEnumMemberName::String(s) => s.value.as_str().to_owned(),
                    TSEnumMemberName::ComputedString(s) => s.value.as_str().to_owned(),
                    TSEnumMemberName::ComputedTemplateString(_) => return None,
                };

                let value = match &member.initializer {
                    Some(init) => self.expression_to_enum_value(init).unwrap_or_else(|| {
                        EnumValue::String(name.clone())
                    }),
                    None => EnumValue::String(name.clone()),
                };

                let description = self.find_jsdoc(member.span.start);
                Some(EnumEntry { name, value, description })
            })
            .collect();

        if !entries.is_empty() {
            self.data.enums.insert(key, entries);
        }
    }

    fn visit_variable_declaration(&mut self, node: &VariableDeclaration<'a>) {
        for declarator in &node.declarations {
            self.try_collect_const_enum(declarator);
            if self.is_tsx {
                if let Some(name) = self.extract_pascal_name(declarator) {
                    if let Some(mapping) = self
                        .try_fc_annotation(declarator, &name)
                        .or_else(|| self.try_forward_ref(declarator, &name))
                        .or_else(|| self.try_hoc_wrapped(declarator, &name))
                    {
                        self.data.component_mappings.push(mapping);
                    }
                }
            }
        }
        // Walk children for nested declarations
        walk::walk_variable_declaration(self, node);
    }

    fn visit_function(&mut self, func: &Function<'a>, flags: ScopeFlags) {
        // Pattern 4: `function Button(props: ButtonProps) { ... }`
        if self.is_tsx {
            if let Some(id) = &func.id {
                let name = id.name.as_str();
                if is_pascal_case(name) && func.r#type == FunctionType::FunctionDeclaration {
                    if let Some(first_param) = func.params.items.first() {
                        if let Some(type_ann) = &first_param.type_annotation {
                            if let Some((props_name, type_args)) =
                                self.extract_type_name_from_type(&type_ann.type_annotation)
                            {
                                let description = self.find_jsdoc(func.span.start);
                                let tags = self.extract_jsdoc_tags(func.span.start);
                                self.data.component_mappings.push(ComponentMapping {
                                    component_name: name.to_owned(),
                                    props_type_name: props_name,
                                    props_type_args: type_args,
                                    file_path: self.file_path.clone(),
                                    description,
                                    tags,
                                    span_start: func.span.start,
                                    span_end: func.span.end,
                                });
                            }
                        }
                    }
                }
            }
        }
        walk::walk_function(self, func, flags);
    }

    fn visit_expression_statement(&mut self, node: &ExpressionStatement<'a>) {
        // Scan for `Button.displayName = "Button"` assignments
        self.try_scan_display_name(node);
        walk::walk_expression_statement(self, node);
    }
}

// ─── JSDoc parsing ────────────────────────────────────────────────────────────

fn parse_jsdoc_text(raw: &str) -> String {
    // Strip `/**` prefix and `*/` suffix
    let inner = raw.trim_start_matches("/**").trim_end_matches("*/");

    let desc_lines: Vec<&str> = inner
        .lines()
        .map(|l| {
            let l = l.trim();
            // Strip leading `* ` or `*`
            let l = l.strip_prefix("* ").or_else(|| l.strip_prefix('*')).unwrap_or(l);
            l
        })
        .take_while(|l| !l.starts_with('@'))
        .collect();

    desc_lines.join("\n").trim().to_owned()
}

fn extract_jsdoc_tags(raw: &str) -> BTreeMap<String, String> {
    let inner = raw.trim_start_matches("/**").trim_end_matches("*/");
    let mut tags: BTreeMap<String, String> = BTreeMap::new();
    let mut in_tags = false;

    for line in inner.lines() {
        let line = line.trim();
        let line = line.strip_prefix("* ").or_else(|| line.strip_prefix('*')).unwrap_or(line);
        let line = line.trim();

        if let Some(rest) = line.strip_prefix('@') {
            in_tags = true;
            // Parse tag: `@tagname rest`
            let (tag, value) = if let Some(sp) = rest.find(char::is_whitespace) {
                let tag = &rest[..sp];
                let value = rest[sp..].trim();
                (tag, value)
            } else {
                (rest, "")
            };

            // Special handling for @param — store as `param:propName`
            if tag == "param" {
                // `@param propName description` or `@param {type} propName description`
                let value = value.trim_start_matches('{');
                // Skip {type} if present
                let value = if value.contains('}') {
                    value.split_once('}').map(|x| x.1).unwrap_or("").trim()
                } else {
                    value
                };
                // First word is the prop name
                if let Some(space) = value.find(char::is_whitespace) {
                    let prop_name = &value[..space];
                    let desc = value[space..].trim();
                    tags.insert(format!("param:{}", prop_name), desc.to_owned());
                } else if !value.is_empty() {
                    tags.insert(format!("param:{}", value), String::new());
                }
            } else {
                tags.insert(tag.to_owned(), value.to_owned());
            }
        } else if in_tags && !line.is_empty() {
            // Continuation of a tag — ignore for now
        }
    }

    tags
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn is_pascal_case(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_uppercase())
}

/// Get the declared name from a Declaration node.
fn declaration_name<'a>(decl: &Declaration<'a>) -> Option<&'a str> {
    match decl {
        Declaration::VariableDeclaration(vd) => {
            vd.declarations.first().and_then(|d| match &d.id {
                BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
                _ => None,
            })
        }
        Declaration::FunctionDeclaration(f) => f.id.as_ref().map(|id| id.name.as_str()),
        Declaration::ClassDeclaration(c) => c.id.as_ref().map(|id| id.name.as_str()),
        Declaration::TSTypeAliasDeclaration(ta) => Some(ta.id.name.as_str()),
        Declaration::TSInterfaceDeclaration(iface) => Some(iface.id.name.as_str()),
        Declaration::TSEnumDeclaration(e) => Some(e.id.name.as_str()),
        Declaration::TSModuleDeclaration(m) => match &m.id {
            TSModuleDeclarationName::Identifier(id) => Some(id.name.as_str()),
            TSModuleDeclarationName::StringLiteral(s) => Some(s.value.as_str()),
        },
        _ => None,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path(rel: &str) -> std::path::PathBuf {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("../../fixtures").join(rel)
    }

    #[test]
    fn test_shadcn_button() {
        let fixture = fixture_path("shadcn/button.tsx");
        let source = std::fs::read_to_string(&fixture)
            .unwrap_or_else(|_| panic!("fixture not found: {}", fixture.display()));
        let path = Utf8Path::new("/fixtures/shadcn/button.tsx");
        let data = parse_file(path, &source);

        assert!(
            data.component_mappings.iter().any(|m| m.component_name == "Button"),
            "Button component not found; mappings: {:?}",
            data.component_mappings.iter().map(|m| &m.component_name).collect::<Vec<_>>()
        );

        let btn = data.component_mappings.iter().find(|m| m.component_name == "Button").unwrap();
        assert_eq!(
            btn.props_type_name.as_str(),
            "ButtonProps",
            "Expected props_type_name 'ButtonProps', got '{}'",
            btn.props_type_name
        );
    }

    #[test]
    fn test_shadcn_input() {
        let fixture = fixture_path("shadcn/input.tsx");
        let source = std::fs::read_to_string(&fixture)
            .unwrap_or_else(|_| panic!("fixture not found: {}", fixture.display()));
        let path = Utf8Path::new("/fixtures/shadcn/input.tsx");
        let data = parse_file(path, &source);

        assert!(
            data.component_mappings.iter().any(|m| m.component_name == "Input"),
            "Input component not found; mappings: {:?}",
            data.component_mappings.iter().map(|m| &m.component_name).collect::<Vec<_>>()
        );

        let inp = data.component_mappings.iter().find(|m| m.component_name == "Input").unwrap();
        assert_eq!(
            inp.props_type_name.as_str(),
            "InputProps",
            "Expected props_type_name 'InputProps', got '{}'",
            inp.props_type_name
        );
    }

    #[test]
    fn test_radix_button_dts() {
        let fixture = fixture_path("radix/button.d.ts");
        let source = std::fs::read_to_string(&fixture)
            .unwrap_or_else(|_| panic!("fixture not found: {}", fixture.display()));
        // .d.ts is NOT tsx — component detection is off, but interfaces should still collect
        let path = Utf8Path::new("/fixtures/radix/button.d.ts");
        let data = parse_file(path, &source);

        // The radix fixture declares `ButtonProps` interface
        let interface_names: Vec<_> = data.interfaces.keys().cloned().collect();
        assert!(
            interface_names.iter().any(|k| k.contains("ButtonProps")),
            "ButtonProps interface not collected; interfaces: {:?}",
            interface_names
        );
    }

    #[test]
    fn test_interface_collected() {
        let source = r#"
            export interface FooProps {
                label: string;
                count?: number;
                onClick: () => void;
            }
        "#;
        let path = Utf8Path::new("/test/foo.tsx");
        let data = parse_file(path, source);

        assert!(data.interfaces.contains_key("/test/foo.tsx:FooProps"));
        let iface = &data.interfaces["/test/foo.tsx:FooProps"];
        assert_eq!(iface.name.as_str(), "FooProps");
        assert_eq!(iface.props.len(), 3);

        let label = iface.props.iter().find(|p| p.name == "label").unwrap();
        assert_eq!(label.raw_type, "string");
        assert!(label.required);

        let count = iface.props.iter().find(|p| p.name == "count").unwrap();
        assert!(!count.required);
    }

    #[test]
    fn test_import_binding_collected() {
        let source = r#"
            import React, { FC, useState } from "react";
            import type { ButtonHTMLAttributes } from "react";
            import * as RadixButton from "@radix-ui/react-button";
        "#;
        let path = Utf8Path::new("/test/imports.ts");
        let data = parse_file(path, source);

        // FC should be recorded
        assert!(
            data.imports.iter().any(|i| i.local_name.as_str() == "FC"),
            "FC not in imports: {:?}",
            data.imports.iter().map(|i| i.local_name.as_str()).collect::<Vec<_>>()
        );

        // ButtonHTMLAttributes should be type-only
        let bha = data.imports.iter().find(|i| i.local_name.as_str() == "ButtonHTMLAttributes");
        assert!(bha.is_some(), "ButtonHTMLAttributes not found");
        assert!(bha.unwrap().is_type_only, "ButtonHTMLAttributes should be type-only");

        // Namespace import
        assert!(
            data.imports.iter().any(|i| i.local_name.as_str() == "RadixButton"),
            "RadixButton namespace not in imports"
        );
    }

    #[test]
    fn test_fc_pattern() {
        let source = r#"
            import { FC } from "react";
            interface ButtonProps { label: string; }
            const Button: FC<ButtonProps> = ({ label }) => <button>{label}</button>;
        "#;
        let path = Utf8Path::new("/test/button.tsx");
        let data = parse_file(path, source);

        let btn = data.component_mappings.iter().find(|m| m.component_name == "Button");
        assert!(btn.is_some(), "Button not found via FC pattern");
        assert_eq!(btn.unwrap().props_type_name.as_str(), "ButtonProps");
    }

    #[test]
    fn test_hoc_pattern() {
        let source = r#"
            interface CardProps { title: string; }
            const Card = memo(function Card(props: CardProps) { return null; });
        "#;
        let path = Utf8Path::new("/test/card.tsx");
        let data = parse_file(path, source);

        let card = data.component_mappings.iter().find(|m| m.component_name == "Card");
        assert!(card.is_some(), "Card not found via HOC pattern");
        assert_eq!(card.unwrap().props_type_name.as_str(), "CardProps");
    }

    #[test]
    fn test_display_name_update() {
        let source = r#"
            interface BtnProps { text: string; }
            const Btn = React.forwardRef<HTMLButtonElement, BtnProps>(function(props, ref) {
                return null;
            });
            Btn.displayName = "Button";
        "#;
        let path = Utf8Path::new("/test/btn.tsx");
        let data = parse_file(path, source);

        // After displayName assignment, the component should be renamed
        let btn = data.component_mappings.iter().find(|m| m.component_name == "Button");
        assert!(btn.is_some(), "Button (renamed via displayName) not found; mappings: {:?}",
            data.component_mappings.iter().map(|m| &m.component_name).collect::<Vec<_>>());
    }

    #[test]
    fn test_type_alias_omit() {
        let source = r#"
            interface FullProps { a: string; b: number; c: boolean; }
            type PartialProps = Omit<FullProps, 'b' | 'c'>;
        "#;
        let path = Utf8Path::new("/test/types.ts");
        let data = parse_file(path, source);

        let alias = data.type_aliases.get("/test/types.ts:PartialProps");
        assert!(alias.is_some(), "PartialProps alias not collected");
        match alias.unwrap() {
            CollectedTypeAlias::Omit { base_name, omitted_keys, .. } => {
                assert_eq!(base_name.as_str(), "FullProps");
                assert!(omitted_keys.contains(&"b".to_owned()));
                assert!(omitted_keys.contains(&"c".to_owned()));
            }
            other => panic!("Expected Omit, got {:?}", other),
        }
    }

    #[test]
    fn test_exports_collected() {
        let source = r#"
            export { Button } from "./button";
            export * from "./types";
            export type { ButtonProps } from "./button";
        "#;
        let path = Utf8Path::new("/test/index.ts");
        let data = parse_file(path, source);

        assert!(!data.exports.is_empty(), "No exports collected");
        assert!(
            data.exports.iter().any(|e| matches!(e, LexedExport::ReExportAll { .. })),
            "ReExportAll not found"
        );
        assert!(
            data.exports.iter().any(|e| matches!(e, LexedExport::ReExportNamed { .. })),
            "ReExportNamed not found"
        );
    }

    #[test]
    fn test_enum_collected() {
        let source = r#"
            enum Direction {
                Up = "UP",
                Down = "DOWN",
                Left = "LEFT",
                Right = "RIGHT",
            }
        "#;
        let path = Utf8Path::new("/test/enum.ts");
        let data = parse_file(path, source);

        let entries = data.enums.get("/test/enum.ts:Direction");
        assert!(entries.is_some(), "Direction enum not collected");
        let entries = entries.unwrap();
        assert_eq!(entries.len(), 4);
        assert!(entries.iter().any(|e| e.name == "Up"));
    }
}
