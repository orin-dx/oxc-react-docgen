# Agent: Extractor (Phase 2a)
# Model: claude-sonnet-4-6
# Runs: After Phase 1a (Types), parallel with Phase 2b (ImportMap) and 2c (Known)
# Owns: crates/core/src/extractor.rs, crates/core/src/react_types.rs

## Mission

Walk OXC AST for a single file, collect everything into a `SourceData`. This is the most complex Rust code in the project. Zero cross-file dependencies — each call is completely independent.

## Acceptance Criteria

- `parse_file(path, source) -> SourceData` is pure and side-effect free
- No AST references escape the function — allocator is local
- `SourceType::from_path` used correctly — tsx vs ts detection
- Component detection covers: FC<P>, forwardRef<E,P>, HOC(function Comp(props: P))
- Interfaces, type aliases, enums, imports, exports all collected
- Tests pass against fixture files

## The Absolute Rule: No AST Refs Escape

```rust
// CORRECT
pub fn parse_file(path: &Utf8Path, source: &str) -> SourceData {
    let allocator = Allocator::default();  // created here
    let source_type = SourceType::from_path(path).unwrap_or_default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    
    let mut collector = SourceDataCollector::new(path, source);
    collector.visit_program(&ret.program);
    collector.finish()
    // allocator drops here — ALL arena memory freed
    // SourceData contains only owned data (String, Vec, etc.)
}

// WRONG — will not compile but shows the intent to avoid:
struct BadCollector<'a> {
    nodes: Vec<&'a TSInterfaceDeclaration<'a>>,  // NEVER store AST refs
}
```

## extractor.rs Structure

```rust
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;

use crate::types::*;

/// Entry point: parse a single file and collect all extractable data.
/// Completely pure — no I/O, no side effects, no cross-file dependencies.
pub fn parse_file(path: &Utf8Path, source: &str) -> SourceData {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_default();
    let ret = Parser::new(&allocator, source, source_type).parse();

    let is_tsx = source_type.is_jsx();
    let mut collector = SourceDataCollector::new(path, source, is_tsx);
    collector.visit_program(&ret.program);
    collector.finish()
}

struct SourceDataCollector<'src> {
    file_path: Utf8PathBuf,
    source: &'src str,
    is_tsx: bool,
    data: SourceData,
    /// Names that came from imports — used to classify ExtendsRef
    imported_names: FxHashSet<CompactString>,
}

impl<'src> SourceDataCollector<'src> {
    fn new(path: &Utf8Path, source: &'src str, is_tsx: bool) -> Self { ... }
    
    fn scoped_key(&self, name: &str) -> String {
        format!("{}:{}", self.file_path, name)
    }
    
    fn finish(self) -> SourceData { self.data }
    
    fn classify_extends(&self, name: &str, type_args: Vec<String>) -> ExtendsRef {
        // Check against baked-in React type names first
        if let Some(element) = crate::react_types::html_element_for(name) {
            return ExtendsRef::Builtin {
                name: name.into(),
                element: Some(element.to_owned()),
                type_args,
            };
        }
        if crate::react_types::is_react_builtin(name) {
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
}

impl<'a, 'src> Visit<'a> for SourceDataCollector<'src> {
    fn visit_import_declaration(&mut self, node: &ImportDeclaration<'a>) {
        // Collect ImportBinding for each imported name
        // Record all imported names into self.imported_names
        let specifier = node.source.value.as_str().to_owned();
        let is_type_only = node.import_kind.is_type();
        
        if let Some(specifiers) = &node.specifiers {
            for spec in specifiers {
                match spec {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => {
                        let local = s.local.name.as_str();
                        let imported = s.imported.name().as_str();
                        self.imported_names.insert(local.into());
                        self.data.imports.push(ImportBinding {
                            local_name: local.into(),
                            exported_name: imported.into(),
                            specifier: specifier.clone(),
                            is_type_only: is_type_only || s.import_kind.is_type(),
                        });
                    }
                    // Handle namespace and default imports similarly
                    _ => {}
                }
            }
        }
    }
    
    fn visit_export_named_declaration(&mut self, node: &ExportNamedDeclaration<'a>) {
        if let Some(source) = &node.source {
            // Re-exports: export { X } from "./y" or export * from "./y"
            let src = source.value.as_str().to_owned();
            for spec in &node.specifiers {
                self.data.exports.push(LexedExport::ReExportNamed {
                    local_name: spec.exported.name().as_str().to_owned(),
                    source_name: spec.local.name().as_str().to_owned(),
                    source_specifier: src.clone(),
                    is_type_only: node.export_kind.is_type(),
                });
            }
        } else {
            // Local exports: export interface Foo / export type Bar / export const X
            if let Some(decl) = &node.declaration {
                // extract name from declaration and push LocalDeclaration
            }
        }
    }

    fn visit_ts_interface_declaration(&mut self, node: &TSInterfaceDeclaration<'a>) {
        let name = node.id.name.as_str();
        let key = self.scoped_key(name);
        
        let extends = node.extends.iter()
            .map(|ext| {
                let ext_name = self.extract_extends_name(ext);
                let type_args = self.extract_type_args(&ext.type_parameters);
                self.classify_extends(&ext_name, type_args)
            })
            .collect();
        
        let props = node.body.body.iter()
            .filter_map(|sig| self.collect_property_signature(sig))
            .collect();
        
        let description = self.find_jsdoc(node.span.start);
        
        self.data.interfaces.insert(key.clone(), CollectedInterface {
            scoped_key: key,
            name: name.into(),
            file_path: self.file_path.clone(),
            props,
            extends,
            description,
            tags: self.extract_jsdoc_tags(node.span.start),
        });
    }
    
    fn visit_ts_type_alias_declaration(&mut self, node: &TSTypeAliasDeclaration<'a>) {
        let name = node.id.name.as_str();
        let key = self.scoped_key(name);
        
        if let Some(alias) = self.classify_type_alias(name, &node.type_annotation) {
            self.data.type_aliases.insert(key, alias);
        }
    }
    
    fn visit_ts_enum_declaration(&mut self, node: &TSEnumDeclaration<'a>) {
        // Collect enum members as EnumEntry vec
    }
    
    fn visit_variable_declaration(&mut self, node: &VariableDeclaration<'a>) {
        for declarator in &node.declarations {
            // Check for: as const enums, cva() calls, component mappings
            self.try_collect_const_enum(declarator);
            if self.is_tsx {
                self.try_collect_component(declarator);
            }
        }
    }
}
```

