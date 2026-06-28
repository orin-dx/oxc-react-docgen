//! `impl Visit for SourceDataCollector` — the AST walker entry points.

use oxc_ast::ast::*;
use oxc_ast_visit::{walk, Visit};
use oxc_syntax::scope::ScopeFlags;

use crate::types::{
    CollectedInterface, ComponentMapping, EnumEntry, EnumValue, ExtendsRef, ImportBinding, LexedExport, RawProp,
};

use super::{declaration_name, is_pascal_case, SourceDataCollector};

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
                    is_type_only: node.export_kind.is_type() || spec.export_kind.is_type(),
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
            self.data
                .exports
                .push(LexedExport::ReExportAll { source_specifier: src, is_type_only: node.export_kind.is_type() });
        }
    }

    fn visit_ts_interface_declaration(&mut self, node: &TSInterfaceDeclaration<'a>) {
        let name = node.id.name.as_str();
        let key = self.scoped_key(name);

        let extends: Vec<ExtendsRef> = node.extends.iter().map(|ext| self.collect_extends(ext)).collect();

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
                    Some(init) => {
                        self.expression_to_enum_value(init).unwrap_or_else(|| EnumValue::String(name.clone()))
                    }
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
            // Collect cva() / tv() variant definitions for all file types (.ts and .tsx)
            if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                let name = id.name.as_str().to_owned();
                self.try_collect_cva_call(declarator, &name);
            }
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
                                let param_defaults = self.extract_param_defaults(&func.params);
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
