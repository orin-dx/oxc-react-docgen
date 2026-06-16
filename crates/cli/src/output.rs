pub fn print_summary(output: &oxc_react_docgen_core::types::ExtractionOutput, quiet: bool) {
    if quiet {
        return;
    }
    use owo_colors::OwoColorize;

    let errors = output
        .diagnostics
        .iter()
        .filter(|d| matches!(d.severity, oxc_react_docgen_core::types::DiagnosticSeverity::Error))
        .count();
    let warnings = output
        .diagnostics
        .iter()
        .filter(|d| matches!(d.severity, oxc_react_docgen_core::types::DiagnosticSeverity::Warning))
        .count();

    println!();
    println!(
        "  {}  {} components  ·  {} enums  ·  {}  ·  {}  ·  {}ms",
        "⚡".yellow(),
        output.stats.components_extracted.to_string().bold(),
        output.enums.len().to_string().bold(),
        if warnings > 0 {
            format!("{warnings} warnings").yellow().to_string()
        } else {
            format!("{warnings} warnings")
        },
        if errors > 0 {
            format!("{errors} errors").red().to_string()
        } else {
            format!("{errors} errors")
        },
        output.stats.duration_ms.to_string().bold(),
    );
    println!();
}

pub fn print_diagnostics(diagnostics: &[oxc_react_docgen_core::types::Diagnostic]) {
    use owo_colors::OwoColorize;
    for d in diagnostics {
        let prefix = match d.severity {
            oxc_react_docgen_core::types::DiagnosticSeverity::Error => "error".red().to_string(),
            oxc_react_docgen_core::types::DiagnosticSeverity::Warning => {
                "warn".yellow().to_string()
            }
            oxc_react_docgen_core::types::DiagnosticSeverity::Info => "info".dimmed().to_string(),
            _ => "info".dimmed().to_string(),
        };
        if let Some(ref file) = d.file {
            println!("  [{prefix}] {file}:{}", d.message);
        } else {
            println!("  [{prefix}] {}", d.message);
        }
        if let Some(ref help) = d.help {
            println!("    {} {}", "help:".dimmed(), help);
        }
    }
}
