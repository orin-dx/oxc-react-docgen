//! OXC AST extractor — Phase 2a.
//!
//! Walks the OXC AST for a single file and collects everything into a [`SourceData`].
//!
//! **The Absolute Rule:** No AST references escape `parse_file`.
//! The `Allocator` is created locally, and all arena memory is freed when `parse_file` returns.
//! [`SourceData`] contains only owned data (String, Vec, etc.).

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use oxc_allocator::{Allocator, Box as OxcBox};
use oxc_ast::ast::*;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;
use rustc_hash::FxHashSet;

#[cfg(test)]
use crate::types::LexedExport;
use crate::types::{CollectedObjectField, CollectedType, CollectedTypeAlias, ExtendsRef, RawProp, SourceData};

mod alias;
mod component;
mod defaults;
mod interface;
mod jsdoc;
mod visit;

// ─── Entry Point ─────────────────────────────────────────────────────────────

/// Parse a single file and collect all extractable data.
///
/// Completely pure — no I/O, no side effects, no cross-file dependencies.
/// Safe to call in parallel from rayon workers.
pub fn parse_file(path: &Utf8Path, source: &str) -> SourceData {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_default();
    let ret = Parser::new(&allocator, source, source_type).parse();

    // .d.ts declaration files use the same React.FC / ForwardRefExoticComponent patterns as .tsx
    let is_tsx = source_type.is_jsx() || source_type.is_typescript_definition();
    let mut collector = SourceDataCollector::new(path, source, is_tsx);
    // Pass comments by cloning them into owned strings before the allocator drops.
    // The comments Vec lives in the arena; we extract them here.
    let comments: Vec<OwnedComment> = ret
        .program
        .comments
        .iter()
        .map(|c| OwnedComment { span_start: c.span.start, span_end: c.span.end, is_block: c.is_block() })
        .collect();
    collector.comments = comments;
    collector.visit_program(&ret.program);
    collector.finish()
    // allocator drops here — ALL arena memory freed
}

// ─── Owned Comment (so nothing escapes the allocator) ────────────────────────

#[derive(Debug, Clone)]
pub(super) struct OwnedComment {
    pub(super) span_start: u32,
    pub(super) span_end: u32,
    pub(super) is_block: bool,
}

// ─── Collector ───────────────────────────────────────────────────────────────

