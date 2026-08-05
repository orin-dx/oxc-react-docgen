use miette::Result;

/// Build the JSON Schema for the oxc-react-docgen `ExtractionOutput` format.
/// Hand-maintained per ADR-0002-style precedent — see the drift-detection
/// test below for the guard against it silently falling out of sync with the
/// real `ComponentEntry`/`ExtractionStats`/`Diagnostic` structs.
fn schema_value() -> serde_json::Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "ExtractionOutput",
        "description": "Output schema for oxc-react-docgen component prop extraction",
        "type": "object",
        "required": ["components", "enums", "diagnostics", "stats"],
        "properties": {
            "components": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "required": ["displayName", "filePath", "props"],
                    "properties": {
                        "displayName": { "type": "string" },
                        "filePath": { "type": "string" },
                        "description": { "type": ["string", "null"] },
                        "props": {
                            "type": "object",
                            "additionalProperties": {
                                "type": "object",
                                "required": ["name", "required", "type"],
                                "properties": {
                                    "name": { "type": "string" },
                                    "required": { "type": "boolean" },
                                    "type": { "type": "object" },
                                    "description": { "type": ["string", "null"] },
                                    "defaultValue": { "type": ["object", "null"] },
                                    "tags": { "type": "object" },
                                    "parent": { "type": ["object", "null"] },
                                    "declarations": { "type": "array" }
                                }
                            }
                        },
                        "inheritance": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "typeName": { "type": "string" },
                                    "fileName": { "type": "string" },
                                    "omitted": { "type": "array", "items": { "type": "string" } },
                                    "htmlElement": { "type": ["string", "null"] },
                                    "totalProps": { "type": "integer" }
                                }
                            }
                        },
                        "notableInherited": { "type": "object" },
                        "discriminantProp": { "type": ["string", "null"] },
                        "composes": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "tags": { "type": "object" },
                        "methods": { "type": "array" }
                    }
                }
            },
            "enums": { "type": "object" },
            "diagnostics": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["severity", "message", "code"],
                    "properties": {
                        "severity": { "type": "string", "enum": ["error", "warning", "info"] },
                        "message": { "type": "string" },
                        "file": { "type": ["string", "null"] },
                        "line": { "type": ["integer", "null"] },
                        "column": { "type": ["integer", "null"] },
                        "help": { "type": ["string", "null"] },
                        "code": { "type": "string" }
                    }
                }
            },
            "stats": {
                "type": "object",
                "required": ["componentsExtracted", "filesParsed", "durationMs"],
                "properties": {
                    "componentsExtracted": { "type": "integer" },
                    "componentsSkipped": { "type": "integer" },
                    "filesParsed": { "type": "integer" },
                    "dtsFilesParsed": { "type": "integer" },
                    "dtsCacheHits": { "type": "integer" },
                    "durationMs": { "type": "integer" },
                    "tier1Count": { "type": "integer" },
                    "tier3Count": { "type": "integer" },
                    "opaqueCount": { "type": "integer" }
                }
            }
        }
    })
}

/// Output JSON schema for the oxc-react-docgen ExtractionOutput format.
pub fn cmd_schema() -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&schema_value()).unwrap_or_default());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmd_schema_valid_json() {
        assert!(cmd_schema().is_ok());
    }

    #[test]
    fn schema_covers_every_field_name_the_real_output_serializes() {
        use oxc_react_docgen_core::types::{
            ComponentEntry, DefaultValue, Diagnostic, DiagnosticCode, DiagnosticSeverity, ExtractionOutput,
            ExtractionStats, InheritedLayer, ParsedProp, PropParent, PropType,
        };
        use std::collections::BTreeMap;

        let mut props = BTreeMap::new();
        props.insert(
            "variant".to_string(),
            ParsedProp {
                name: "variant".into(),
                prop_type: PropType::String,
                required: true,
                default_value: Some(DefaultValue { value: "\"a\"".into(), computed: false }),
                description: "desc".into(),
                tags: BTreeMap::new(),
                parent: Some(PropParent { name: "ButtonProps".into(), file_name: "Button.tsx".into() }),
                declarations: vec![],
            },
        );

        let mut components = BTreeMap::new();
        components.insert(
            "Button".to_string(),
            ComponentEntry {
                display_name: "Button".into(),
                file_path: "src/Button.tsx".into(),
                description: "A button".into(),
                props,
                inheritance: vec![InheritedLayer {
                    type_name: "ButtonHTMLAttributes".into(),
                    file_name: "react.d.ts".into(),
                    omitted: vec![],
                    html_element: Some("button".into()),
                    total_props: 3,
                }],
                notable_inherited: BTreeMap::new(),
                discriminant_prop: Some("variant".into()),
                composes: vec!["SomeUnresolved".into()],
                tags: BTreeMap::from([("deprecated".to_string(), String::new())]),
                methods: vec![],
            },
        );

        let output = ExtractionOutput {
            components,
            enums: BTreeMap::new(),
            diagnostics: vec![Diagnostic {
                severity: DiagnosticSeverity::Warning,
                message: "msg".into(),
                file: Some("Button.tsx".into()),
                line: Some(10),
                column: Some(4),
                help: Some("try this".into()),
                code: DiagnosticCode::OpaqueType,
            }],
            stats: ExtractionStats {
                components_extracted: 1,
                components_skipped: 1,
                files_parsed: 1,
                dts_files_parsed: 1,
                dts_cache_hits: 0,
                duration_ms: 5,
                tier1_count: 1,
                tier3_count: 1,
                opaque_count: 1,
            },
        };

        let value = serde_json::to_value(&output).expect("ExtractionOutput must serialize");
        let mut real_fields: Vec<String> = Vec::new();
        for key in ["components", "diagnostics", "stats"] {
            assert!(value.get(key).is_some(), "fixture is missing top-level key {key}");
        }
        if let Some(obj) = value["components"]["Button"].as_object() {
            real_fields.extend(obj.keys().cloned());
        }
        if let Some(obj) = value["components"]["Button"]["props"]["variant"].as_object() {
            real_fields.extend(obj.keys().cloned());
        }
        if let Some(obj) = value["components"]["Button"]["inheritance"][0].as_object() {
            real_fields.extend(obj.keys().cloned());
        }
        if let Some(obj) = value["diagnostics"][0].as_object() {
            real_fields.extend(obj.keys().cloned());
        }
        if let Some(obj) = value["stats"].as_object() {
            real_fields.extend(obj.keys().cloned());
        }

        let schema_str = serde_json::to_string(&schema_value()).expect("schema must serialize");
        let missing: Vec<&String> = real_fields.iter().filter(|f| !schema_str.contains(f.as_str())).collect();
        assert!(missing.is_empty(), "schema.rs is missing field(s) present in real serialized output: {missing:?}");
    }
}