## Component Detection — The Three Patterns

```rust
impl<'src> SourceDataCollector<'src> {
    fn try_collect_component(&mut self, decl: &VariableDeclarator<'a>) {
        let name = self.extract_pascal_name(decl)?;
        
        // Pattern 1: const Button: FC<ButtonProps> = ...
        if let Some(mapping) = self.try_fc_annotation(decl, &name) {
            self.data.component_mappings.push(mapping);
            return;
        }
        
        // Pattern 2: const Button = forwardRef<HTMLButtonElement, ButtonProps>(...)
        if let Some(mapping) = self.try_forward_ref(decl, &name) {
            self.data.component_mappings.push(mapping);
            return;
        }
        
        // Pattern 3: const Button = anyHOC(function Button(props: ButtonProps) {...})
        // or: const Button = anyHOC(function Button(props: ButtonProps, ref: R) {...})
        if let Some(mapping) = self.try_hoc_wrapped(decl, &name) {
            self.data.component_mappings.push(mapping);
            return;
        }
        
        // Pattern 4: function Button(props: ButtonProps) { ... }  [arrow not needed]
        // Handled in visit_function_declaration
    }
    
    fn try_fc_annotation(
        &self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        // type annotation on the variable: Button: FC<ButtonProps>
        let type_ann = decl.id.type_annotation.as_ref()?;
        self.extract_props_from_type_annotation(&type_ann.type_annotation, name)
    }
    
    fn extract_props_from_type_annotation(
        &self,
        ty: &TSType<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        match ty {
            TSType::TSTypeReference(tr) => {
                let type_name = self.extract_type_ref_name(tr);
                // FC, FunctionComponent, ComponentType, React.FC, React.FunctionComponent
                if !matches!(
                    type_name.as_str(),
                    "FC" | "FunctionComponent" | "ComponentType"
                    | "React.FC" | "React.FunctionComponent" | "React.ComponentType"
                    | "VFC" | "VoidFunctionComponent"
                ) {
                    return None;
                }
                // Unwrap PropsWithChildren<P> and Readonly<P>
                let (props_name, type_args) = self.extract_props_arg(&tr.type_parameters)?;
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
            _ => None,
        }
    }
    
    fn try_forward_ref(
        &self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        let init = decl.init.as_ref()?;
        let call = init.as_call_expression()?;
        let callee_name = self.extract_callee_name(call)?;
        
        if !matches!(callee_name.as_str(), "forwardRef" | "React.forwardRef") {
            return None;
        }
        
        // forwardRef<RefType, PropsType>(fn)
        // PropsType is the second type parameter
        let type_params = call.type_parameters.as_ref()?;
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
            tags: BTreeMap::new(),
            span_start: decl.span.start,
            span_end: decl.span.end,
        })
    }
    
    fn try_hoc_wrapped(
        &self,
        decl: &VariableDeclarator<'a>,
        name: &str,
    ) -> Option<ComponentMapping> {
        let init = decl.init.as_ref()?;
        let call = init.as_call_expression()?;
        
        // First arg should be a function with a typed props param
        let first_arg = call.arguments.first()?;
        let fn_expr = self.extract_function_from_arg(first_arg)?;
        
        // Must be PascalCase — not anonymous utility functions
        if let Some(fn_name) = fn_expr.name {
            if !is_pascal_case(fn_name.name.as_str()) {
                return None;
            }
        }
        
        // Extract props type from first parameter annotation
        let first_param = fn_expr.params.items.first()?;
        let type_ann = first_param.pattern.type_annotation.as_ref()?;
        let (props_name, type_args) = self.extract_type_name_from_type(&type_ann.type_annotation)?;
        
        Some(ComponentMapping {
            component_name: name.to_owned(),
            props_type_name: props_name,
            props_type_args: type_args,
            file_path: self.file_path.clone(),
            description: self.find_jsdoc(decl.span.start),
            tags: BTreeMap::new(),
            span_start: decl.span.start,
            span_end: decl.span.end,
        })
    }
}

fn is_pascal_case(s: &str) -> bool {
    s.starts_with(|c: char| c.is_uppercase())
}
```

