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
use oxc_span::{SourceType, Span};
use rustc_hash::FxHashSet;

#[cfg(test)]
use crate::types::LexedExport;
use crate::types::{
    CollectedObjectField, CollectedType, CollectedTypeAlias, Diagnostic, DiagnosticCode, DiagnosticSeverity,
    ExtendsRef, RawProp, SourceData,
};

mod alias;
mod component;
mod defaults;
mod interface;
mod jsdoc;
mod visit;

// ─── Entry Point ─────────────────────────────────────────────────────────────

/// Maximum bracket-nesting depth a source file may contain before we refuse to parse it.
///
/// `oxc_parser`'s recursive-descent grammar has no depth guard for nested parenthesized
/// types, object type literals, or conditional types — a file with ~6,000+ levels of
/// nesting deterministically stack-overflows the parser itself (confirmed via a
/// standalone parser-only harness). 2000 leaves a wide safety margin.
const MAX_SOURCE_NESTING_DEPTH: usize = 2000;

/// Maximum AST recursion depth for `ts_type_to_collected` and its mutually
/// recursive siblings. `max_bracket_nesting_depth` bounds raw-text bracket
/// depth as a cheap pre-parse proxy for parser stack safety, but chained
/// conditional types (`A extends B ? C extends D ? ... : ... : ...`) add one
/// AST level per `? :` with no brackets at all — the proxy metric undercounts
/// exactly this shape. This counter guards the extractor's own recursion the
/// same way the resolver's `depth: u8` / `MAX_DEPTH` already guards
/// `resolve_collected_type` (see `resolver/mod.rs`), just at a higher ceiling
/// since this walk is a single in-process AST-to-struct conversion, not
/// cross-file resolution. `MAX_SOURCE_NESTING_DEPTH` (bracket-based) and this
/// constant (AST-based) are independent knobs and do not need to match.
const MAX_TYPE_COLLECT_DEPTH: u8 = 200;

/// Cheap linear scan bounding the maximum bracket-nesting depth of `source`.
///
/// Only tracks a running max, not full balance — sufficient to bound recursion depth
/// before handing the source to the real parser. Comments and string/template
/// literals are skipped entirely rather than scanned for brackets: real .d.ts
/// files (TypeScript's own `lib.dom.d.ts` included) ship prose JSDoc with
/// unmatched brackets — e.g. MDN-scraped artifacts like `MISSING: RFC(5646,
/// '...')].` — that would otherwise drive `depth` negative and, once negative
/// enough, wrap on the next legitimate bracket. Doesn't track `${...}`
/// interpolation inside template literals as real nesting; under-counting a
/// rare template-literal type's internal depth is an acceptable trade-off for
/// a crash-prevention heuristic, never causing a false rejection.
fn max_bracket_nesting_depth(source: &str) -> usize {
    let bytes = source.as_bytes();
    let mut depth: usize = 0;
    let mut max_depth: usize = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            quote @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' && i + 1 < bytes.len() { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
            }
            b'(' | b'{' | b'[' => {
                depth += 1;
                max_depth = max_depth.max(depth);
                i += 1;
            }
            b')' | b'}' | b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            _ => i += 1,
        }
    }
    max_depth
}

