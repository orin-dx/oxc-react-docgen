use std::fmt;

use serde::{Deserialize, Serialize};

/// A non-fatal issue discovered during extraction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub help: Option<String>,
    pub code: DiagnosticCode,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{file}: ")?;
        }
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Diagnostic {}

impl Diagnostic {
    /// A file could not be read from disk (permission error, race with deletion, etc).
    pub fn io_read_error(path: &camino::Utf8Path, error: &std::io::Error) -> Diagnostic {
        Diagnostic {
            severity: DiagnosticSeverity::Error,
            message: format!("Failed to read '{path}': {error}"),
            file: Some(path.to_string()),
            line: None,
            column: None,
            help: Some("Check file permissions and that the file exists.".into()),
            code: DiagnosticCode::IoError,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// Machine-readable diagnostic codes for programmatic consumers.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiagnosticCode {
    UnresolvableImport,
    OpaqueType,
    PandaCodegenMissing,
    MaxDepthExceeded,
    ComponentDetectionFailed,
    BarrelResolutionFailed,
    Unknown,
    /// JSDoc @default conflicts with code default value — code value was used.
    JsDocDefaultMismatch,
    /// Default value is a runtime expression that could not be statically evaluated.
    ComputedDefault,
    /// Indexed access type (Type["key"]) that could not be resolved from known tables.
    IndexedAccessOpaque,
    /// Template literal type that could not be statically expanded.
    TemplateLiteralOpaque,
    /// Callable component detected via call signature interface.
    CallableComponent,
    /// Discriminated union detected — props merged with discriminant surfaced.
    DiscriminatedUnion,
    /// File could not be read — permission error or file missing.
    IoError,
    /// Source file exceeds the maximum type-nesting depth; skipped to avoid parser stack overflow.
    ExcessiveNesting,
    /// TypeScript syntax error reported by the parser.
    ParseError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_read_error_reports_the_path_and_underlying_error() {
        let path = camino::Utf8Path::new("src/Button.tsx");
        let error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");

        let diagnostic = Diagnostic::io_read_error(path, &error);

        assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
        assert_eq!(diagnostic.code, DiagnosticCode::IoError);
        assert_eq!(diagnostic.file.as_deref(), Some("src/Button.tsx"));
        assert!(diagnostic.message.contains("src/Button.tsx"));
        assert!(diagnostic.message.contains("permission denied"));
    }
}
