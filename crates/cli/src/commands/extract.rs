use miette::{IntoDiagnostic, Result, WrapErr};

use crate::config::{build_options, BuildOptionsArgs};
use crate::output::{print_diagnostics, print_summary, write_atomic};

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
            Some(ref path) => write_atomic(path, &json).into_diagnostic().wrap_err(format!("Writing to {path}"))?,
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

    // ── SPEC-SERIALIZATION-001 AC-1/AC-2/AC-3/AC-4: rdt_type_json had zero
    // direct tests — it's private and was only ever exercised indirectly.

    use oxc_react_docgen_core::types::PropType;

    #[test]
    fn rdt_type_json_zero_and_one_member_unions_never_produce_enum_shape() {
        assert_eq!(rdt_type_json(&PropType::Union(vec![])), serde_json::json!({"name": ""}));
        assert_eq!(
            rdt_type_json(&PropType::Union(vec![PropType::StringLiteral("x".into())])),
            serde_json::json!({"name": "\"x\""})
        );
        assert_eq!(
            rdt_type_json(&PropType::LiteralUnion { members: vec![], has_default: false }),
            serde_json::json!({"name": ""})
        );
        assert_eq!(
            rdt_type_json(&PropType::LiteralUnion { members: vec!["only".into()], has_default: false }),
            serde_json::json!({"name": "\"only\""})
        );
    }

    #[test]
    fn rdt_type_json_two_plus_member_literal_unions_produce_exact_enum_shape() {
        let union = PropType::Union(vec![PropType::StringLiteral("red".into()), PropType::NumberLiteral(5.0)]);
        assert_eq!(
            rdt_type_json(&union),
            serde_json::json!({"name": "enum", "value": [{"value": "\"red\""}, {"value": "5"}]})
        );

        let literal_union = PropType::LiteralUnion { members: vec!["red".into(), "blue".into()], has_default: false };
        assert_eq!(
            rdt_type_json(&literal_union),
            serde_json::json!({"name": "enum", "value": [{"value": "\"red\""}, {"value": "\"blue\""}]})
        );
    }

    #[test]
    fn rdt_type_json_enum_shape_matches_is_literal_union_exactly() {
        let fixtures: Vec<PropType> = vec![
            PropType::Union(vec![]),
            PropType::Union(vec![PropType::StringLiteral("a".into())]),
            PropType::Union(vec![PropType::StringLiteral("a".into()), PropType::StringLiteral("b".into())]),
            PropType::Union(vec![
                PropType::StringLiteral("a".into()),
                PropType::StringLiteral("b".into()),
                PropType::StringLiteral("c".into()),
            ]),
            PropType::Union(vec![
                PropType::StringLiteral("a".into()),
                PropType::Named { name: "Foo".into(), args: vec![] },
            ]),
            PropType::LiteralUnion { members: vec![], has_default: false },
            PropType::LiteralUnion { members: vec!["a".into()], has_default: false },
            PropType::LiteralUnion { members: vec!["a".into(), "b".into()], has_default: false },
        ];
        for fixture in fixtures {
            let is_enum_shape = rdt_type_json(&fixture)["name"] == "enum";
            assert_eq!(
                is_enum_shape,
                fixture.is_literal_union(),
                "rdt_type_json's enum-shape decision must exactly match is_literal_union() for {fixture:?}"
            );
        }
    }

    #[test]
    fn rdt_type_json_union_with_one_non_literal_member_never_produces_enum_shape() {
        let union = PropType::Union(vec![
            PropType::StringLiteral("a".into()),
            PropType::StringLiteral("b".into()),
            PropType::Named { name: "Foo".into(), args: vec![] },
        ]);
        let json = rdt_type_json(&union);
        assert_ne!(
            json["name"], "enum",
            "a union with a non-literal member must not produce the enum shape, got {json}"
        );
    }

    // ── SPEC-SERIALIZATION-001 AC-8: serialize_rdt emits composes verbatim —
    // strengthened from a non-empty check to an exact match.

    #[test]
    fn serialize_rdt_emits_composes_exactly() {
        use oxc_react_docgen_core::types::ComponentEntry;
        let entry = ComponentEntry {
            display_name: "Widget".to_string(),
            file_path: "Widget.tsx".into(),
            description: String::new(),
            props: Default::default(),
            inheritance: vec![],
            notable_inherited: Default::default(),
            discriminant_prop: None,
            composes: vec!["BaseA".to_string(), "BaseB".to_string()],
            tags: Default::default(),
            methods: vec![],
        };
        let mut components = std::collections::BTreeMap::new();
        components.insert("Widget".to_string(), entry);
        let output = oxc_react_docgen_core::types::ExtractionOutput {
            components,
            enums: Default::default(),
            diagnostics: vec![],
            stats: Default::default(),
        };

        let rdt_json = serialize_rdt(&output);
        let parsed: serde_json::Value = serde_json::from_str(&rdt_json).unwrap();
        assert_eq!(parsed["Widget"]["composes"], serde_json::json!(["BaseA", "BaseB"]));
    }

    // ── SPEC-SERIALIZATION-001 AC-10: end-to-end — a props interface
    // extending a string-literal-union type alias routes through
    // ResolvedChain::give_up (resolver/alias.rs), producing a composes value
    // with each member quote-wrapped by to_raw_string(), and that exact
    // quoted value survives into serialize_rdt's output.

    #[test]
    fn literal_union_extends_base_produces_quoted_composes_end_to_end() {
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = tempfile::TempDir::new_in(manifest_dir).unwrap();
        std::fs::write(
            tmp.path().join("Variant.tsx"),
            r#"
type Variant = 'a' | 'b';
interface Props extends Variant {}
export function Comp(props: Props) { return null; }
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

        let comp = output.components.get("Comp").expect("expected a Comp component");
        assert_eq!(
            comp.composes,
            vec!["\"a\" | \"b\"".to_string()],
            "expected quote-wrapped members in composes, got {:?}",
            comp.composes
        );

        let rdt_json = serialize_rdt(&output);
        let parsed: serde_json::Value = serde_json::from_str(&rdt_json).unwrap();
        assert_eq!(parsed["Comp"]["composes"], serde_json::json!(["\"a\" | \"b\""]));
    }
}
