use miette::Result;

use crate::config::build_options;

pub fn cmd_inspect(args: crate::InspectArgs, config_path: Option<&str>) -> Result<()> {
    use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};
    use owo_colors::OwoColorize;

    let options = build_options(&args.src, false, None, None, config_path)?;
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
    Ok(())
}
