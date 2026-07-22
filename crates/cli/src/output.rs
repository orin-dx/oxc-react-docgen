/// Human-readable extraction summary. Always written to stderr — stdout is reserved for the
/// JSON payload (canonical/RDT/storybook), in every mode, so `oxc-react-docgen extract | jq .`
/// never sees this interleaved with the data.
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

    eprintln!();
    eprintln!(
        "  {}  {} components  ·  {} enums  ·  {}  ·  {}  ·  {}ms",
        "⚡".yellow(),
        output.stats.components_extracted.to_string().bold(),
        output.enums.len().to_string().bold(),
        if warnings > 0 { format!("{warnings} warnings").yellow().to_string() } else { format!("{warnings} warnings") },
        if errors > 0 { format!("{errors} errors").red().to_string() } else { format!("{errors} errors") },
        output.stats.duration_ms.to_string().bold(),
    );
    eprintln!();
}

/// Human-readable diagnostic list. Always written to stderr — see [`print_summary`].
pub fn print_diagnostics(diagnostics: &[oxc_react_docgen_core::types::Diagnostic]) {
    for line in format_diagnostics(diagnostics) {
        eprintln!("{line}");
    }
}

/// The text between the first pair of single quotes in `message`, e.g. `Date` from
/// `"Cannot resolve type 'Date' in 'a.ts'"`. Falls back to the full message when there's
/// no quoted substring, so unrelated diagnostics never collide on an empty subject.
fn extract_subject(message: &str) -> &str {
    let Some(start) = message.find('\'') else {
        return message;
    };
    let rest = &message[start + 1..];
    match rest.find('\'') {
        Some(len) => &rest[..len],
        None => message,
    }
}

/// One or more diagnostics collapsed into a single reported group, keyed by `(code, subject)`.
struct DiagnosticGroup<'a> {
    representative: &'a oxc_react_docgen_core::types::Diagnostic,
    count: usize,
    files_seen: Vec<Option<&'a str>>,
}

/// Collapses near-duplicate diagnostics (same code + same quoted subject) into groups, so a
/// single root cause that produced hundreds of diagnostics reports as one line with a count
/// instead of flooding the terminal. Order is preserved as first-seen, per group.
fn group_diagnostics(diagnostics: &[oxc_react_docgen_core::types::Diagnostic]) -> Vec<DiagnosticGroup<'_>> {
    let mut groups: Vec<(&oxc_react_docgen_core::types::DiagnosticCode, &str, DiagnosticGroup)> = Vec::new();
    for d in diagnostics {
        let subject = extract_subject(&d.message);
        match groups.iter_mut().find(|(code, s, _)| *code == &d.code && *s == subject) {
            Some((_, _, group)) => {
                group.count += 1;
                if !group.files_seen.contains(&d.file.as_deref()) {
                    group.files_seen.push(d.file.as_deref());
                }
            }
            None => groups.push((
                &d.code,
                subject,
                DiagnosticGroup { representative: d, count: 1, files_seen: vec![d.file.as_deref()] },
            )),
        }
    }
    let mut groups: Vec<DiagnosticGroup> = groups.into_iter().map(|(_, _, group)| group).collect();
    groups.sort_by_key(|group| std::cmp::Reverse(group.count));
    groups
}

