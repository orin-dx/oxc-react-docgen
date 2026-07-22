use miette::{IntoDiagnostic, Result};

use crate::config::{build_options, BuildOptionsArgs};
use crate::output::{print_diagnostics, print_summary};

/// Returns the process exit code (0 = clean, 1 = --strict and warnings found,
/// 2 = errors found) rather than calling `std::process::exit` directly — see
/// `cmd_extract`'s doc comment for why.
pub fn cmd_check(args: crate::CheckArgs, quiet: bool, config_path: Option<&str>) -> Result<i32> {
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

    if args.json {
        println!("{}", serde_json::to_string(&output.diagnostics).into_diagnostic()?);
    } else if !quiet {
        print_summary(&output, quiet);
        print_diagnostics(&output.diagnostics);
    }

    if !errors.is_empty() {
        return Ok(2);
    }
    if args.strict && !warnings.is_empty() {
        return Ok(1);
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_mode_still_returns_the_error_exit_code() {
        let args = crate::CheckArgs { src: vec!["/nonexistent/does-not-exist".into()], strict: false, json: true };
        let code = cmd_check(args, true, None).expect("cmd_check itself should not error");
        assert_eq!(code, 2, "expected exit code 2 for a nonexistent src dir even in --json mode");
    }
}