/// Parse a single file and collect all extractable data.
///
/// Completely pure — no I/O, no side effects, no cross-file dependencies.
/// Safe to call in parallel from rayon workers.
///
/// Never panics on malformed input: excessive nesting and TypeScript syntax errors
/// are reported via `SourceData::diagnostics` rather than failing silently or
/// letting the parser overrun the stack.
pub fn parse_file(path: &Utf8Path, source: &str) -> SourceData {
    let observed_depth = max_bracket_nesting_depth(source);
    if observed_depth > MAX_SOURCE_NESTING_DEPTH {
        let mut data = SourceData::default();
        data.diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            message: format!(
                "File exceeds maximum type nesting depth ({observed_depth} > {MAX_SOURCE_NESTING_DEPTH}), skipped to avoid parser stack overflow"
            ),
            file: Some(path.to_string()),
            line: None,
            column: None,
            help: Some("Reduce nested parenthesized/object/conditional types in this file.".into()),
            code: DiagnosticCode::ExcessiveNesting,
        });
        return data;
    }

    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_default();
    let ret = Parser::new(&allocator, source, source_type).parse();

    // .d.ts declaration files use the same React.FC / ForwardRefExoticComponent patterns as .tsx
    let is_tsx = source_type.is_jsx() || source_type.is_typescript_definition();
    let mut collector = SourceDataCollector::new(path, source, is_tsx);

    // oxc_parser is error-recovering: `ret.program` is still usable even when
    // `ret.errors` is non-empty. Surface each error as a diagnostic instead of
    // silently treating the source as if it parsed cleanly.
    for err in &ret.errors {
        collector.data.diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            message: err.to_string(),
            file: Some(path.to_string()),
            line: None,
            column: None,
            help: None,
            code: DiagnosticCode::ParseError,
        });
    }

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
    /// Enclosing `namespace X { ... }` names, outermost first. Type references to a
    /// namespace member are always fully qualified (`X.Y`, per `ts_type_name_str`'s
    /// `TSTypeName::QualifiedName` handling) — so storage keys must match, or a same-file
    /// reference to a namespace member can never resolve.
    pub(super) namespace_stack: Vec<CompactString>,
    /// `X.displayName = "..."` renames recorded during the main traversal but
    /// not yet applied — see `try_scan_display_name`. Applying these
    /// immediately would change `component_name` out from under any later
    /// static-property scan (`try_scan_default_props`, another displayName
    /// assignment) still looking the mapping up by its original identifier;
    /// deferring to `finish()` means every other scan always sees the
    /// original identifier for the whole file, regardless of source order.
    pub(super) pending_display_name_renames: Vec<(std::string::String, std::string::String)>,
    /// Identifiers that were the *source* of a `try_rename_identifier_wrapped_component`
    /// alias — e.g. `InternalButton` in `const Button = InternalButton;`. Once
    /// aliased, the original is implementation detail, not a second public
    /// component; filtered out of the final `component_mappings` in `finish()`.
    pub(super) aliased_away: FxHashSet<CompactString>,
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
            namespace_stack: Vec::new(),
            pending_display_name_renames: Vec::new(),
            aliased_away: FxHashSet::default(),
        }
    }

    /// Record that a recognized-but-malformed AST shape was skipped — distinct
    /// from "wrong shape, not a candidate at all" (which stays silent). Used by
    /// `classify_type_alias`'s Omit/Pick/Partial/Required/Readonly arms and the
    /// component-detector chains in `visit.rs` when a shape matches a known
    /// pattern but is missing/malformed pieces the pattern requires.
    pub(super) fn record_skip(&mut self, code: DiagnosticCode, message: impl Into<String>, span: Span) {
        let _ = span; // no line/column conversion helper exists yet; kept for future use and call-site documentation
        self.data.diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Info,
            message: message.into(),
            file: Some(self.file_path.to_string()),
            line: None,
            column: None,
            help: None,
            code,
        });
    }

    pub(super) fn scoped_key(&self, name: &str) -> String {
        if self.namespace_stack.is_empty() {
            format!("{}:{}", self.file_path, name)
        } else {
            format!("{}:{}.{}", self.file_path, self.namespace_stack.join("."), name)
        }
    }

    fn finish(mut self) -> SourceData {
        // Filtered by original identifier first, before any displayName rename
        // below can change what identifier a mapping currently answers to.
        if !self.aliased_away.is_empty() {
            let aliased_away = &self.aliased_away;
            self.data.component_mappings.retain(|m| !aliased_away.contains(m.component_name.as_str()));
        }
        for (obj_name, display_name) in self.pending_display_name_renames.drain(..) {
            if let Some(mapping) = self.data.component_mappings.iter_mut().find(|m| m.component_name == obj_name) {
                mapping.component_name = display_name;
            }
        }
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

        // SVGAttributes/SVGProps/HTMLProps carry no element in their own name —
        // unlike html_element_for's other entries (ButtonHTMLAttributes, etc.),
        // where the element is baked into the interface name itself. Real call
        // sites always supply a concrete DOM element as the type argument
        // (`SVGAttributes<SVGSVGElement>`, `HTMLProps<HTMLDivElement>`) — derive
        // the element from that instead of falling through to html_element_for's
        // static result (None for SVGAttributes/SVGProps, a generic "div"
        // fallback for HTMLProps) whenever it's one this crate recognizes.
        if matches!(lookup_name, "SVGAttributes" | "SVGProps" | "HTMLProps") {
            if let Some(element) = type_args.first().and_then(|arg| crate::react_types::html_element_from_type_arg(arg))
            {
                return ExtendsRef::Builtin { name: name.into(), element: Some(element.to_owned()), type_args };
            }
        }

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
            Some(tp) => tp.params.iter().map(|p| self.ts_type_to_collected_at_depth(p, 0).to_raw_string()).collect(),
            None => vec![],
        }
    }

    // ─── TSType → CollectedType ───────────────────────────────────────────────

    pub(super) fn ts_type_to_collected<'a>(&mut self, ty: &TSType<'a>) -> CollectedType {
        self.ts_type_to_collected_at_depth(ty, 0)
    }

    fn ts_type_to_collected_at_depth<'a>(&mut self, ty: &TSType<'a>, depth: u8) -> CollectedType {
        if depth > MAX_TYPE_COLLECT_DEPTH {
            use oxc_span::GetSpan;
            let span = ty.span();
            self.data.diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                message: format!(
                    "Type nesting exceeds maximum extractor recursion depth ({depth} > {MAX_TYPE_COLLECT_DEPTH})"
                ),
                file: Some(self.file_path.to_string()),
                line: None,
                column: None,
                help: Some("This may indicate a deeply chained conditional or mapped type.".into()),
                code: DiagnosticCode::MaxDepthExceeded,
            });
            let raw = self.source[span.start as usize..span.end as usize].to_owned();
            return CollectedType::Raw(raw);
        }
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
                    .map(|ta| ta.params.iter().map(|p| self.ts_type_to_collected_at_depth(p, depth + 1)).collect())
                    .unwrap_or_default();
                CollectedType::Named { name, args }
            }

            TSType::TSTypeQuery(q) => {
                let name = self.ts_type_query_name(q);
                CollectedType::TypeOf(name.into())
            }

            TSType::TSUnionType(u) => {
                let members: Vec<CollectedType> =
                    u.types.iter().map(|t| self.ts_type_to_collected_at_depth(t, depth + 1)).collect();
                CollectedType::Union(members)
            }

            TSType::TSIntersectionType(i) => {
                let members: Vec<CollectedType> =
                    i.types.iter().map(|t| self.ts_type_to_collected_at_depth(t, depth + 1)).collect();
                CollectedType::Intersection(members)
            }

            TSType::TSArrayType(a) => {
                CollectedType::Array(Box::new(self.ts_type_to_collected_at_depth(&a.element_type, depth + 1)))
            }

            TSType::TSTupleType(t) => {
                let members: Vec<CollectedType> =
                    t.element_types.iter().map(|el| self.ts_tuple_element_to_collected(el, depth + 1)).collect();
                CollectedType::Tuple(members)
            }

            TSType::TSTypeLiteral(lit) => {
                let fields: Vec<CollectedObjectField> = lit
                    .members
                    .iter()
                    .filter_map(|member| self.ts_signature_to_object_field(member, depth + 1))
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
                            .map(|ta| self.ts_type_to_collected_at_depth(&ta.type_annotation, depth + 1))
                            .unwrap_or(CollectedType::Any)
                    })
                    .collect();
                let param_names: Vec<Option<CompactString>> =
                    f.params.items.iter().map(|p| binding_pattern_name(&p.pattern)).collect();
                // return_type on TSFunctionType is Box<TSTypeAnnotation> (not Option)
                let return_type = self.ts_type_to_collected_at_depth(&f.return_type.type_annotation, depth + 1);
                CollectedType::Function { params, param_names, return_type: Box::new(return_type) }
            }

            TSType::TSIndexedAccessType(ia) => CollectedType::IndexedAccess {
                obj: Box::new(self.ts_type_to_collected_at_depth(&ia.object_type, depth + 1)),
                key: Box::new(self.ts_type_to_collected_at_depth(&ia.index_type, depth + 1)),
            },

            TSType::TSTemplateLiteralType(tl) => {
                let mut parts: Vec<CollectedType> = Vec::new();
                for (i, quasi) in tl.quasis.iter().enumerate() {
                    let s = quasi.value.raw.as_str();
                    if !s.is_empty() {
                        parts.push(CollectedType::StringLiteral(s.into()));
                    }
                    if let Some(ty) = tl.types.get(i) {
                        parts.push(self.ts_type_to_collected_at_depth(ty, depth + 1));
                    }
                }
                CollectedType::TemplateLiteral(parts)
            }

            TSType::TSConditionalType(c) => CollectedType::Conditional {
                check: Box::new(self.ts_type_to_collected_at_depth(&c.check_type, depth + 1)),
                extends_type: Box::new(self.ts_type_to_collected_at_depth(&c.extends_type, depth + 1)),
                true_type: Box::new(self.ts_type_to_collected_at_depth(&c.true_type, depth + 1)),
                false_type: Box::new(self.ts_type_to_collected_at_depth(&c.false_type, depth + 1)),
            },

            TSType::TSMappedType(m) => {
                // In OXC 0.135, TSMappedType has `constraint: TSType` directly (not via
                // type_parameter) and `type_annotation: Option<TSType>` (not Box<TSTypeAnnotation>)
                let key_type = self.ts_type_to_collected_at_depth(&m.constraint, depth + 1);
                let value_type = m
                    .type_annotation
                    .as_ref()
                    .map(|ta| self.ts_type_to_collected_at_depth(ta, depth + 1))
                    .unwrap_or(CollectedType::Unknown);
                CollectedType::Mapped { key_type: Box::new(key_type), value_type: Box::new(value_type) }
            }

            TSType::TSParenthesizedType(p) => {
                // Unwrap parentheses — (Type) → Type
                self.ts_type_to_collected_at_depth(&p.type_annotation, depth + 1)
            }

            // TSTypeOperatorType covers keyof, unique, readonly. `keyof` is kept
            // structured (its operand may itself need substitution or resolution —
            // see `CollectedType::KeyOf`). `readonly`/`unique` are peeled
            // transparently to their operand — docgen doesn't track mutability or
            // symbol uniqueness, so there's nothing to preserve — rather than
            // capturing the whole modified type as raw source text, which used to
            // make `readonly string[]` (real @types/react's ButtonHTMLAttributes
            // `defaultValue`/`value` shape) degrade to Opaque even though the
            // element type is fully knowable.
            TSType::TSTypeOperatorType(op) => match op.operator {
                TSTypeOperatorOperator::Keyof => {
                    CollectedType::KeyOf(Box::new(self.ts_type_to_collected_at_depth(&op.type_annotation, depth + 1)))
                }
                TSTypeOperatorOperator::Unique | TSTypeOperatorOperator::Readonly => {
                    self.ts_type_to_collected_at_depth(&op.type_annotation, depth + 1)
                }
            },

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
    pub(super) fn ts_tuple_element_to_collected<'a>(&mut self, el: &TSTupleElement<'a>, depth: u8) -> CollectedType {
        match el {
            TSTupleElement::TSOptionalType(o) => {
                // T? in tuple → Union([T, Undefined])
                let inner = self.ts_type_to_collected_at_depth(&o.type_annotation, depth + 1);
                CollectedType::Union(vec![inner, CollectedType::Undefined])
            }
            TSTupleElement::TSRestType(r) => {
                // ...T[] in tuple → Array(T)
                CollectedType::Array(Box::new(self.ts_type_to_collected_at_depth(&r.type_annotation, depth + 1)))
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
        depth: u8,
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
                    .map(|ta| self.ts_type_to_collected_at_depth(&ta.type_annotation, depth + 1))
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
                            .map(|ta| self.ts_type_to_collected_at_depth(&ta.type_annotation, depth + 1))
                            .unwrap_or(CollectedType::Any)
                    })
                    .collect();
                let param_names: Vec<Option<CompactString>> =
                    sig.params.items.iter().map(|p| binding_pattern_name(&p.pattern)).collect();
                let return_type = sig
                    .return_type
                    .as_ref()
                    .map(|rt| self.ts_type_to_collected_at_depth(&rt.type_annotation, depth + 1))
                    .unwrap_or(CollectedType::Void);
                Some(CollectedObjectField {
                    name,
                    collected_type: CollectedType::Function { params, param_names, return_type: Box::new(return_type) },
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
            TSType::TSTypeLiteral(_) => {
                // Bare inline object type used directly as props, e.g. `FC<{ x: string }>`
                // or `forwardRef<Elem, { x: string }>`. Synthesize an anonymous passthrough
                // alias so the resolver's existing alias machinery can resolve it, instead
                // of silently dropping the whole component (the `_ => None` fallback below).
                let collected = self.ts_type_to_collected(ty);
                let bare = format!("__anon_{}", self.data.type_aliases.len());
                let scoped = self.scoped_key(&bare);
                self.data.type_aliases.insert(
                    scoped,
                    CollectedTypeAlias::Passthrough { target: collected, file_path: self.file_path.clone() },
                );
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

                let (description, tags) = self.find_jsdoc_with_tags(ps.span.start);

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
                let (description, tags) = self.find_jsdoc_with_tags(ms.span.start);
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
                let param_names: Vec<Option<CompactString>> =
                    ms.params.items.iter().map(|p| binding_pattern_name(&p.pattern)).collect();
                let return_type = ms
                    .return_type
                    .as_ref()
                    .map(|rt| self.ts_type_to_collected(&rt.type_annotation))
                    .unwrap_or(CollectedType::Void);
                Some(RawProp {
                    name,
                    collected_type: CollectedType::Function { params, param_names, return_type: Box::new(return_type) },
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

/// A parameter's simple identifier name, if it has one (not a destructured pattern).
fn binding_pattern_name(pattern: &BindingPattern) -> Option<CompactString> {
    match pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().into()),
        _ => None,
    }
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
    use crate::types::{DefaultSource, EnumValue};

    fn fixture_path(rel: &str) -> std::path::PathBuf {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("../../fixtures").join(rel)
    }

    #[test]
    fn test_readonly_array_type_operator_peeled_transparently() {
        // `readonly string[]` (the type-operator form, not the `Readonly<T>` utility
        // type — already handled separately) captured its entire span as raw source
        // text unconditionally, found while investigating whether fully resolving
        // @types/react's real HTMLAttributes chain was tractable: ButtonHTMLAttributes'
        // real `defaultValue`/`value` fields use exactly this shape. Downstream
        // heuristics reject a raw string containing a space, so it degraded to
        // Opaque even though the actual element type (`string`) is fully knowable.
        let source = r#"
interface Props { items: readonly string[]; }
"#;
        let path = Utf8Path::new("/fixtures/readonly-array.tsx");
        let data = parse_file(path, source);

        let key = format!("{path}:Props");
        let iface = data.interfaces.get(&key).unwrap_or_else(|| panic!("expected interfaces to contain '{key}'"));
        let items = iface.props.iter().find(|p| p.name == "items").expect("'items' prop not found");

        assert!(
            matches!(&items.collected_type, CollectedType::Array(inner) if matches!(**inner, CollectedType::String)),
            "expected Array(String), got {:?}",
            items.collected_type
        );
    }

    #[test]
    fn test_inline_object_type_alias_not_silently_dropped() {
        let source = r#"
type ToastVariant = { message: string; kind?: 'info' | 'error' };
"#;
        let path = Utf8Path::new("/fixtures/inline-alias.tsx");
        let data = parse_file(path, source);

        let key = format!("{path}:ToastVariant");
        let alias = data.type_aliases.get(&key).unwrap_or_else(|| {
            panic!("expected type_aliases to contain '{key}', got keys: {:?}", data.type_aliases.keys())
        });

        match alias {
            CollectedTypeAlias::Passthrough { target: CollectedType::Object(fields), .. } => {
                assert_eq!(fields.len(), 2);
                assert!(fields.iter().any(|f| f.name == "message"));
                assert!(fields.iter().any(|f| f.name == "kind"));
            }
            other => panic!("expected Passthrough{{Object}}, got {other:?}"),
        }
    }

    #[test]
    fn test_bare_function_type_alias_not_silently_dropped() {
        // react-day-picker's real pattern: `type OnSelectHandler<T> = (selected: T, ...) => void`
        // — a bare function type as the alias body, previously falling through
        // classify_type_alias's `_ => None` and vanishing from data.type_aliases with
        // no diagnostic, so every `OnSelectHandler<Date>` reference resolved as unknown.
        let source = r#"
type OnSelectHandler<T> = (selected: T, triggerDate: Date) => void;
"#;
        let path = Utf8Path::new("/fixtures/function-alias.tsx");
        let data = parse_file(path, source);

        let key = format!("{path}:OnSelectHandler");
        let alias = data.type_aliases.get(&key).unwrap_or_else(|| {
            panic!("expected type_aliases to contain '{key}', got keys: {:?}", data.type_aliases.keys())
        });

        match alias {
            CollectedTypeAlias::Passthrough { target: CollectedType::Function { params, .. }, .. } => {
                assert_eq!(params.len(), 2, "expected 2 params, got {params:?}");
            }
            other => panic!("expected Passthrough{{Function}}, got {other:?}"),
        }
    }

    #[test]
    fn test_array_type_alias_not_silently_dropped() {
        // Storybook's real pattern: `type API_KeyCollection = string[]` — a bare
        // array type as the alias body. Same silent-vanishing bug as the function-
        // type/type-literal cases above: classify_type_alias's `_ => None` catch-all
        // dropped ANY type shape without a dedicated arm, not just those two, so
        // every `API_KeyCollection` reference resolved as unknown with a "Cannot
        // resolve type" diagnostic despite being a plain same-file declaration.
        let source = r#"
type API_KeyCollection = string[];
"#;
        let path = Utf8Path::new("/fixtures/array-alias.tsx");
        let data = parse_file(path, source);

        let key = format!("{path}:API_KeyCollection");
        let alias = data.type_aliases.get(&key).unwrap_or_else(|| {
            panic!("expected type_aliases to contain '{key}', got keys: {:?}", data.type_aliases.keys())
        });

        match alias {
            CollectedTypeAlias::Passthrough { target: CollectedType::Array(inner), .. } => {
                assert!(matches!(**inner, CollectedType::String), "expected Array<String>, got {inner:?}");
            }
            other => panic!("expected Passthrough{{Array}}, got {other:?}"),
        }
    }

    #[test]
    fn test_tuple_type_alias_not_silently_dropped() {
        // Same class of bug as the array case above, exercised with a different
        // previously-unhandled shape to prove the fix is a general catch-all
        // fallback, not another narrowly-scoped special case for arrays only.
        let source = r#"
type Point = [number, number];
"#;
        let path = Utf8Path::new("/fixtures/tuple-alias.tsx");
        let data = parse_file(path, source);

        let key = format!("{path}:Point");
        let alias = data.type_aliases.get(&key).unwrap_or_else(|| {
            panic!("expected type_aliases to contain '{key}', got keys: {:?}", data.type_aliases.keys())
        });

        match alias {
            CollectedTypeAlias::Passthrough { target: CollectedType::Tuple(members), .. } => {
                assert_eq!(members.len(), 2, "expected 2 tuple members, got {members:?}");
            }
            other => panic!("expected Passthrough{{Tuple}}, got {other:?}"),
        }
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
    fn test_interface_description_not_stolen_by_first_prop() {
        let source = r#"
/** Props for Button. */
interface ButtonProps {
  variant: string;
}
"#;
        let path = Utf8Path::new("/fixtures/inline.tsx");
        let data = parse_file(path, source);

        let iface = data.interfaces.values().find(|i| i.name.as_str() == "ButtonProps").unwrap();
        assert_eq!(iface.description, "Props for Button.");
        assert_eq!(iface.props[0].description, "");
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
    fn test_default_props_assignment_merged_into_component_mapping() {
        // MUI-style: `Component.defaultProps = { size: 'md' }` (deprecated in
        // React 19 but still shipped in real .d.ts/.tsx). Distinct from
        // destructured defaults (`function Button({ size = 'md' })`, already
        // handled by extract_param_defaults) — this is a static assignment
        // after the component's own declaration.
        let source = r#"
            interface ButtonProps { size?: string; disabled?: boolean; }
            function Button(props: ButtonProps) {
                return null;
            }
            Button.defaultProps = { size: 'md', disabled: false };
        "#;
        let path = Utf8Path::new("/test/button.tsx");
        let data = parse_file(path, source);

        let button = data.component_mappings.iter().find(|m| m.component_name == "Button");
        assert!(button.is_some(), "Button mapping not found");
        let button = button.unwrap();

        let size_default = button.param_defaults.get("size");
        assert!(size_default.is_some(), "expected 'size' default from defaultProps, got {:?}", button.param_defaults);
        let size_default = size_default.unwrap();
        assert_eq!(size_default.value, "\"md\"");
        assert!(!size_default.computed);
        assert!(matches!(size_default.source, DefaultSource::DefaultProps));

        let disabled_default = button.param_defaults.get("disabled");
        assert!(disabled_default.is_some(), "expected 'disabled' default from defaultProps");
        assert_eq!(disabled_default.unwrap().value, "false");
    }

    #[test]
    fn test_default_props_scan_survives_a_displayname_rename_to_a_different_string() {
        // Adversarial review finding: try_scan_display_name used to rename
        // mapping.component_name to the *string value* of `X.displayName = "..."`
        // immediately. When that string differs from the variable identifier
        // (a real, plausible pattern — an internal implementation name exposed
        // under a nicer public displayName), a later `X.defaultProps = {...}`
        // referencing the *original* identifier could no longer find the
        // mapping, since its component_name had already changed out from under
        // it. The rename must not affect lookups for the rest of the file.
        let source = r#"
            interface ButtonProps { size?: string; }
            function InternalButton(props: ButtonProps) {
                return null;
            }
            InternalButton.displayName = 'PublicButton';
            InternalButton.defaultProps = { size: 'md' };
        "#;
        let path = Utf8Path::new("/test/button.tsx");
        let data = parse_file(path, source);

        let names: Vec<&str> = data.component_mappings.iter().map(|m| m.component_name.as_str()).collect();
        assert!(names.contains(&"PublicButton"), "expected the displayName rename to apply, got {:?}", names);

        let button = data.component_mappings.iter().find(|m| m.component_name == "PublicButton");
        let button = button.expect("PublicButton mapping not found");
        let size_default = button.param_defaults.get("size");
        assert!(
            size_default.is_some(),
            "expected 'size' default from defaultProps to have been merged despite the earlier \
             displayName rename to a different string, got {:?}",
            button.param_defaults
        );
    }

    #[test]
    fn test_two_aliases_to_the_same_base_component_both_survive() {
        // Adversarial review finding: try_rename_identifier_wrapped_component
        // mutated the matched mapping's component_name in place. A second,
        // different alias to the same base component (a real pattern —
        // exposing both a legacy and a new name for the same implementation)
        // could never find the base again, since the first alias had already
        // renamed it away — silently dropping the second alias with no trace.
        let source = r#"
            interface ButtonProps { label: string; }
            const InternalButton = React.forwardRef<HTMLButtonElement, ButtonProps>((props, ref) => {
                return null;
            });
            const LegacyButton = InternalButton;
            const Button = InternalButton;
            export default Button;
        "#;
        let path = Utf8Path::new("/test/button.tsx");
        let data = parse_file(path, source);

        let names: Vec<&str> = data.component_mappings.iter().map(|m| m.component_name.as_str()).collect();
        assert!(names.contains(&"LegacyButton"), "expected LegacyButton to survive as its own alias, got {:?}", names);
        assert!(names.contains(&"Button"), "expected Button to survive as its own alias, got {:?}", names);
        assert!(
            !names.contains(&"InternalButton"),
            "the internal implementation name should not also appear as a separate public component, got {:?}",
            names
        );
    }

    #[test]
    fn test_identifier_wrapped_component_renamed_to_export_binding() {
        // Headless UI's real pattern: a standalone top-level function declaration
        // (already independently detected as its own component by Pattern 4) is
        // later wrapped by a same-file custom wrapper — not React's own forwardRef,
        // a library-defined one — and referenced by bare identifier, not inlined.
        // Before this fix, `ButtonFn` stayed visible under its own inner name
        // instead of being recognized as `ListboxButton`'s real implementation.
        let source = r#"
            interface ButtonProps { disabled?: boolean; }
            function ButtonFn(props: ButtonProps) {
                return null;
            }
            export let ListboxButton = forwardRefWithAs(ButtonFn) as unknown as SomeExportedType;
        "#;
        let path = Utf8Path::new("/test/listbox.tsx");
        let data = parse_file(path, source);

        let names: Vec<&str> = data.component_mappings.iter().map(|m| m.component_name.as_str()).collect();
        assert!(
            names.contains(&"ListboxButton"),
            "expected ListboxButton (the real export name) among component mappings, got {:?}",
            names
        );
        assert!(
            !names.contains(&"ButtonFn"),
            "ButtonFn should be renamed to its real export name, not left visible as a separate/wrong component, got {:?}",
            names
        );
    }

    #[test]
    fn test_bare_identifier_passthrough_alias_renames_component() {
        // antd's real Button pattern: no wrapper call at all, just a plain
        // `const NewName = OldName;` re-binding — narrower than
        // try_rename_identifier_wrapped_component's original CallExpression
        // case (Headless UI's forwardRefWithAs(ButtonFn) above), which never
        // matched a bare identifier init and left this silently unrenamed.
        let source = r#"
            interface ButtonProps { label: string; }
            const InternalCompoundedButton = React.forwardRef<HTMLButtonElement, ButtonProps>((props, ref) => {
                return null;
            });
            const Button = InternalCompoundedButton;
            export default Button;
        "#;
        let path = Utf8Path::new("/test/button.tsx");
        let data = parse_file(path, source);

        let names: Vec<&str> = data.component_mappings.iter().map(|m| m.component_name.as_str()).collect();
        assert!(names.contains(&"Button"), "expected 'Button' (the real export name) among mappings, got {:?}", names);
        assert!(
            !names.contains(&"InternalCompoundedButton"),
            "InternalCompoundedButton should be renamed to its real export name, not left visible under the \
             internal name, got {:?}",
            names
        );
    }

    #[test]
    fn test_bare_identifier_alias_with_as_cast_renames_component() {
        // The real upstream antd shape (per fixtures/antd/Button.tsx's own
        // comment): `const Button = InternalCompoundedButton as
        // CompoundedComponent;` — same bare-identifier passthrough, wrapped in
        // an `as` cast. unwrap_as_expression must see through it.
        let source = r#"
            interface ButtonProps { label: string; }
            const InternalCompoundedButton = React.forwardRef<HTMLButtonElement, ButtonProps>((props, ref) => {
                return null;
            });
            type CompoundedComponent = typeof InternalCompoundedButton & { Group: unknown };
            const Button = InternalCompoundedButton as CompoundedComponent;
            export default Button;
        "#;
        let path = Utf8Path::new("/test/button.tsx");
        let data = parse_file(path, source);

        let names: Vec<&str> = data.component_mappings.iter().map(|m| m.component_name.as_str()).collect();
        assert!(names.contains(&"Button"), "expected 'Button' among mappings, got {:?}", names);
        assert!(!names.contains(&"InternalCompoundedButton"), "got {:?}", names);
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
    fn test_namespace_member_stored_under_qualified_name() {
        // Base UI's real pattern: `namespace MenuRoot { export type Props<Payload> = ... }`,
        // referenced elsewhere in the same file as `MenuRoot.Props<SomePayload>`. The
        // resolver looks up references by their fully-qualified dotted name (confirmed via
        // ts_type_name_str's TSTypeName::QualifiedName handling), so storage must key on
        // the same qualified name, not the bare member name.
        let source = r#"
            namespace MenuRoot {
                export type Props<Payload> = { open?: boolean; payload?: Payload };
            }
        "#;
        let path = Utf8Path::new("/test/menu-root.tsx");
        let data = parse_file(path, source);

        let qualified_key = format!("{path}:MenuRoot.Props");
        assert!(
            data.type_aliases.contains_key(&qualified_key),
            "expected type_aliases to contain '{qualified_key}', got keys: {:?}",
            data.type_aliases.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_deprecated_tag_does_not_bleed_to_sibling_props_without_jsdoc() {
        // Ant Design's real pattern: one prop has a real @deprecated JSDoc comment;
        // adjacent sibling props (declared close enough to fall within find_jsdoc's
        // proximity window) have no JSDoc of their own at all. Only the annotated
        // prop should carry the tag.
        let source = r#"
            interface ButtonProps {
                /** @deprecated use iconPlacement instead */
                iconPosition?: 'start' | 'end';
                shape?: 'default' | 'circle' | 'round';
                size?: 'small' | 'middle' | 'large';
            }
        "#;
        let path = Utf8Path::new("/test/button.tsx");
        let data = parse_file(path, source);

        let key = format!("{path}:ButtonProps");
        let iface = data.interfaces.get(&key).expect("ButtonProps interface not collected");

        let icon_position = iface.props.iter().find(|p| p.name == "iconPosition").expect("iconPosition not found");
        assert!(icon_position.tags.contains_key("deprecated"), "iconPosition should carry its own @deprecated tag");

        let shape = iface.props.iter().find(|p| p.name == "shape").expect("shape not found");
        assert!(
            !shape.tags.contains_key("deprecated"),
            "shape has no JSDoc of its own and must not inherit iconPosition's @deprecated tag, got tags: {:?}",
            shape.tags
        );

        let size = iface.props.iter().find(|p| p.name == "size").expect("size not found");
        assert!(
            !size.tags.contains_key("deprecated"),
            "size has no JSDoc of its own and must not inherit iconPosition's @deprecated tag, got tags: {:?}",
            size.tags
        );
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
    fn test_forward_ref_component_annotation_with_as_cast() {
        // Fluent UI's real pattern: no explicit forwardRef<Ref, Props> type args, and
        // no type annotation on the render function's own props param — the props type
        // is only ever spelled out via the wrapper annotation and the matching trailing
        // `as` cast (both point at the same ForwardRefComponent<ButtonProps>).
        let source = r#"
            import * as React from "react";
            interface ButtonProps { label: string; disabled?: boolean }
            type ForwardRefComponent<P> = (props: P) => React.ReactElement | null;
            export const Button: ForwardRefComponent<ButtonProps> = React.forwardRef((props, ref) => (
                <button ref={ref}>{props.label}</button>
            )) as ForwardRefComponent<ButtonProps>;
        "#;
        let path = Utf8Path::new("/test/button.tsx");
        let data = parse_file(path, source);
        let mapping = data.component_mappings.iter().find(|m| m.component_name == "Button");
        assert!(mapping.is_some(), "Button not detected via ForwardRefComponent<Props> annotation + as-cast");
        let mapping = mapping.unwrap();
        assert_eq!(mapping.props_type_name, "ButtonProps");
    }

    #[test]
    fn test_forward_ref_explicit_generics_survives_as_cast() {
        // Defensive generalization: an explicit forwardRef<Ref, Props>(fn) call can also
        // be wrapped in a trailing `as` cast (e.g. to a component-family union type).
        let source = r#"
            import { forwardRef } from "react";
            interface ButtonProps { label: string }
            type AnyComponent = unknown;
            export const Button = forwardRef<HTMLButtonElement, ButtonProps>((props, ref) => (
                <button ref={ref}>{props.label}</button>
            )) as AnyComponent;
        "#;
        let path = Utf8Path::new("/test/button2.tsx");
        let data = parse_file(path, source);
        let mapping = data.component_mappings.iter().find(|m| m.component_name == "Button");
        assert!(mapping.is_some(), "Button not detected via explicit forwardRef<Ref, Props> wrapped in an as-cast");
        let mapping = mapping.unwrap();
        assert_eq!(mapping.props_type_name, "ButtonProps");
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

    #[test]
    fn test_const_array_as_const_collected() {
        // antd's `type ButtonType = (typeof _ButtonTypes)[number]` pattern —
        // a flat array literal, not an object (test_enum_collected above) or a
        // cva-style variants config. Distinct storage: SourceData::const_arrays.
        let source = r#"
            const _ButtonTypes = ['default', 'primary', 'dashed', 'link', 'text'] as const;
        "#;
        let path = Utf8Path::new("/test/buttonHelpers.tsx");
        let data = parse_file(path, source);

        let values = data.const_arrays.get("/test/buttonHelpers.tsx:_ButtonTypes");
        assert!(values.is_some(), "_ButtonTypes const array not collected");
        let values = values.unwrap();
        assert_eq!(values.len(), 5);
        assert_eq!(values[0], EnumValue::String("default".into()));
        assert!(data.enums.is_empty(), "a plain const array must not be captured as an enum");
    }

    #[test]
    fn test_excessive_nesting_guard() {
        // ~2500 levels of paren nesting — well past MAX_SOURCE_NESTING_DEPTH.
        // Before the guard, this would hand a deeply nested expression straight to
        // oxc_parser's recursive-descent parser and risk a stack overflow.
        let nested = "(".repeat(2500) + &")".repeat(2500);
        let source = format!("const x = {nested};");
        let path = Utf8Path::new("/test/deep.ts");

        let data = parse_file(path, &source);

        assert_eq!(data.diagnostics.len(), 1, "expected exactly one diagnostic, got {:?}", data.diagnostics);
        assert_eq!(data.diagnostics[0].code, DiagnosticCode::ExcessiveNesting);
        assert!(data.component_mappings.is_empty(), "no components should be extracted from a skipped file");
        assert!(data.interfaces.is_empty());
    }

    #[test]
    fn test_nesting_guard_ignores_brackets_inside_comments() {
        // Regression test for: TypeScript's own real lib.dom.d.ts ships JSDoc
        // comments scraped from MDN containing artifacts like
        // `... MISSING: RFC(5646, '...')].` — a stray, unmatched `]` with no
        // opening `[` anywhere nearby (confirmed present verbatim ~2000 times
        // in the real file). `max_bracket_nesting_depth` counted brackets
        // byte-by-byte with no comment/string awareness, so each of these
        // drove its running depth negative; once enough had accumulated, the
        // next legitimate `(`/`{`/`[` in real code still left depth negative,
        // and casting that negative `i64` to `usize` wrapped to ~u64::MAX,
        // spuriously tripping the "exceeds maximum nesting depth" guard and
        // silently discarding the entire file — 0 interfaces extracted from a
        // 1.8MB file whose real code nesting never exceeds a handful of levels.
        let noisy_comment = "/** MISSING: RFC(5646, 'tag')]. */\n".repeat(10);
        let source = format!("{noisy_comment}interface Foo {{ bar: string; }}");
        let observed = max_bracket_nesting_depth(&source);
        assert!(observed < 5, "expected shallow depth (comment brackets should not count), got {observed}");
    }

    #[test]
    fn test_parse_error_surfaced_as_diagnostic() {
        // Deliberately malformed: unclosed interface body.
        let source = r#"
            export interface BrokenProps {
                label: string;
        "#;
        let path = Utf8Path::new("/test/broken.tsx");

        let data = parse_file(path, source);

        assert!(
            data.diagnostics.iter().any(|d| d.code == DiagnosticCode::ParseError),
            "expected a ParseError diagnostic; got {:?}",
            data.diagnostics
        );
    }

    #[test]
    fn record_skip_pushes_an_info_diagnostic_with_the_given_code() {
        use oxc_span::Span;
        let path = Utf8Path::new("/test/skip.tsx");
        let mut collector = SourceDataCollector::new(path, "", false);
        collector.record_skip(DiagnosticCode::SkippedCandidate, "malformed Omit<> arguments", Span::new(10, 20));

        assert_eq!(collector.data.diagnostics.len(), 1);
        let diag = &collector.data.diagnostics[0];
        assert_eq!(diag.severity, DiagnosticSeverity::Info);
        assert_eq!(diag.code, DiagnosticCode::SkippedCandidate);
        assert_eq!(diag.message, "malformed Omit<> arguments");
        assert_eq!(diag.file.as_deref(), Some("/test/skip.tsx"));
    }

    #[test]
    fn pascal_case_binding_with_no_matching_detector_records_skipped_candidate() {
        // `const Button = something()` — PascalCase binding, .tsx file, but the
        // init expression matches none of try_fc_annotation / try_forward_ref /
        // try_hoc_wrapped / try_rename_identifier_wrapped_component. Previously
        // the whole chain fell through silently with zero trace it was even
        // considered a component candidate.
        let source = r#"
            const Button = someUnrecognizedFactory();
        "#;
        let path = Utf8Path::new("/test/unrecognized.tsx");
        let data = parse_file(path, source);

        assert!(
            !data.component_mappings.iter().any(|m| m.component_name == "Button"),
            "no mapping should have been produced for an unrecognized pattern"
        );
        assert!(
            data.diagnostics.iter().any(|d| d.code == DiagnosticCode::SkippedCandidate),
            "expected a SkippedCandidate diagnostic, got: {:?}",
            data.diagnostics
        );
    }

    #[test]
    fn pascal_case_function_declaration_with_untyped_first_param_records_skipped_candidate() {
        // `function Button(props) { ... }` — PascalCase FunctionDeclaration,
        // .tsx file, has a first param, but it carries no type annotation at
        // all. Previously the whole chain (type_annotation.as_ref()?...) fell
        // through silently.
        let source = r#"
            function Button(props) {
                return null;
            }
        "#;
        let path = Utf8Path::new("/test/untyped-param.tsx");
        let data = parse_file(path, source);

        assert!(
            !data.component_mappings.iter().any(|m| m.component_name == "Button"),
            "no mapping should have been produced for an untyped first param"
        );
        assert!(
            data.diagnostics.iter().any(|d| d.code == DiagnosticCode::SkippedCandidate),
            "expected a SkippedCandidate diagnostic, got: {:?}",
            data.diagnostics
        );
    }

    #[test]
    fn zero_param_pascal_case_function_declaration_is_not_flagged_as_skipped() {
        // `function Button() { ... }` — a legitimate zero-props component.
        // func.params.items.first() is None here, which is "wrong shape, not
        // a candidate at all" per SkippedCandidate's own doc comment — this
        // function was never a malformed candidate for Pattern 4 (that
        // pattern exists specifically to read a first param's type
        // annotation), so it must not emit a diagnostic. It also produces no
        // mapping, since there's no props type to extract — this is a
        // pre-existing, separate limitation (no props type source at all for
        // truly prop-less components), not something this task changes.
        let source = r#"
            function Button() {
                return null;
            }
        "#;
        let path = Utf8Path::new("/test/zero-param.tsx");
        let data = parse_file(path, source);

        assert!(
            !data.diagnostics.iter().any(|d| d.code == DiagnosticCode::SkippedCandidate),
            "a parameterless component must not be flagged as a skipped candidate, got: {:?}",
            data.diagnostics
        );
    }

    #[test]
    fn deeply_chained_conditional_types_hit_the_depth_guard_not_the_bracket_heuristic() {
        // Adversarial case for the depth-tracking gap: chained conditional types
        // (`A extends B ? C extends D ? ... : ... : ...`) add one AST recursion
        // level per `? :` with only 2 brackets total (none at all, actually —
        // conditional types need no parens/braces/brackets per level), so
        // max_bracket_nesting_depth's proxy metric undercounts this shape badly.
        // Construct enough chained conditionals to exceed the depth guard while
        // staying far under MAX_SOURCE_NESTING_DEPTH's bracket-based limit (2000),
        // proving the depth counter — not the existing bracket guard — is what
        // catches this.
        let mut ty = "boolean".to_owned();
        for i in 0..600 {
            ty = format!("T{i} extends string ? {ty} : never");
        }
        let source = format!("type Deep = {ty};");

        // The bracket-nesting proxy must NOT trip on this source — proves this
        // test is closing a real gap, not duplicating existing coverage.
        assert!(
            max_bracket_nesting_depth(&source) <= MAX_SOURCE_NESTING_DEPTH,
            "test fixture is invalid: bracket heuristic already catches this, \
             defeating the purpose of the adversarial case"
        );

        let path = Utf8Path::new("/test/deep-conditional.ts");
        let data = parse_file(path, &source);

        // Multi-child TSType form (TSConditionalType descends into 4 children —
        // check_type/extends_type/true_type/false_type — simultaneously at the
        // boundary depth), so this emits one diagnostic per child branch that
        // individually crosses the threshold, not a single diagnostic the way a
        // single-child nesting chain (TSTypeReference, TSArrayType, ...) does.
        // See SPEC-EXTRACTOR-001's AC-023 for the single-child-vs-multi-child
        // distinction this exact count is pinned to.
        let count = data.diagnostics.iter().filter(|d| d.code == DiagnosticCode::MaxDepthExceeded).count();
        assert_eq!(count, 4, "expected exactly 4 MaxDepthExceeded diagnostics (one per TSConditionalType child branch that crosses the threshold), got {count}: {:?}", data.diagnostics);
        assert!(data.diagnostics.iter().all(|d| d.severity == DiagnosticSeverity::Warning));
    }

    #[test]
    fn deeply_nested_single_child_type_chain_emits_exactly_one_max_depth_diagnostic() {
        // SPEC-EXTRACTOR-001 AC-023: a single-child nesting form (each level has
        // exactly one nested TSType descendant, unlike TSConditionalType's four)
        // must emit exactly ONE MaxDepthExceeded diagnostic for the whole chain,
        // not one per over-deep node — descent stops at the first node that trips
        // the guard. Contrast with the TSConditionalType case above, which emits 4.
        let mut ty = "string".to_owned();
        for _ in 0..250 {
            ty = format!("Array<{ty}>");
        }
        let source = format!("type Deep = {ty};");
        let path = Utf8Path::new("/test/deep-array.ts");
        let data = parse_file(path, &source);

        let count = data.diagnostics.iter().filter(|d| d.code == DiagnosticCode::MaxDepthExceeded).count();
        assert_eq!(
            count, 1,
            "expected exactly 1 MaxDepthExceeded diagnostic for a single-child chain, got {count}: {:?}",
            data.diagnostics
        );
    }

    // ── SPEC-EXTRACTOR-001 AC-004: the ForwardRefExoticComponent no-init
    // pattern is the sole pattern NOT gated by file extension — assert it
    // maps in a plain .ts file.

    #[test]
    fn forward_ref_exotic_component_no_init_maps_in_plain_ts_file() {
        let source = r#"
            interface IconProps { size: number; }
            declare const Icon: React.ForwardRefExoticComponent<IconProps>;
        "#;
        let path = Utf8Path::new("/test/icon.ts");
        let data = parse_file(path, source);

        let mapping = data.component_mappings.iter().find(|m| m.component_name == "Icon");
        assert!(mapping.is_some(), "expected an Icon mapping in a plain .ts file, got {:?}", data.component_mappings);
        assert_eq!(mapping.unwrap().props_type_name, "IconProps");
    }

    // ── SPEC-EXTRACTOR-001 AC-008: file-extension gating matrix — one fixture
    // across three source-type contexts.

    #[test]
    fn file_extension_gating_matrix() {
        let source = r#"
            interface AProps { a: string; }
            export function A(props: AProps) { return null; }

            interface BProps { b: string; }
            const B: React.ForwardRefComponent<BProps> = null as any;

            interface CProps { c: string; }
            declare const C: React.ForwardRefExoticComponent<CProps>;
        "#;

        let tsx = parse_file(Utf8Path::new("/test/matrix.tsx"), source);
        let names: Vec<&str> = tsx.component_mappings.iter().map(|m| m.component_name.as_str()).collect();
        assert!(
            names.contains(&"A") && names.contains(&"B") && names.contains(&"C"),
            "expected A, B, C in .tsx, got {names:?}"
        );

        let dts = parse_file(Utf8Path::new("/test/matrix.d.ts"), source);
        let names: Vec<&str> = dts.component_mappings.iter().map(|m| m.component_name.as_str()).collect();
        assert!(
            names.contains(&"A") && names.contains(&"B") && names.contains(&"C"),
            "expected A, B, C in .d.ts, got {names:?}"
        );

        let ts = parse_file(Utf8Path::new("/test/matrix.ts"), source);
        let names: Vec<&str> = ts.component_mappings.iter().map(|m| m.component_name.as_str()).collect();
        assert_eq!(names, vec!["C"], "expected ONLY the AC-004 no-init pattern to map in plain .ts, got {names:?}");
    }

    // ── SPEC-EXTRACTOR-001 AC-014: a forward reference (target declared
    // lexically LATER) must not match — the target isn't yet an
    // already-collected component when the wrapping binding is visited.

    #[test]
    fn forward_referenced_wrap_target_is_not_yet_collected_and_is_skipped() {
        let source = r#"
            const Wrap = memo(Later);
            interface LaterProps { x: string; }
            function Later(props: LaterProps) { return null; }
        "#;
        let path = Utf8Path::new("/test/forward-ref.tsx");
        let data = parse_file(path, source);

        assert!(
            !data.component_mappings.iter().any(|m| m.component_name == "Wrap"),
            "no Wrap mapping should exist for a forward reference, got {:?}",
            data.component_mappings
        );
        assert!(
            data.diagnostics.iter().any(|d| d.code == DiagnosticCode::SkippedCandidate),
            "expected a SkippedCandidate diagnostic, got {:?}",
            data.diagnostics
        );
    }

    // ── SPEC-EXTRACTOR-001 AC-015: a lowercase top-level binding matching an
    // otherwise-recognized component shape is silently omitted — no mapping,
    // no diagnostic.

    #[test]
    fn lowercase_top_level_binding_is_silently_omitted() {
        let source = r#"
            interface ButtonProps { label: string; }
            const button = React.forwardRef<HTMLButtonElement, ButtonProps>((props, ref) => null);
        "#;
        let path = Utf8Path::new("/test/lowercase.tsx");
        let data = parse_file(path, source);

        assert!(!data.component_mappings.iter().any(|m| m.component_name == "button"));
        assert!(
            data.diagnostics.is_empty(),
            "lowercase bindings are documented silent omissions, expected no diagnostics, got {:?}",
            data.diagnostics
        );
    }

    // ── SPEC-EXTRACTOR-001 AC-016: a no-initializer PascalCase binding whose
    // type is neither ForwardRefExoticComponent nor an FC-family annotation
    // is silently omitted.

    #[test]
    fn unrelated_no_init_type_annotation_is_silently_omitted() {
        let source = r#"
            declare const Ghost: SomeOtherType;
        "#;
        let path = Utf8Path::new("/test/ghost.tsx");
        let data = parse_file(path, source);

        assert!(!data.component_mappings.iter().any(|m| m.component_name == "Ghost"));
        assert!(data.diagnostics.is_empty(), "expected no diagnostics, got {:?}", data.diagnostics);
    }

    // ── SPEC-EXTRACTOR-001 AC-018: computed/symbol keys are silently omitted;
    // a numeric-literal key extracts as its decimal string form.

    #[test]
    fn computed_symbol_and_numeric_interface_keys() {
        let source = r#"
            interface Props {
                [key: string]: unknown;
                [Symbol.iterator]?: () => void;
                0: string;
                normal: number;
            }
            export function Widget(props: Props) { return null; }
        "#;
        let path = Utf8Path::new("/test/keys.tsx");
        let data = parse_file(path, source);

        let iface = data.interfaces.values().find(|i| i.name == "Props").expect("expected Props interface");
        let prop_names: Vec<&str> = iface.props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(prop_names, vec!["0", "normal"], "expected only the numeric and normal keys, got {prop_names:?}");
        assert!(data.diagnostics.is_empty(), "computed/symbol key omission is silent, got {:?}", data.diagnostics);
    }

    // ── SPEC-EXTRACTOR-001 AC-019: a zero-byte file produces empty
    // collections and no diagnostics.

    #[test]
    fn empty_source_file_produces_empty_collections_and_no_diagnostics() {
        let path = Utf8Path::new("/test/empty.tsx");
        let data = parse_file(path, "");

        assert!(data.component_mappings.is_empty());
        assert!(data.interfaces.is_empty());
        assert!(data.type_aliases.is_empty());
        assert!(data.enums.is_empty());
        assert!(data.imports.is_empty());
        assert!(data.exports.is_empty());
        assert!(data.diagnostics.is_empty());
    }

    // ── SPEC-EXTRACTOR-001 AC-022: a recoverable parse error still extracts
    // every other cleanly-parsed declaration — the clause distinguishing this
    // from AC-026's fatal, whole-file-empty case.

    #[test]
    fn recoverable_parse_error_still_extracts_other_clean_declarations() {
        let source = "type Bad = Partial<>; function Ok(props: OkProps) { return null; }";
        let path = Utf8Path::new("/test/recoverable.tsx");
        let data = parse_file(path, source);

        assert!(
            data.diagnostics.iter().any(|d| d.code == DiagnosticCode::ParseError),
            "expected a ParseError diagnostic, got {:?}",
            data.diagnostics
        );
        assert!(
            data.component_mappings.iter().any(|m| m.component_name == "Ok"),
            "expected Ok's mapping to survive the earlier recoverable parse error, got {:?}",
            data.component_mappings
        );
    }

    // ── SPEC-EXTRACTOR-001 AC-024/AC-025: inline union/intersection/object
    // props types synthesize an anonymous alias; the FC-family no-init
    // pattern's own first type argument routes through the same mechanism.

    #[test]
    fn inline_union_intersection_object_props_synthesize_anonymous_aliases() {
        let source = r#"
            function Card(props: { title: string }) { return null; }
            function Boxy(props: A | B) { return null; }
            declare const Widget: React.FC<{ label: string }>;
        "#;
        let path = Utf8Path::new("/test/anon.tsx");
        let data = parse_file(path, source);

        for name in ["Card", "Boxy", "Widget"] {
            let mapping = data.component_mappings.iter().find(|m| m.component_name == name);
            assert!(mapping.is_some(), "expected a mapping for {name}, got {:?}", data.component_mappings);
            let props_type_name = &mapping.unwrap().props_type_name;
            assert!(props_type_name.starts_with("__anon_"), "expected an anon alias for {name}, got {props_type_name}");
            let scoped_key = format!("{}:{}", path, props_type_name);
            assert!(
                data.type_aliases.contains_key(&scoped_key),
                "expected a type_aliases entry keyed {scoped_key}, got keys {:?}",
                data.type_aliases.keys().collect::<Vec<_>>()
            );
        }
        assert!(
            data.diagnostics.is_empty(),
            "no diagnostic expected for the anon-alias path, got {:?}",
            data.diagnostics
        );
    }

    // ── SPEC-EXTRACTOR-001 AC-026: a fatal parse error empties every named
    // collection for the whole file.

    #[test]
    fn fatal_parse_error_empties_the_whole_file() {
        let source = "function Card(props: Props) { return ;;; ) }";
        let path = Utf8Path::new("/test/fatal.tsx");
        let data = parse_file(path, source);

        assert!(data.component_mappings.is_empty());
        assert!(data.interfaces.is_empty());
        assert!(data.type_aliases.is_empty());
        assert!(data.enums.is_empty());
        assert!(data.imports.is_empty());
        assert!(data.exports.is_empty());
        assert!(
            data.diagnostics.iter().any(|d| d.code == DiagnosticCode::ParseError),
            "expected a ParseError diagnostic, got {:?}",
            data.diagnostics
        );
    }
}