/// Formats grouped diagnostics into the lines `print_diagnostics` writes to stderr. Pulled out
/// as a pure function so grouping/formatting behavior is unit-testable without capturing stderr.
fn format_diagnostics(diagnostics: &[oxc_react_docgen_core::types::Diagnostic]) -> Vec<String> {
    use owo_colors::OwoColorize;

    let mut lines = Vec::new();
    for group in group_diagnostics(diagnostics) {
        let d = group.representative;
        let prefix = match d.severity {
            oxc_react_docgen_core::types::DiagnosticSeverity::Error => "error".red().to_string(),
            oxc_react_docgen_core::types::DiagnosticSeverity::Warning => "warn".yellow().to_string(),
            oxc_react_docgen_core::types::DiagnosticSeverity::Info => "info".dimmed().to_string(),
            _ => "info".dimmed().to_string(),
        };

        let suffix = match (group.count, group.files_seen.len()) {
            (1, _) => String::new(),
            (count, 1) => format!("  (×{count})"),
            (count, files) => format!("  (×{count}, {files} files)"),
        };

        if let Some(ref file) = d.file {
            lines.push(format!("  [{prefix}] {file}:{}{suffix}", d.message));
        } else {
            lines.push(format!("  [{prefix}] {}{suffix}", d.message));
        }
        if let Some(ref help) = d.help {
            lines.push(format!("    {} {}", "help:".dimmed(), help));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use owo_colors::OwoColorize;
    use oxc_react_docgen_core::types::{Diagnostic, DiagnosticCode, DiagnosticSeverity};

    use super::{extract_subject, format_diagnostics};

    fn diag(severity: DiagnosticSeverity, code: DiagnosticCode, message: &str, file: Option<&str>) -> Diagnostic {
        Diagnostic {
            severity,
            message: message.to_string(),
            file: file.map(str::to_string),
            line: None,
            column: None,
            help: None,
            code,
        }
    }

    #[test]
    fn extract_subject_finds_first_quoted_substring() {
        assert_eq!(extract_subject("Cannot resolve type 'Date' in 'a.ts'"), "Date");
    }

    #[test]
    fn extract_subject_falls_back_to_full_message_without_quotes() {
        let msg = "Discriminated union detected with discriminant prop 'kind'";
        // sanity: this one DOES have quotes, subject should be "kind"
        assert_eq!(extract_subject(msg), "kind");

        let msg_no_quotes = "Discriminated union detected";
        assert_eq!(extract_subject(msg_no_quotes), msg_no_quotes);
    }

    #[test]
    fn single_diagnostic_is_unchanged_from_todays_format() {
        let d = diag(
            DiagnosticSeverity::Warning,
            DiagnosticCode::OpaqueType,
            "Cannot resolve type 'Date' in 'a.ts' — it will appear as opaque",
            Some("a.ts"),
        );
        let lines = format_diagnostics(std::slice::from_ref(&d));
        let expected = format!("  [{}] {}:{}", "warn".yellow(), "a.ts", d.message);
        assert_eq!(lines, vec![expected]);
    }

    #[test]
    fn groups_same_code_and_subject_across_different_files() {
        let diagnostics = vec![
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'Date' in 'a.ts' — it will appear as opaque",
                Some("a.ts"),
            ),
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'Date' in 'b.ts' — it will appear as opaque",
                Some("b.ts"),
            ),
        ];
        let lines = format_diagnostics(&diagnostics);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("(×2, 2 files)"), "{}", lines[0]);
    }

    #[test]
    fn groups_same_code_and_subject_same_file_without_file_count() {
        let diagnostics = vec![
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'Date' in 'a.ts' — it will appear as opaque",
                Some("a.ts"),
            ),
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'Date' in 'a.ts' — it will appear as opaque",
                Some("a.ts"),
            ),
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'Date' in 'a.ts' — it will appear as opaque",
                Some("a.ts"),
            ),
        ];
        let lines = format_diagnostics(&diagnostics);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("(×3)"), "{}", lines[0]);
        assert!(!lines[0].contains("files"), "{}", lines[0]);
    }

    #[test]
    fn different_code_keeps_diagnostics_in_separate_groups() {
        let diagnostics = vec![
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'Date' in 'a.ts' — it will appear as opaque",
                Some("a.ts"),
            ),
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::IndexedAccessOpaque,
                "Cannot resolve type 'Date' in 'a.ts' — it will appear as opaque",
                Some("a.ts"),
            ),
        ];
        let lines = format_diagnostics(&diagnostics);
        assert_eq!(lines.len(), 2, "{lines:?}");
    }

    #[test]
    fn different_subject_keeps_diagnostics_in_separate_groups() {
        let diagnostics = vec![
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'Date' in 'a.ts' — it will appear as opaque",
                Some("a.ts"),
            ),
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'RegExp' in 'a.ts' — it will appear as opaque",
                Some("a.ts"),
            ),
        ];
        let lines = format_diagnostics(&diagnostics);
        assert_eq!(lines.len(), 2, "{lines:?}");
    }

    #[test]
    fn message_without_quotes_groups_by_full_message_without_panicking() {
        let diagnostics = vec![
            diag(
                DiagnosticSeverity::Info,
                DiagnosticCode::DiscriminatedUnion,
                "Discriminated union detected",
                Some("a.ts"),
            ),
            diag(
                DiagnosticSeverity::Info,
                DiagnosticCode::DiscriminatedUnion,
                "Discriminated union detected",
                Some("b.ts"),
            ),
            diag(DiagnosticSeverity::Info, DiagnosticCode::DiscriminatedUnion, "Some other message", Some("c.ts")),
        ];
        let lines = format_diagnostics(&diagnostics);
        assert_eq!(lines.len(), 2, "{lines:?}");
    }

    #[test]
    fn groups_are_sorted_by_descending_occurrence_count() {
        let diagnostics = vec![
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'RegExp' in 'a.ts' — it will appear as opaque",
                Some("a.ts"),
            ),
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'Date' in 'a.ts' — it will appear as opaque",
                Some("a.ts"),
            ),
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'Date' in 'b.ts' — it will appear as opaque",
                Some("b.ts"),
            ),
            diag(
                DiagnosticSeverity::Warning,
                DiagnosticCode::OpaqueType,
                "Cannot resolve type 'Date' in 'c.ts' — it will appear as opaque",
                Some("c.ts"),
            ),
        ];
        let lines = format_diagnostics(&diagnostics);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("Date"), "{}", lines[0]);
        assert!(lines[0].contains("×3"), "{}", lines[0]);
        assert!(lines[1].contains("RegExp"), "{}", lines[1]);
    }

    #[test]
    fn help_line_prints_once_per_group_from_representative() {
        let mut d1 = diag(
            DiagnosticSeverity::Warning,
            DiagnosticCode::OpaqueType,
            "Cannot resolve type 'Date' in 'a.ts' — it will appear as opaque",
            Some("a.ts"),
        );
        d1.help = Some("Check that the package is installed and its types are resolvable.".to_string());
        let mut d2 = diag(
            DiagnosticSeverity::Warning,
            DiagnosticCode::OpaqueType,
            "Cannot resolve type 'Date' in 'b.ts' — it will appear as opaque",
            Some("b.ts"),
        );
        d2.help = Some("Check that the package is installed and its types are resolvable.".to_string());

        let lines = format_diagnostics(&[d1, d2]);
        let help_lines: Vec<_> = lines.iter().filter(|l| l.contains("Check that the package is installed")).collect();
        assert_eq!(help_lines.len(), 1, "{lines:?}");
    }
}
