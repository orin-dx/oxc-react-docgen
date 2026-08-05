use miette::Result;

/// Output JSON schema for the oxc-react-docgen ExtractionOutput format.
pub fn cmd_schema() -> Result<()> {
    let schema = serde_json::json!({
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
                                    "description": { "type": ["string", "null"] }
                                }
                            }
                        },
                        "inheritance": { "type": "array" },
                        "notableInherited": { "type": "object" },
                        "discriminantProp": { "type": ["string", "null"] },
                        "composes": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
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
                        "code": { "type": "string" }
                    }
                }
            },
            "stats": {
                "type": "object",
                "required": ["componentsExtracted", "filesParsed", "durationMs"],
                "properties": {
                    "componentsExtracted": { "type": "integer" },
                    "filesParsed": { "type": "integer" },
                    "dtsCacheHits": { "type": "integer" },
                    "durationMs": { "type": "integer" }
                }
            }
        }
    });

    println!("{}", serde_json::to_string_pretty(&schema).unwrap_or_default());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmd_schema_valid_json() {
        assert!(cmd_schema().is_ok());
    }
}