pub(super) struct SourceDataCollector<'src> {
    pub(super) file_path: Utf8PathBuf,
    pub(super) source: &'src str,
    pub(super) is_tsx: bool,
    pub(super) data: SourceData,
    /// All comments in the file (owned — not tied to allocator lifetime).
    pub(super) comments: Vec<OwnedComment>,
    /// Names that came from imports — used to classify ExtendsRef.
    pub(super) imported_names: FxHashSet<CompactString>,
    /// Tracks which JSDoc comment spans have already been consumed (by span_end).
    /// Prevents the same comment from leaking to both a component description and its first prop.
    pub(super) consumed_jsdoc: FxHashSet<u32>,
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
            consumed_jsdoc: FxHashSet::default(),
        }
    }

    pub(super) fn scoped_key(&self, name: &str) -> String {
        format!("{}:{}", self.file_path, name)
    }

    fn finish(self) -> SourceData {
        self.data
    }

    // ─── Import source specifier lookup ──────────────────────────────────────

    pub(super) fn find_import_specifier(&self, local_name: &str) -> Option<String> {
        self.data.imports.iter().find(|imp| imp.local_name.as_str() == local_name).map(|imp| imp.specifier.clone())
    }

    // ─── ExtendsRef classification ────────────────────────────────────────────

    pub(super) fn classify_extends(&self, name: &str, type_args: Vec<String>) -> ExtendsRef {
        // Strip "React." prefix for lookup in builtin tables
        let lookup_name = name.strip_prefix("React.").unwrap_or(name);

        if let Some(element) = crate::react_types::html_element_for(lookup_name) {
            return ExtendsRef::Builtin { name: name.into(), element: Some(element.to_owned()), type_args };
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

    pub(super) fn ts_type_name_str<'a>(&self, name: &TSTypeName<'a>) -> String {
        match name {
            TSTypeName::IdentifierReference(id) => id.name.as_str().to_owned(),
            TSTypeName::QualifiedName(q) => {
                format!("{}.{}", self.ts_type_name_str(&q.left), q.right.name.as_str())
            }
            TSTypeName::ThisExpression(_) => "this".to_owned(),
        }
    }

    // ─── TSTypeParameterInstantiation → Vec<String> ──────────────────────────

    pub(super) fn extract_type_args<'a>(
        &mut self,
        type_params: &Option<OxcBox<'a, TSTypeParameterInstantiation<'a>>>,
    ) -> Vec<String> {
        match type_params {
            Some(tp) => tp.params.iter().map(|p| self.ts_type_to_collected(p).to_raw_string()).collect(),
            None => vec![],
        }
    }

    // ─── TSType → CollectedType ───────────────────────────────────────────────

    pub(super) fn ts_type_to_collected<'a>(&mut self, ty: &TSType<'a>) -> CollectedType {
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
            TSType::TSObjectKeyword(_) => CollectedType::Named { name: "object".into(), args: vec![] },

            TSType::TSLiteralType(lit) => match &lit.literal {
                TSLiteral::StringLiteral(s) => CollectedType::StringLiteral(s.value.as_str().into()),
                TSLiteral::NumericLiteral(n) => CollectedType::NumberLiteral(n.value),
                TSLiteral::BooleanLiteral(b) => CollectedType::BoolLiteral(b.value),
                TSLiteral::UnaryExpression(u) => {
                    // Handle negative numbers: -1
                    let raw = self.source[u.span.start as usize..u.span.end as usize].to_owned();
                    CollectedType::Raw(raw)
                }
                _ => CollectedType::Raw(self.source[lit.span.start as usize..lit.span.end as usize].to_owned()),
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
                let members: Vec<CollectedType> = u.types.iter().map(|t| self.ts_type_to_collected(t)).collect();
                CollectedType::Union(members)
            }

            TSType::TSIntersectionType(i) => {
                let members: Vec<CollectedType> = i.types.iter().map(|t| self.ts_type_to_collected(t)).collect();
                CollectedType::Intersection(members)
            }

            TSType::TSArrayType(a) => CollectedType::Array(Box::new(self.ts_type_to_collected(&a.element_type))),

            TSType::TSTupleType(t) => {
                let members: Vec<CollectedType> =
                    t.element_types.iter().map(|el| self.ts_tuple_element_to_collected(el)).collect();
                CollectedType::Tuple(members)
            }

            TSType::TSTypeLiteral(lit) => {
                let fields: Vec<CollectedObjectField> =
                    lit.members.iter().filter_map(|member| self.ts_signature_to_object_field(member)).collect();
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
                let return_type = self.ts_type_to_collected(&f.return_type.type_annotation);
                CollectedType::Function { params, return_type: Box::new(return_type) }
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
                CollectedType::Mapped { key_type: Box::new(key_type), value_type: Box::new(value_type) }
            }

            TSType::TSParenthesizedType(p) => {
                // Unwrap parentheses — (Type) → Type
                self.ts_type_to_collected(&p.type_annotation)
            }

            // TSTypeOperatorType covers keyof, unique, readonly
            TSType::TSTypeOperatorType(op) => {
                let raw = self.source[op.span.start as usize..op.span.end as usize].to_owned();
                CollectedType::Raw(raw)
            }

            TSType::TSInferType(i) => {
                let raw = self.source[i.span.start as usize..i.span.end as usize].to_owned();
                CollectedType::Raw(raw)
            }

            // Anything else: capture raw source text as fallback
            _ => {
                use oxc_span::GetSpan;
                let span = ty.span();
                let raw = self.source[span.start as usize..span.end as usize].to_owned();
                CollectedType::Raw(raw)
            }
        }
    }

    /// Convert a `TSTupleElement` (which is a superset of `TSType`) to a `CollectedType`.
    ///
    /// TSTupleElement inherits all TSType variants and adds TSOptionalType and TSRestType.
    pub(super) fn ts_tuple_element_to_collected<'a>(&mut self, el: &TSTupleElement<'a>) -> CollectedType {
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

    pub(super) fn ts_type_query_name<'a>(&self, q: &TSTypeQuery<'a>) -> String {
        // TSTypeQueryExprName can be Identifier or a qualified name
        // Capture the source text of the expression name
        use oxc_span::GetSpan;
        let span = q.expr_name.span();
        self.source[span.start as usize..span.end as usize].to_owned()
    }

    pub(super) fn ts_signature_to_object_field<'a>(
        &mut self,
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
                Some(CollectedObjectField { name, collected_type, required: !sig.optional, description })
            }
            TSSignature::TSMethodSignature(sig) => {
                let name = match &sig.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str().to_owned(),
                    PropertyKey::StringLiteral(s) => s.value.as_str().to_owned(),
                    _ => return None,
                };
                let params: Vec<CollectedType> = sig
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
                let return_type = sig
                    .return_type
                    .as_ref()
                    .map(|rt| self.ts_type_to_collected(&rt.type_annotation))
                    .unwrap_or(CollectedType::Void);
                Some(CollectedObjectField {
                    name,
                    collected_type: CollectedType::Function { params, return_type: Box::new(return_type) },
                    required: !sig.optional,
                    description: String::new(),
                })
            }
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub(super) fn expression_to_string<'a>(&self, expr: &Expression<'a>) -> String {
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
    pub(super) fn extract_type_ref_name<'a>(&self, tr: &TSTypeReference<'a>) -> String {
        self.ts_type_name_str(&tr.type_name)
    }

    /// Extract the first type argument name from a type ref's type params.
    ///
    /// Used for `FC<ButtonProps>` → "ButtonProps".
    /// Handles `PropsWithChildren<P>` and `Readonly<P>` wrappers.
    pub(super) fn extract_props_arg<'a>(
        &mut self,
        type_params: &Option<OxcBox<'a, TSTypeParameterInstantiation<'a>>>,
    ) -> Option<(CompactString, Vec<String>)> {
        let tp = type_params.as_ref()?;
        let first = tp.params.first()?;
        self.extract_type_name_from_type(first)
    }

    /// Get the (name, type_args) of a TSType if it's a simple named reference.
    ///
    /// Unwraps single-layer wrappers like `PropsWithChildren<P>` and `Readonly<P>`.
    pub(super) fn extract_type_name_from_type<'a>(&mut self, ty: &TSType<'a>) -> Option<(CompactString, Vec<String>)> {
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
            TSType::TSParenthesizedType(p) => self.extract_type_name_from_type(&p.type_annotation),
            TSType::TSUnionType(u) => {
                let members: Vec<CollectedType> = u.types.iter().map(|t| self.ts_type_to_collected(t)).collect();
                let bare = format!("__anon_{}", self.data.type_aliases.len());
                let scoped = self.scoped_key(&bare);
                self.data
                    .type_aliases
                    .insert(scoped, CollectedTypeAlias::Union { members, file_path: self.file_path.clone() });
                Some((bare.into(), vec![]))
            }
            TSType::TSIntersectionType(i) => {
                let members: Vec<CollectedType> = i.types.iter().map(|t| self.ts_type_to_collected(t)).collect();
                let bare = format!("__anon_{}", self.data.type_aliases.len());
                let scoped = self.scoped_key(&bare);
                self.data
                    .type_aliases
                    .insert(scoped, CollectedTypeAlias::Intersection { members, file_path: self.file_path.clone() });
                Some((bare.into(), vec![]))
            }
            _ => None,
        }
    }

    // ─── TSInterfaceHeritage → ExtendsRef ────────────────────────────────────

    pub(super) fn collect_extends<'a>(&mut self, ext: &TSInterfaceHeritage<'a>) -> ExtendsRef {
        let name = self.expression_to_ident_name(&ext.expression);
        let type_args = self.extract_type_args(&ext.type_arguments);
        self.classify_extends(&name, type_args)
    }

    pub(super) fn expression_to_ident_name<'a>(&self, expr: &Expression<'a>) -> String {
        match expr {
            Expression::Identifier(id) => id.name.as_str().to_owned(),
            Expression::StaticMemberExpression(me) => {
                format!("{}.{}", self.expression_to_ident_name(&me.object), me.property.name.as_str())
            }
            _ => "unknown".to_owned(),
        }
    }

    // ─── Property Signature collection ───────────────────────────────────────

    pub(super) fn collect_property_signature<'a>(&mut self, sig: &TSSignature<'a>) -> Option<RawProp> {
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
                let params: Vec<CollectedType> = ms
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
                let return_type = ms
                    .return_type
                    .as_ref()
                    .map(|rt| self.ts_type_to_collected(&rt.type_annotation))
                    .unwrap_or(CollectedType::Void);
                Some(RawProp {
                    name,
                    collected_type: CollectedType::Function { params, return_type: Box::new(return_type) },
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
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

pub(super) fn is_pascal_case(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_uppercase())
}

/// Get the declared name from a Declaration node.
pub(super) fn declaration_name<'a>(decl: &Declaration<'a>) -> Option<&'a str> {
    match decl {
        Declaration::VariableDeclaration(vd) => vd.declarations.first().and_then(|d| match &d.id {
            BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
            _ => None,
        }),
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
        let source =
            std::fs::read_to_string(&fixture).unwrap_or_else(|_| panic!("fixture not found: {}", fixture.display()));
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
        let source =
            std::fs::read_to_string(&fixture).unwrap_or_else(|_| panic!("fixture not found: {}", fixture.display()));
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
        let source =
            std::fs::read_to_string(&fixture).unwrap_or_else(|_| panic!("fixture not found: {}", fixture.display()));
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
        assert!(
            btn.is_some(),
            "Button (renamed via displayName) not found; mappings: {:?}",
            data.component_mappings.iter().map(|m| &m.component_name).collect::<Vec<_>>()
        );
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
        assert!(data.exports.iter().any(|e| matches!(e, LexedExport::ReExportAll { .. })), "ReExportAll not found");
        assert!(data.exports.iter().any(|e| matches!(e, LexedExport::ReExportNamed { .. })), "ReExportNamed not found");
    }

    #[test]
    fn test_memo_forward_ref_detected() {
        let source = r#"
            import React, { memo, forwardRef } from "react";
            interface ButtonProps { label: string; disabled?: boolean }
            export const Button = memo(forwardRef<HTMLButtonElement, ButtonProps>((props, ref) => (
                <button ref={ref} {...props}>{props.label}</button>
            )));
        "#;
        let path = Utf8Path::new("/test/button.tsx");
        let data = parse_file(path, source);
        let mapping = data.component_mappings.iter().find(|m| m.component_name == "Button");
        assert!(mapping.is_some(), "Button not detected via memo(forwardRef(...))");
        let mapping = mapping.unwrap();
        assert_eq!(mapping.props_type_name, "ButtonProps");
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
