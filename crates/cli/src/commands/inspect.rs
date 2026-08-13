use miette::Result;

use crate::config::{build_options, BuildOptionsArgs};

pub fn cmd_inspect(args: crate::InspectArgs, config_path: Option<&str>) -> Result<i32> {
    use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};
    use owo_colors::OwoColorize;

    let options = build_options(BuildOptionsArgs {
        src: &args.src,
        no_cross_package: false,
        react_version: None,
        cache_dir: None,
        html_attributes: None,
        config_path,
        extra_builtins: &[],
    })?;
    let output = oxc_react_docgen_core::pipeline::extract(&options);

    let component = output.components.get(&args.component).ok_or_else(|| {
        miette::miette!(
            "Component '{}' not found.\nAvailable: {}",
            args.component,
            output.components.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    })?;

    println!();
    println!("  {}  {}", component.display_name.bold(), component.file_path.to_string().dimmed());
    println!("  {}", "─".repeat(70).dimmed());

    if !component.description.is_empty() {
        println!();
        println!("  {}", component.description);
    }

    println!();
    println!("  {} ({})", "Props".bold(), component.props.len());
    println!();

    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("Prop").add_attribute(Attribute::Bold),
        Cell::new("Type").add_attribute(Attribute::Bold),
        Cell::new("Req").add_attribute(Attribute::Bold),
        Cell::new("Default").add_attribute(Attribute::Bold),
        Cell::new("From").add_attribute(Attribute::Bold),
    ]);

    for prop in component.props.values() {
        let type_str = prop.prop_type.raw_string();
        let req_str = if prop.required { "✓".to_string() } else { "–".to_string() };
        let default_str = prop.default_value.as_ref().map(|d| d.value.clone()).unwrap_or_else(|| "–".into());
        let from_str = prop.parent.as_ref().map(|p| p.name.clone()).unwrap_or_default();

        table.add_row(vec![
            Cell::new(&prop.name).fg(Color::White),
            Cell::new(&type_str).fg(Color::Cyan),
            Cell::new(&req_str),
            Cell::new(&default_str).fg(Color::DarkGrey),
            Cell::new(&from_str).fg(Color::DarkGrey),
        ]);
    }

    for line in table.to_string().lines() {
        println!("  {line}");
    }

    if !component.notable_inherited.is_empty() {
        println!();
        for layer in &component.inheritance {
            let element_note = layer.html_element.as_ref().map(|e| format!(" (<{e}>)")).unwrap_or_default();
            println!("  {} {}{}", "↳".dimmed(), layer.type_name.dimmed(), element_note.dimmed());
        }

        let notable_names: Vec<&str> = component.notable_inherited.keys().map(|s| s.as_str()).collect();
        println!("    Notable: {}", notable_names.join("  ").dimmed());
    }

    println!();
    Ok(output.exit_code(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_surfaces_error_exit_code_from_diagnostics_elsewhere_in_the_tree() {
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = tempfile::TempDir::new_in(manifest_dir).unwrap();
        std::fs::write(
            tmp.path().join("Comp.tsx"),
            r#"
export interface CompProps { label: string; }
export function Comp(props: CompProps) { return null; }
"#,
        )
        .unwrap();
        // Deliberately malformed, same fixture shape as
        // extractor::tests::test_parse_error_surfaced_as_diagnostic — unclosed
        // interface body triggers a ParseError diagnostic (Error severity).
        std::fs::write(
            tmp.path().join("Bad.tsx"),
            r#"
export interface BrokenProps {
    label: string;
"#,
        )
        .unwrap();

        let dir = camino::Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let args = crate::InspectArgs { component: "Comp".into(), src: vec![dir.to_string()] };
        let code = cmd_inspect(args, None).expect("cmd_inspect should find Comp and not error");
        assert_eq!(
            code, 2,
            "expected exit code 2: Bad.tsx has a parse error even though the inspected component is fine"
        );
    }

    // ── SPEC-CLI-001a AC-011: inspect COMPONENT where COMPONENT is present
    // and nothing in the scanned tree has an Error-severity diagnostic
    // returns exit code 0.

    #[test]
    fn clean_run_returns_exit_code_zero() {
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = tempfile::TempDir::new_in(manifest_dir).unwrap();
        std::fs::write(
            tmp.path().join("Comp.tsx"),
            "export interface CompProps { label: string; }\nexport function Comp(props: CompProps) { return null; }\n",
        )
        .unwrap();
        let dir = camino::Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let args = crate::InspectArgs { component: "Comp".into(), src: vec![dir.to_string()] };
        let code = cmd_inspect(args, None).expect("cmd_inspect should find Comp and not error");
        assert_eq!(code, 0, "expected exit code 0 for a clean run");
    }

    // ── SPEC-CLI-001a AC-013: inspect COMPONENT where COMPONENT is not among
    // the extracted components' names returns Err (mapped to exit code 1 by
    // main()'s shared Err-propagation path — see AC-017/AC-018), naming the
    // requested component and listing the available ones.

    #[test]
    fn missing_component_errors_naming_the_request_and_available_components() {
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = tempfile::TempDir::new_in(manifest_dir).unwrap();
        std::fs::write(
            tmp.path().join("Comp.tsx"),
            "export interface CompProps { label: string; }\nexport function Comp(props: CompProps) { return null; }\n",
        )
        .unwrap();
        let dir = camino::Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let args = crate::InspectArgs { component: "NoSuchComponent".into(), src: vec![dir.to_string()] };
        let err = cmd_inspect(args, None).expect_err("expected an Err for a component that doesn't exist");
        let message = format!("{err:?}");
        assert!(
            message.contains("NoSuchComponent"),
            "expected the error to name the requested component, got {message}"
        );
        assert!(message.contains("Comp"), "expected the error to list the available components, got {message}");
    }
}