## react_types.rs — Baked-In React Type Tables

```rust
//! Baked-in knowledge of React and DOM types.
//! No file I/O — these are compile-time constants derived from @types/react.

/// HTML element names recognized as component-level inheritors.
pub fn html_element_for(type_name: &str) -> Option<&'static str> {
    match type_name {
        "ButtonHTMLAttributes" => Some("button"),
        "InputHTMLAttributes" => Some("input"),
        "TextareaHTMLAttributes" => Some("textarea"),
        "SelectHTMLAttributes" => Some("select"),
        "AnchorHTMLAttributes" => Some("a"),
        "FormHTMLAttributes" => Some("form"),
        "LabelHTMLAttributes" => Some("label"),
        "ImgHTMLAttributes" => Some("img"),
        "VideoHTMLAttributes" => Some("video"),
        "AudioHTMLAttributes" => Some("audio"),
        "HTMLAttributes" => Some("div"),
        "DOMAttributes" => Some("div"),
        "AriaAttributes" => None,  // not an element, but known
        _ => None,
    }
}

/// Types that are terminal — never need further resolution.
pub fn is_react_builtin(name: &str) -> bool {
    matches!(
        name,
        "ReactNode" | "ReactElement" | "JSX.Element"
        | "CSSProperties" | "CSSObject"
        | "SyntheticEvent" | "MouseEvent" | "KeyboardEvent" | "ChangeEvent"
        | "MouseEventHandler" | "KeyboardEventHandler" | "ChangeEventHandler"
        | "FocusEventHandler" | "FormEventHandler" | "DragEventHandler"
        | "TouchEventHandler" | "WheelEventHandler" | "AnimationEventHandler"
        | "TransitionEventHandler" | "ClipboardEventHandler" | "CompositionEventHandler"
        | "FC" | "FunctionComponent" | "VFC" | "ComponentType"
        | "PropsWithChildren" | "PropsWithRef"
        | "RefObject" | "Ref" | "ForwardedRef" | "MutableRefObject"
        | "RefCallback" | "LegacyRef"
        | "Context" | "Consumer" | "Provider"
        | "ComponentPropsWithoutRef" | "ComponentPropsWithRef" | "ComponentProps"
        | "ElementRef" | "ElementType" | "ElementType"
        | "ReactPortal" | "ReactFragment" | "ReactChild"
    )
}

/// React 18 vs 19 behavioral differences for component detection.
pub struct ReactVersion {
    /// React 19: FC no longer implicitly includes children
    pub implicit_children: bool,
    /// React 19: ref is a plain prop, not via forwardRef
    pub ref_as_prop: bool,
}

pub const REACT_18: ReactVersion = ReactVersion { implicit_children: true, ref_as_prop: false };
pub const REACT_19: ReactVersion = ReactVersion { implicit_children: false, ref_as_prop: true };
```

