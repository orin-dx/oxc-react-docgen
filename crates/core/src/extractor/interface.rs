//! Interface-adjacent collection: const enums, cva variants, displayName scanning.

use oxc_ast::ast::*;

use crate::types::{EnumEntry, EnumValue};

use super::SourceDataCollector;

impl<'src> SourceDataCollector<'src> {
    // ─── `const` enum (as-const objects) collection ──────────────────────────

    pub(super) fn try_collect_const_enum<'a>(&mut self, decl: &VariableDeclarator<'a>) {
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

    /// Detect: `const X = cva(base, { variants: { key: { val: "...", ... } } })`
    /// Store each variant key's values in self.data.enums under the scoped key.
    pub(super) fn try_collect_cva_call<'a>(&mut self, decl: &VariableDeclarator<'a>, name: &str) {
        let Some(init) = decl.init.as_ref() else { return };
        let call = match init {
            Expression::CallExpression(ce) => ce,
            _ => return,
        };

        // Check callee is a known variant function
        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str().to_owned(),
            Expression::StaticMemberExpression(m) => m.property.name.as_str().to_owned(),
            _ => return,
        };

        if !matches!(
            callee_name.as_str(),
            "cva" | "tv" | "defineRecipe" | "recipe" | "defineSlotRecipe"
        ) {
            return;
        }

        // Second argument should be an object with a "variants" key
        let Some(second_arg) = call.arguments.get(1) else { return };
        let second_expr = match second_arg {
            Argument::SpreadElement(_) => return,
            other => match other.as_expression() {
                Some(e) => e,
                None => return,
            },
        };
        let obj = match second_expr {
            Expression::ObjectExpression(o) => o,
            _ => return,
        };

        // Find the "variants" property
        let variants_value = obj.properties.iter().find_map(|prop| {
            if let ObjectPropertyKind::ObjectProperty(p) = prop {
                if let PropertyKey::StaticIdentifier(key) = &p.key {
                    if key.name.as_str() == "variants" {
                        return match &p.value {
                            Expression::ObjectExpression(o) => Some(o),
                            _ => None,
                        };
                    }
                }
            }
            None
        });
        let Some(variants_value) = variants_value else { return };

        let scoped_key = self.scoped_key(name);
        let mut entries: Vec<EnumEntry> = Vec::new();

        for prop in &variants_value.properties {
            if let ObjectPropertyKind::ObjectProperty(variant_prop) = prop {
                let variant_key = match &variant_prop.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str().to_owned(),
                    PropertyKey::StringLiteral(s) => s.value.as_str().to_owned(),
                    _ => continue,
                };

                // The value is another object: { default: "...", sm: "...", ... }
                let values_obj = match &variant_prop.value {
                    Expression::ObjectExpression(o) => o,
                    _ => continue,
                };

                for value_prop in &values_obj.properties {
                    if let ObjectPropertyKind::ObjectProperty(vp) = value_prop {
                        let value_name = match &vp.key {
                            PropertyKey::StaticIdentifier(id) => id.name.as_str().to_owned(),
                            PropertyKey::StringLiteral(s) => s.value.as_str().to_owned(),
                            _ => continue,
                        };

                        entries.push(EnumEntry {
                            name: variant_key.clone(),
                            value: EnumValue::String(value_name),
                            description: String::new(),
                        });
                    }
                }
            }
        }

        if !entries.is_empty() {
            self.data.enums.insert(scoped_key, entries);
        }
    }

    pub(super) fn expression_to_enum_value<'a>(&self, expr: &Expression<'a>) -> Option<EnumValue> {
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
    pub(super) fn try_scan_display_name<'a>(&mut self, stmt: &ExpressionStatement<'a>) {
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
}
