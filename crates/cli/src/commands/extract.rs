use miette::{IntoDiagnostic, Result, WrapErr};

use crate::config::build_options;
use crate::output::{print_diagnostics, print_summary};

pub fn cmd_extract(args: crate::ExtractArgs, json_mode: bool, quiet: bool, config_path: Option<&str>) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};

    let options = build_options(
        &args.src,
        args.no_cross_package,
        args.react_version.as_deref(),
        args.cache_dir.as_deref(),
        config_path,
    );

    let pb = if !quiet && !json_mode {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner().template("{spinner:.cyan} {msg}").into_diagnostic()?);
        pb.set_message("Extracting...");
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        Some(pb)
    } else {
        None
    };

    let output = oxc_react_docgen_core::pipeline::extract(&options);

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    if json_mode {
        println!("{}", serde_json::to_string(&output).into_diagnostic()?);
        return Ok(());
    }

    if !quiet {
        print_summary(&output, quiet);
        print_diagnostics(&output.diagnostics);
    }

    let json = match args.format {
        crate::OutputFormat::Canonical => serde_json::to_string_pretty(&output).into_diagnostic()?,
        crate::OutputFormat::Rdt => serialize_rdt(&output),
        crate::OutputFormat::Storybook => serialize_storybook(&output),
    };

    match args.out {
        Some(ref path) => std::fs::write(path, &json).into_diagnostic().wrap_err(format!("Writing to {path}"))?,
        None => println!("{json}"),
    }

    if output.diagnostics.iter().any(|d| matches!(d.severity, oxc_react_docgen_core::types::DiagnosticSeverity::Error))
    {
        std::process::exit(2);
    }

    Ok(())
}

/// RDT's type-name convention for literal unions: `{"name": "enum", "value": [...]}`
/// instead of inlining the literal text into `type.name` as a plain string. Lets
/// prop-table renderers that pattern-match `type.name === "enum"` (a common
/// Storybook addon integration point) show a `<select>` control for the most
/// common, most curated props in any design system (variant, size, color…).
fn rdt_type_json(prop_type: &oxc_react_docgen_core::types::PropType) -> serde_json::Value {
    use oxc_react_docgen_core::types::PropType;

    if prop_type.is_literal_union() {
        let values: Vec<serde_json::Value> = match prop_type {
            PropType::Union(members) => members.iter().map(|m| serde_json::json!({"value": m.raw_string()})).collect(),
            PropType::LiteralUnion { members, .. } => {
                members.iter().map(|m| serde_json::json!({"value": format!("\"{m}\"")})).collect()
            }
            _ => vec![],
        };
        return serde_json::json!({ "name": "enum", "value": values });
    }
    serde_json::json!({ "name": prop_type.raw_string() })
}

pub fn serialize_rdt(output: &oxc_react_docgen_core::types::ExtractionOutput) -> String {
    // RDT-compatible shape: flatten to Record<string, ComponentDoc>
    let mut map = serde_json::Map::new();
    for (name, entry) in &output.components {
        let props: serde_json::Map<String, serde_json::Value> = entry
            .props
            .iter()
            .map(|(k, prop)| {
                let obj = serde_json::json!({
                    "name": prop.name,
                    "type": rdt_type_json(&prop.prop_type),
                    "required": prop.required,
                    "defaultValue": prop.default_value.as_ref().map(|d| serde_json::json!({"value": d.value, "computed": d.computed})),
                    "description": prop.description,
                    "parent": prop.parent.as_ref().map(|p| serde_json::json!({"name": p.name, "fileName": p.file_name})),
                });
                (k.clone(), obj)
            })
            .collect();
        map.insert(
            name.clone(),
            serde_json::json!({
                "displayName": entry.display_name,
                "props": serde_json::Value::Object(props),
                "description": entry.description,
                "methods": [],
                "tags": entry.tags,
            }),
        );
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap_or_default()
}

pub fn serialize_storybook(output: &oxc_react_docgen_core::types::ExtractionOutput) -> String {
    // Storybook __docgenInfo shape
    let mut map = serde_json::Map::new();
    for (name, entry) in &output.components {
        let props: serde_json::Map<String, serde_json::Value> = entry
            .props
            .iter()
            .map(|(k, prop)| {
                let obj = serde_json::json!({
                    "name": prop.name,
                    "type": { "name": prop.prop_type.raw_string() },
                    "required": prop.required,
                    "defaultValue": prop.default_value.as_ref().map(|d| serde_json::json!({"value": d.value})),
                    "description": prop.description,
                });
                (k.clone(), obj)
            })
            .collect();
        map.insert(
            name.clone(),
            serde_json::json!({
                "displayName": entry.display_name,
                "props": serde_json::Value::Object(props),
                "description": entry.description,
                "methods": [],
            }),
        );
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap_or_default()
}
