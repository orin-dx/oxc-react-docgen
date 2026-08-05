use miette::{IntoDiagnostic, Result, WrapErr};

use crate::config::{build_options, BuildOptionsArgs};
use crate::output::{print_diagnostics, print_summary};

/// Returns the process exit code (0 = success, 2 = extraction reported an
/// error-severity diagnostic) rather than calling `std::process::exit`
/// directly — keeps the exit decision testable and lets `main()` be the only
/// place that actually terminates the process.
pub fn cmd_extract(args: crate::ExtractArgs, quiet: bool, config_path: Option<&str>) -> Result<i32> {
    use indicatif::{ProgressBar, ProgressStyle};

    let html_attributes = args.html_attributes.map(|m| match m {
        crate::HtmlAttributeModeArg::Curated => "curated",
        crate::HtmlAttributeModeArg::Full => "full",
        crate::HtmlAttributeModeArg::None => "none",
    });
    let options = build_options(BuildOptionsArgs {
        src: &args.src,
        no_cross_package: args.no_cross_package,
        react_version: args.react_version.as_deref(),
        cache_dir: args.cache_dir.as_deref(),
        html_attributes,
        config_path,
        extra_builtins: &args.extra_builtins,
    })?;

    let pb = if !quiet && !args.json {
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

    // --json ignores --out/--format — see ExtractArgs::json's doc comment.
    if args.json {
        println!("{}", serde_json::to_string(&output).into_diagnostic()?);
    } else {
        if !quiet {
            print_summary(&output, quiet);
            print_diagnostics(&output.diagnostics);
        }

        let json = match args.format {
            crate::OutputFormat::Canonical => serde_json::to_string_pretty(&output).into_diagnostic()?,
            crate::OutputFormat::Rdt => serialize_rdt(&output),
            crate::OutputFormat::Storybook => serialize_storybook(&output),
            crate::OutputFormat::Toon => oxc_react_docgen_core::toon::render_output_toon(&output),
        };

        match args.out {
            Some(ref path) => std::fs::write(path, &json).into_diagnostic().wrap_err(format!("Writing to {path}"))?,
            None => println!("{json}"),
        }
    }

    // Must run regardless of --json — this is the one thing CI actually
    // depends on the exit code for.
    Ok(output.exit_code(false))
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

/// `methods` is always `[]` — this tool doesn't extract class methods, and RDT
/// consumers (Storybook's docgen addon) only ever read it for class components.
pub fn serialize_rdt(output: &oxc_react_docgen_core::types::ExtractionOutput) -> String {
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
                "composes": entry.composes,
            }),
        );
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap_or_default()
}

pub fn serialize_storybook(output: &oxc_react_docgen_core::types::ExtractionOutput) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args_for(src: &str, json: bool) -> crate::ExtractArgs {
        crate::ExtractArgs {
            src: vec![src.to_owned()],
            out: None,
            format: crate::OutputFormat::Canonical,
            no_cross_package: false,
            react_version: None,
            cache_dir: None,
            html_attributes: None,
            extra_builtins: vec![],
            json,
        }
    }

    #[test]
    fn json_mode_still_returns_the_error_exit_code() {
        // Adversarial review finding: --json returned before the error-
        // diagnostic exit-code check, so `extract --json` always exited 0
        // even on hard errors — the one signal CI actually depends on.
        let code = cmd_extract(args_for("/nonexistent/does-not-exist", true), true, None)
            .expect("cmd_extract itself should not error");
        assert_eq!(code, 2, "expected exit code 2 for a nonexistent src dir even in --json mode");
    }

    #[test]
    fn non_json_mode_returns_the_same_error_exit_code() {
        // Same check, non-json path — both must agree.
        let code = cmd_extract(args_for("/nonexistent/does-not-exist", false), true, None)
            .expect("cmd_extract itself should not error");
        assert_eq!(code, 2);
    }

    // ── rdt_output_includes_composes ──────────────────────────────────────────
    //
    // `ComponentEntry.composes` (react-docgen's own "props come from this type,
    // listed by name instead of flattened" field) was populated by the resolver
    // but silently dropped by serialize_rdt — this is the RDT-format half of
    // that fix; the resolver half is
    // pipeline::tests::test_unresolvable_intersection_member_records_raw_type_in_composes.

    #[test]
    fn rdt_output_includes_composes() {
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = tempfile::TempDir::new_in(manifest_dir).unwrap();
        std::fs::write(
            tmp.path().join("Comp.tsx"),
            r#"
export type Weird<T> = T extends string ? { a: string } : { b: number };
export type CompProps = Weird<'x'> & { c: boolean };
export function Comp(props: CompProps) { return null; }
"#,
        )
        .unwrap();

        let dir = camino::Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let options = oxc_react_docgen_core::pipeline::PipelineOptions {
            src_dirs: vec![dir],
            cache_dir: Some(camino::Utf8PathBuf::from_path_buf(tmp.path().join("cache")).unwrap()),
            ..Default::default()
        };
        let output = oxc_react_docgen_core::pipeline::extract(&options);

        let rdt_json = serialize_rdt(&output);
        let parsed: serde_json::Value = serde_json::from_str(&rdt_json).unwrap();
        let composes = parsed["Comp"]["composes"].as_array().expect("expected a composes array in RDT output");
        assert!(!composes.is_empty(), "expected the unresolvable Weird<'x'> member to appear in RDT's composes field");
    }
}
