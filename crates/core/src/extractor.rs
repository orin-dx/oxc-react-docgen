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
    CollectedInterface, CollectedObjectField, CollectedType, CollectedTypeAlias, ComponentMapping,
    DefaultSource, EnumEntry, EnumValue, ExtendsRef, ImportBinding, LexedExport, RawDefault,
    RawProp, SourceData,
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
        if crate::react_types::is_react_builtin(lookup_name, &rustc_hash::FxHashSet::default()) {
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
            Some(tp) => {
                tp.params.iter().map(|p| self.ts_type_to_collected(p).to_raw_string()).collect()
            }
            None => vec![],
        }
    }

    // ─── TSType → CollectedType ───────────────────────────────────────────────

    fn ts_type_to_collected<'a>(&self, ty: &TSType<'a>) -> CollectedType {
        match ty {
            TSType::TSStringKeyword(_) => CollectedType::String,
            TSType::TSNumberKeyword(_) => CollectedType::Number,
            TSType::TSBooleanKeyword(_) => CollectedType::Boolean,
            TSType::TSNullKeyword(_) => CollectedType::Null,
            TSType::TSUndefinedKeyword(_) => CollectedType::Undefined,
            TSType::TSAnyKeyword(_) => CollectedType::Any,
            TSType::TSNeverKeyword(_) => CollectedType::Never,
            TSType::TSUnknownKeyword(_) => CollectedType::Unknown,
            TSType::TSVoidKeyword(_) => CollectedType::Void,
            TSType::TSBigIntKeyword(_) => CollectedType::BigInt,
            TSType::TSSymbolKeyword(_) => CollectedType::Symbol,
            TSType::TSObjectKeyword(_) => {
                CollectedType::Named { name: "object".into(), args: vec![] }
            }

            TSType::TSLiteralType(lit) => match &lit.literal {
                TSLiteral::StringLiteral(s) => {
                    CollectedType::StringLiteral(s.value.as_str().into())
                }
                TSLiteral::NumericLiteral(n) => CollectedType::NumberLiteral(n.value),
                TSLiteral::BooleanLiteral(b) => CollectedType::BoolLiteral(b.value),
                TSLiteral::UnaryExpression(u) => {
                    // Handle negative numbers: -1
                    let raw =
                        self.source[u.span.start as usize..u.span.end as usize].to_owned();
                    CollectedType::Raw(raw)
                }
                _ => CollectedType::Raw(
                    self.source[lit.span.start as usize..lit.span.end as usize].to_owned(),
                ),
            },

            TSType::TSTypeReference(tr) => {
                let name: CompactString = self.ts_type_name_str(&tr.type_name).into();
                let args = tr
                    .type_arguments
                    .as_ref()
                    .map(|ta| ta.params.iter().map(|p| self.ts_type_to_collected(p)).collect())
                    .unwrap_or_default();
                CollectedType::Named { name, args }
            }

            TSType::TSTypeQuery(q) => {
                let name = self.ts_type_query_name(q);
                CollectedType::TypeOf(name.into())
            }

            TSType::TSUnionType(u) => {
                let members: Vec<CollectedType> =
                    u.types.iter().map(|t| self.ts_type_to_collected(t)).collect();
                CollectedType::Union(members)
            }

            TSType::TSIntersectionType(i) => {
                let members: Vec<CollectedType> =
                    i.types.iter().map(|t| self.ts_type_to_collected(t)).collect();
                CollectedType::Intersection(members)
            }

            TSType::TSArrayType(a) => {
                CollectedType::Array(Box::new(self.ts_type_to_collected(&a.element_type)))
            }

            TSType::TSTupleType(t) => {
                let members: Vec<CollectedType> = t
                    .element_types
                    .iter()
                    .map(|el| self.ts_tuple_element_to_collected(el))
                    .collect();
                CollectedType::Tuple(members)
            }

            TSType::TSTypeLiteral(lit) => {
                let fields: Vec<CollectedObjectField> = lit
                    .members
                    .iter()
                    .filter_map(|member| self.ts_signature_to_object_field(member))
                    .collect();
                CollectedType::Object(fields)
            }

            TSType::TSFunctionType(f) => {
                // In OXC 0.135, FormalParameter has type_annotation as a separate field
                let params: Vec<CollectedType> = f
                    .params
                    .items
                    .iter()
                    .map(|p| {
                        p.type_annotation
                            .as_ref()
                            .map(|ta| self.ts_type_to_collected(&ta.type_annotation))
                            .unwrap_or(CollectedType::Any)
                    })
                    .collect();
                // return_type on TSFunctionType is Box<TSTypeAnnotation> (not Option)
                let return_type =
                    self.ts_type_to_collected(&f.return_type.type_annotation);
                CollectedType::Function {
                    params,
                    return_type: Box::new(return_type),
                }
            }

            TSType::TSIndexedAccessType(ia) => CollectedType::IndexedAccess {
                obj: Box::new(self.ts_type_to_collected(&ia.object_type)),
                key: Box::new(self.ts_type_to_collected(&ia.index_type)),
            },

            TSType::TSTemplateLiteralType(tl) => {
                let mut parts: Vec<CollectedType> = Vec::new();
                for (i, quasi) in tl.quasis.iter().enumerate() {
                    let s = quasi.value.raw.as_str();
                    if !s.is_empty() {
                        parts.push(CollectedType::StringLiteral(s.into()));
                    }
                    if let Some(ty) = tl.types.get(i) {
                        parts.push(self.ts_type_to_collected(ty));
                    }
                }
                CollectedType::TemplateLiteral(parts)
            }

            TSType::TSConditionalType(c) => CollectedType::Conditional {
                check: Box::new(self.ts_type_to_collected(&c.check_type)),
                extends_type: Box::new(self.ts_type_to_collected(&c.extends_type)),
                true_type: Box::new(self.ts_type_to_collected(&c.true_type)),
                false_type: Box::new(self.ts_type_to_collected(&c.false_type)),
            },

            TSType::TSMappedType(m) => {
                // In OXC 0.135, TSMappedType has `constraint: TSType` directly (not via
                // type_parameter) and `type_annotation: Option<TSType>` (not Box<TSTypeAnnotation>)
                let key_type = self.ts_type_to_collected(&m.constraint);
                let value_type = m
                    .type_annotation
                    .as_ref()
                    .map(|ta| self.ts_type_to_collected(ta))
                    .unwrap_or(CollectedType::Unknown);
                CollectedType::Mapped {
                    key_type: Box::new(key_type),
                    value_type: Box::new(value_type),
                }
            }

            TSType::TSParenthesizedType(p) => {
                // Unwrap parentheses — (Type) → Type
                self.ts_type_to_collected(&p.type_annotation)
            }

            // TSTypeOperatorType covers keyof, unique, readonly
            TSType::TSTypeOperatorType(op) => {
                let raw =
                    self.source[op.span.start as usize..op.span.end as usize].to_owned();
                CollectedType::Raw(raw)
            }

            TSType::TSInferType(i) => {
                let raw =
                    self.source[i.span.start as usize..i.span.end as usize].to_owned();
                CollectedType::Raw(raw)
            }

            // Anything else: capture raw source text as fallback
            _ => {
                use oxc_span::GetSpan;
                let span = ty.span();
                let raw =
                    self.source[span.start as usize..span.end as usize].to_owned();
                CollectedType::Raw(raw)
            }
        }
    }

    /// Convert a `TSTupleElement` (which is a superset of `TSType`) to a `CollectedType`.
    ///
    /// TSTupleElement inherits all TSType variants and adds TSOptionalType and TSRestType.
    fn ts_tuple_element_to_collected<'a>(&self, el: &TSTupleElement<'a>) -> CollectedType {
        match el {
            TSTupleElement::TSOptionalType(o) => {
                // T? in tuple → Union([T, Undefined])
                let inner = self.ts_type_to_collected(&o.type_annotation);
                CollectedType::Union(vec![inner, CollectedType::Undefined])
            }
            TSTupleElement::TSRestType(r) => {
                // ...T[] in tuple → Array(T)
                CollectedType::Array(Box::new(self.ts_type_to_collected(&r.type_annotation)))
            }
            // All TSType variants are inherited — we can safely transmute via span+raw fallback
            // but the cleanest approach is to match the known shared variants.
            // Since TSTupleElement inherits TSType, we cast via the span trick:
            other => {
                use oxc_span::GetSpan;
                let span = other.span();
                let raw = self.source[span.start as usize..span.end as usize].to_owned();
                // Try to produce a real type from source-text parsing would require re-parsing,
                // so instead we check common patterns via span-based raw parsing and use
                // the raw fallback for anything we can't directly handle.
                // For the common case of plain TSType variants (not TSOptionalType/TSRestType),
                // we need to re-interpret the element as a TSType. We can't do this safely
                // at runtime without unsafe, but all such variants produce valid CollectedType
                // via raw text. Most real tuple usage is simple types.
                CollectedType::Raw(raw)
            }
        }
    }

    fn ts_type_query_name<'a>(&self, q: &TSTypeQuery<'a>) -> String {
        // TSTypeQueryExprName can be Identifier or a qualified name
        // Capture the source text of the expression name
        use oxc_span::GetSpan;
        let span = q.expr_name.span();
        self.source[span.start as usize..span.end as usize].to_owned()
    }

    fn ts_signature_to_object_field<'a>(
        &self,
        member: &TSSignature<'a>,
    ) -> Option<CollectedObjectField> {
        match member {
            TSSignature::TSPropertySignature(sig) => {
                let name = match &sig.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str().to_owned(),
                    PropertyKey::StringLiteral(s) => s.value.as_str().to_owned(),
                    _ => return None,
                };
                let collected_type = sig
                    .type_annotation
                    .as_ref()
                    .map(|ta| self.ts_type_to_collected(&ta.type_annotation))
                    .unwrap_or(CollectedType::Any);
                let description = self.find_jsdoc(sig.span.start);
                Some(CollectedObjectField {
                    name,
                    collected_type,
                    required: !sig.optional,
                    description,
                })
            }
            TSSignature::TSMethodSignature(sig) => {
                let name = match &sig.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str().to_owned(),
                    PropertyKey::StringLiteral(s) => s.value.as_str().to_owned(),
                    _ => return None,
                };
                Some(CollectedObjectField {
                    name,
                    collected_type: CollectedType::Function {
                        params: vec![CollectedType::Raw("...".into())],
                        return_type: Box::new(CollectedType::Any),
                    },
                    required: !sig.optional,
                    description: String::new(),
                })
            }
            _ => None,
        }
    }

    #[allow(dead_code)]
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
                let collected_type = ps
                    .type_annotation
                    .as_ref()
                    .map(|ta| self.ts_type_to_collected(&ta.type_annotation))
                    .unwrap_or(CollectedType::Any);

                let description = self.find_jsdoc(ps.span.start);
                let tags = self.extract_jsdoc_tags(ps.span.start);

                Some(RawProp {
                    name,
                    collected_type,
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
                    collected_type: CollectedType::Function {
                        params: vec![CollectedType::Raw("...".into())],
                        return_type: Box::new(CollectedType::Any),
                    },
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
                        let (base_name, base_args) =
                            self.extract_type_name_from_type(&tp.params[0])?;
                        let base = CollectedType::Named {
                            name: base_name,
                            args: base_args
                                .into_iter()
                                .map(CollectedType::Raw)
                                .collect(),
                        };
                        let omitted_keys = self.collect_string_union_keys(&tp.params[1]);
                        Some(CollectedTypeAlias::Omit { base, omitted_keys, file_path: fp })
                    }
                    "Pick" => {
                        let tp = tr.type_arguments.as_ref()?;
                        if tp.params.len() < 2 {
                            return None;
                        }
                        let (base_name, base_args) =
                            self.extract_type_name_from_type(&tp.params[0])?;
                        let base = CollectedType::Named {
                            name: base_name,
                            args: base_args
                                .into_iter()
                                .map(CollectedType::Raw)
                                .collect(),
                        };
                        let picked_keys = self.collect_string_union_keys(&tp.params[1]);
                        Some(CollectedTypeAlias::Pick { base, picked_keys, file_path: fp })
                    }
                    "Partial" => {
                        let tp = tr.type_arguments.as_ref()?;
                        let (base_name, base_args) =
                            self.extract_type_name_from_type(tp.params.first()?)?;
                        let base = CollectedType::Named {
                            name: base_name,
                            args: base_args
                                .into_iter()
                                .map(CollectedType::Raw)
                                .collect(),
                        };
                        Some(CollectedTypeAlias::Partial { base, file_path: fp })
                    }
                    "Required" => {
                        let tp = tr.type_arguments.as_ref()?;
                        let (base_name, base_args) =
                            self.extract_type_name_from_type(tp.params.first()?)?;
                        let base = CollectedType::Named {
                            name: base_name,
                            args: base_args
                                .into_iter()
                                .map(CollectedType::Raw)
                                .collect(),
                        };
                        Some(CollectedTypeAlias::Required { base, file_path: fp })
                    }
                    _ => {
                        // Simple passthrough: `type Size = SomeOtherType`
                        let args = self.extract_type_args(&tr.type_arguments);
                        let target = CollectedType::Named {
                            name: ref_name.into(),
                            args: args.into_iter().map(CollectedType::Raw).collect(),
                        };
                        Some(CollectedTypeAlias::Passthrough { target, file_path: fp })
                    }
                }
            }
            TSType::TSUnionType(u) => {
                // Check if all members are string/number literals → LiteralUnion
                let all_string_literals = u.types.iter().all(|t| match t {
                    TSType::TSLiteralType(lit) => {
                        matches!(lit.literal, TSLiteral::StringLiteral(_))
                    }
                    TSType::TSUndefinedKeyword(_) | TSType::TSNullKeyword(_) => true,
                    _ => false,
                });

                let members: Vec<CollectedType> =
                    u.types.iter().map(|t| self.ts_type_to_collected(t)).collect();

                if all_string_literals {
                    let member_strs: Vec<String> =
                        members.iter().map(|m| m.to_raw_string()).collect();
                    return Some(CollectedTypeAlias::LiteralUnion {
                        members: member_strs,
                        file_path: fp,
                    });
                }
                Some(CollectedTypeAlias::Union { members, file_path: fp })
            }
            TSType::TSIntersectionType(i) => {
                let members =
                    i.types.iter().map(|t| self.ts_type_to_collected(t)).collect();
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
            param_defaults: Default::default(),
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
    fn try_forward_ref_exotic_decl<'a>(
        &self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        let type_ann = decl.type_annotation.as_ref()?;
        let ct = self.ts_type_to_collected(&type_ann.type_annotation);

        // Look for ForwardRefExoticComponent<P & RefAttributes<E>>
        // or ForwardRefExoticComponent<P>
        let (type_name, args) = match &ct {
            CollectedType::Named { name, args } => (name.as_str(), args.as_slice()),
            _ => return None,
        };

        if !matches!(
            type_name,
            "ForwardRefExoticComponent" | "React.ForwardRefExoticComponent"
        ) {
            return None;
        }

        let first_arg = args.first()?;

        // Extract P from P & RefAttributes<E>
        let props_type = match first_arg {
            CollectedType::Intersection(members) => {
                // Find the member that is NOT RefAttributes/RefAttributes<E>
                members
                    .iter()
                    .find(|m| {
                        !matches!(m, CollectedType::Named { name, .. }
                            if matches!(name.as_str(), "RefAttributes" | "React.RefAttributes"))
                    })
                    .unwrap_or(first_arg)
            }
            other => other,
        };

        let (props_name, props_args) = match props_type {
            CollectedType::Named { name, args } => (name.clone(), args.clone()),
            _ => return None,
        };

        // Convert args to strings for ComponentMapping (resolver will re-parse)
        let props_type_args: Vec<String> =
            props_args.iter().map(|a| a.to_raw_string()).collect();

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

    /// Extract default values from destructured function parameters.
    ///
    /// Handles `({ size = 'md', disabled = false }: Props) => ...` patterns.
    fn extract_param_defaults<'a>(
        &self,
        params: &FormalParameters<'a>,
    ) -> rustc_hash::FxHashMap<String, RawDefault> {
        let mut defaults = rustc_hash::FxHashMap::default();

        let Some(first_param) = params.items.first() else {
            return defaults;
        };
        // In OXC 0.135, BindingPattern is an enum directly (not a struct with .kind)
        let BindingPattern::ObjectPattern(obj) = &first_param.pattern else {
            return defaults;
        };

        for prop in &obj.properties {
            // In OXC 0.135, BindingProperty.value is BindingPattern.
            // A default value `{ size = 'md' }` is represented as
            // BindingProperty { key: "size", value: BindingPattern::AssignmentPattern { left: "size", right: 'md' } }
            let (name, default_expr) = match &prop.value {
                BindingPattern::AssignmentPattern(ap) => {
                    let name = match &ap.left {
                        BindingPattern::BindingIdentifier(id) => id.name.as_str().to_owned(),
                        _ => continue,
                    };
                    (name, &ap.right)
                }
                _ => continue,
            };
            let raw_default = self.eval_expr_as_default(default_expr);
            defaults.insert(name, raw_default);
        }
        defaults
    }

    fn eval_expr_as_default<'a>(&self, expr: &Expression<'a>) -> RawDefault {
        match expr {
            Expression::StringLiteral(s) => RawDefault {
                value: format!("\"{}\"", s.value.as_str()),
                computed: false,
                source: DefaultSource::Destructuring,
            },
            Expression::NumericLiteral(n) => RawDefault {
                value: n.value.to_string(),
                computed: false,
                source: DefaultSource::Destructuring,
            },
            Expression::BooleanLiteral(b) => RawDefault {
                value: b.value.to_string(),
                computed: false,
                source: DefaultSource::Destructuring,
            },
            Expression::NullLiteral(_) => RawDefault {
                value: "null".into(),
                computed: false,
                source: DefaultSource::Destructuring,
            },
            Expression::Identifier(id) if id.name.as_str() == "undefined" => RawDefault {
                value: "undefined".into(),
                computed: false,
                source: DefaultSource::Destructuring,
            },
            // Array and object literals: capture source text, not computed
            Expression::ArrayExpression(_) | Expression::ObjectExpression(_) => {
                use oxc_span::GetSpan;
                let span = expr.span();
                RawDefault {
                    value: self.source[span.start as usize..span.end as usize].to_owned(),
                    computed: false,
                    source: DefaultSource::Destructuring,
                }
            }
            // Everything else (identifier refs, calls, ternaries): computed
            _ => {
                use oxc_span::GetSpan;
                let span = expr.span();
                RawDefault {
                    value: self.source[span.start as usize..span.end as usize].to_owned(),
                    computed: true,
                    source: DefaultSource::Destructuring,
                }
            }
        }
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
                        continue;
                    }
                }
            }
            // Pattern 5: declare const Button: React.ForwardRefExoticComponent<ButtonProps & RefAttributes<E>>
            // Common in .d.ts files — no initializer, just a type annotation
            if declarator.init.is_none() {
                if let Some(name) = self.extract_pascal_name(declarator) {
                    if let Some(mapping) = self.try_forward_ref_exotic_decl(declarator, &name) {
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
                                let param_defaults =
                                    self.extract_param_defaults(&func.params);
                                self.data.component_mappings.push(ComponentMapping {
                                    component_name: name.to_owned(),
                                    props_type_name: props_name,
                                    props_type_args: type_args,
                                    file_path: self.file_path.clone(),
                                    description,
                                    tags,
                                    span_start: func.span.start,
                                    span_end: func.span.end,
                                    param_defaults,
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
        assert_eq!(label.collected_type.to_raw_string(), "string");
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
            CollectedTypeAlias::Omit { base, omitted_keys, .. } => {
                assert_eq!(base.to_raw_string(), "FullProps");
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
