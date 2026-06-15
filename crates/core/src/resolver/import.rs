//! Import and canonical resolution.

use camino::{Utf8Path, Utf8PathBuf};

use crate::types::*;

use super::{ResolutionContext};

/// Resolve `name` to its canonical `(file_path, name)` pair.
/// Returns `None` if `name` is a local declaration (not imported).
pub(super) fn resolve_to_canonical(
    name: &str,
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(Utf8PathBuf, String)> {
    let import_ref = ctx.import_map.find_import(consuming_file, name)?;

    // Resolve the specifier to an absolute file path.
    let resolved_file =
        resolve_import_specifier(&import_ref.specifier, consuming_file, ctx, diagnostics)?;

    let canonical_name = import_ref.exported_name.to_string();
    Some((resolved_file, canonical_name))
}

/// Use `oxc_resolver` to turn an import specifier into an absolute file path.
pub(super) fn resolve_import_specifier(
    specifier: &str,
    from_file: &Utf8Path,
    ctx: &ResolutionContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Utf8PathBuf> {
    let from_dir = from_file.parent()?;
    match ctx.oxc_resolver.resolve(from_dir.as_std_path(), specifier) {
        Ok(resolved) => Utf8PathBuf::from_path_buf(resolved.path().to_owned()).ok(),
        Err(e) => {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                message: format!("Cannot resolve '{}' from '{}'", specifier, from_file),
                file: Some(from_file.to_string()),
                line: None,
                column: None,
                help: Some(format!("Resolution error: {}", e)),
                code: DiagnosticCode::UnresolvableImport,
            });
            None
        }
    }
}