## JSDoc Extraction — Keep Simple

```rust
/// Find JSDoc comment immediately preceding the given byte offset.
/// Returns empty string if none found.
fn find_jsdoc(comments: &[Comment], source: &str, span_start: u32) -> String {
    const PROXIMITY_THRESHOLD: u32 = 50; // bytes
    
    let comment = comments.iter()
        .rev()  // search backwards
        .find(|c| {
            c.is_block()
            && c.span.end <= span_start
            && span_start - c.span.end <= PROXIMITY_THRESHOLD
        })?;
    
    parse_jsdoc_text(&source[comment.span.start as usize..comment.span.end as usize])
}

fn parse_jsdoc_text(raw: &str) -> String {
    // Strip /** */ markers
    // Strip leading " * " per line
    // Stop at @tag lines
    // Handle triple-backtick code blocks (toggle skip)
    raw.trim_start_matches("/**")
       .trim_end_matches("*/")
       .lines()
       .map(|l| l.trim().trim_start_matches("* ").trim_start_matches('*'))
       .take_while(|l| !l.starts_with('@'))
       .collect::<Vec<_>>()
       .join("\n")
       .trim()
       .to_owned()
}
```

## Tests (fixtures)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_shadcn_button() {
        let source = fs::read_to_string("../../fixtures/shadcn/button.tsx").unwrap();
        let path = Utf8Path::new("/fixtures/shadcn/button.tsx");
        let data = parse_file(path, &source);
        
        assert!(data.component_mappings.iter().any(|m| m.component_name == "Button"));
        // Button should have props_type_name "ButtonProps"
        let btn = data.component_mappings.iter().find(|m| m.component_name == "Button").unwrap();
        assert_eq!(btn.props_type_name.as_str(), "ButtonProps");
    }

    #[test]
    fn test_react_aria_button_hoc() {
        let source = fs::read_to_string("../../fixtures/react-aria/Button.d.ts").unwrap();
        let path = Utf8Path::new("/fixtures/react-aria/Button.d.ts");
        let data = parse_file(path, &source);
        // createHideableComponent wrapping should be detected via HOC pattern
        assert!(!data.component_mappings.is_empty());
    }
}
```

## Benchmarks

```rust
// benches/extraction.rs
#[divan::bench(args = ["shadcn/button.tsx", "radix/button.d.ts", "mui/Button.d.ts"])]
fn parse_file_bench(bencher: divan::Bencher, fixture: &str) {
    let source = std::fs::read_to_string(format!("../../fixtures/{}", fixture)).unwrap();
    let path = camino::Utf8Path::new(fixture);
    bencher.bench(|| {
        oxc_react_docgen_core::extractor::parse_file(path, &source)
    });
}
```

## What NOT to Do

- Do not call `oxc_resolver` — that is the Pipeline agent's job
- Do not read other files — this function is pure
- Do not use `unwrap()` on parser results — handle parse errors gracefully
- Do not hardcode `"ButtonProps"` — extract from type annotations
- Do not confuse `.ts` and `.tsx` — component detection only runs for tsx
