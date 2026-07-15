use miette::Result;

use crate::config::build_options;
use crate::output::{print_diagnostics, print_summary};

pub fn cmd_check(args: crate::CheckArgs, quiet: bool, config_path: Option<&str>) -> Result<()> {
    let options = build_options(&args.src, false, None, None, None, config_path)?;
    let output = oxc_react_docgen_core::pipeline::extract(&options);

    let errors: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|d| matches!(d.severity, oxc_react_docgen_core::types::DiagnosticSeverity::Error))
        .collect();
    let warnings: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|d| matches!(d.severity, oxc_react_docgen_core::types::DiagnosticSeverity::Warning))
        .collect();

    if !quiet {
        print_summary(&output, quiet);
        print_diagnostics(&output.diagnostics);
    }

    if !errors.is_empty() {
        std::process::exit(2);
    }
    if args.strict && !warnings.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}
