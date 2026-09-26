use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;

use crossterm::event::KeyCode;
use miette::{IntoDiagnostic, Result};
use oxc_react_docgen_core::pipeline::{PipelineOptions, WatchSession};
use oxc_react_docgen_core::types::{Diagnostic, DiagnosticCode, DiagnosticSeverity, ExtractionOutput};

use crate::config::{build_options, BuildOptionsArgs};
use crate::output::{print_diagnostics, print_summary, write_atomic};

/// Watch mode never runs `--strict` — there's no CLI flag for it — so this is always `exit_code(false)`.
fn watch_exit_code(output: &ExtractionOutput) -> i32 {
    output.exit_code(false)
}

/// Builds the options, prints the banner, and runs the first extraction.
fn start_session(
    args: &crate::WatchArgs,
    quiet: bool,
    config_path: Option<&str>,
) -> Result<(PipelineOptions, Arc<WatchSession>, Arc<AtomicI32>)> {
    use indicatif::{ProgressBar, ProgressStyle};
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

    if !quiet {
        println!();
        println!(
            "  {}  {} watching {}  {}",
            "⚡".yellow(),
            "oxc-react-docgen".bold(),
            options.src_dirs.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ").cyan(),
            "(press q to quit, r to re-extract)".dimmed()
        );
        println!();
    }

    let pb = if !quiet {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner().template("{spinner:.cyan} {msg}").into_diagnostic()?);
        pb.set_message("Extracting...");
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        Some(pb)
    } else {
        None
    };

    let session = Arc::new(WatchSession::new(options.clone()));
    let first = session.initialize();

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }
    if !quiet {
        print_summary(&first, quiet);
        print_diagnostics(&first.diagnostics);
    }

    let exit_code = Arc::new(AtomicI32::new(watch_exit_code(&first)));
    Ok((options, session, exit_code))
}

/// `Some(exit code)` when `key` quits. `r` only refreshes the tracked exit code: past its first call `initialize()`
/// returns the current snapshot without re-extracting.
fn handle_key(key: KeyCode, session: &WatchSession, exit_code: &AtomicI32) -> Option<i32> {
    match key {
        KeyCode::Char('q') | KeyCode::Char('c') => Some(exit_code.load(Ordering::Relaxed)),
        KeyCode::Char('r') => {
            exit_code.store(watch_exit_code(&session.initialize()), Ordering::Relaxed);
            None
        }
        _ => None,
    }
}

fn spawn_keyboard_thread(session: Arc<WatchSession>, exit_code: Arc<AtomicI32>, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        use crossterm::event::{self, Event};
        let _ = crossterm::terminal::enable_raw_mode();
        while running.load(Ordering::Relaxed) {
            if let Ok(Event::Key(key)) = event::read() {
                if let Some(code) = handle_key(key.code, &session, &exit_code) {
                    let _ = crossterm::terminal::disable_raw_mode();
                    // watchexec has no graceful-quit handle reachable from this thread, so this hard-exits, with the
                    // tracked code so a session that quit on an unresolved error still fails the shell.
                    std::process::exit(code);
                }
            }
        }
        let _ = crossterm::terminal::disable_raw_mode();
    });
}

/// Returns the diagnostic to report when the snapshot can't be written to `path`.
fn write_snapshot(path: &str, snapshot: &ExtractionOutput) -> Option<Diagnostic> {
    let json = serde_json::to_string(snapshot).ok()?;
    let error = write_atomic(path, &json).err()?;
    Some(Diagnostic {
        severity: DiagnosticSeverity::Error,
        message: format!("Failed to write '{path}': {error}"),
        file: Some(path.to_owned()),
        line: None,
        column: None,
        help: Some("Check that the output path's parent directory exists and is writable.".into()),
        code: DiagnosticCode::IoError,
    })
}

/// One file-change event: re-resolves a TypeScript `path`, tracks the exit code, prints the delta, rewrites `--out`.
fn handle_change(session: &WatchSession, path: &Path, quiet: bool, out: Option<&str>, exit_code: &AtomicI32) {
    if !matches!(path.extension().and_then(|e| e.to_str()), Some("ts" | "tsx")) {
        return;
    }
    let Ok(path) = camino::Utf8PathBuf::from_path_buf(path.to_owned()) else { return };

    let update = session.update_file(&path);
    // The cumulative snapshot, not this event's delta: a clean update to one file must not reset the exit code while
    // an earlier file in the session still has an unresolved error.
    let snapshot = session.snapshot();
    exit_code.store(watch_exit_code(&snapshot), Ordering::Relaxed);

    if !quiet {
        use owo_colors::OwoColorize;
        let names: Vec<_> = update.updated_components.iter().map(|c| c.display_name.as_str()).collect();
        if !names.is_empty() {
            println!("  {}  {}", path.file_name().unwrap_or("?").dimmed(), names.join(", ").bold());
        }
        print_diagnostics(&update.diagnostics);
    }
    if let Some(failure) = out.and_then(|out| write_snapshot(out, &snapshot)) {
        print_diagnostics(&[failure]);
    }
}

pub fn cmd_watch(args: crate::WatchArgs, quiet: bool, config_path: Option<&str>) -> Result<i32> {
    let (options, session, exit_code) = start_session(&args, quiet, config_path)?;

    let running = Arc::new(AtomicBool::new(true));
    spawn_keyboard_thread(session.clone(), exit_code.clone(), running.clone());

    // watchexec's constructor is synchronous even though its event loop is async.
    let src_dirs: Vec<std::path::PathBuf> = options.src_dirs.iter().map(|p| p.as_std_path().to_owned()).collect();

    let rt = tokio::runtime::Runtime::new().into_diagnostic()?;
    let handler_exit_code = exit_code.clone();
    rt.block_on(async move {
        use watchexec::Watchexec;

        let wx = Watchexec::new(move |action| {
            for event in action.events.iter() {
                for (path, _) in event.paths() {
                    handle_change(&session, path, quiet, args.out.as_deref(), &handler_exit_code);
                }
            }
            action
        })
        .into_diagnostic()?;

        wx.config.pathset(src_dirs);
        wx.main().await.into_diagnostic()??;
        Ok::<(), miette::Error>(())
    })?;

    running.store(false, Ordering::Relaxed);
    Ok(exit_code.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    use camino::Utf8PathBuf;
    use tempfile::TempDir;

    fn empty_output() -> ExtractionOutput {
        ExtractionOutput {
            components: Default::default(),
            enums: Default::default(),
            diagnostics: vec![],
            stats: Default::default(),
        }
    }

    #[test]
    fn watch_exit_code_mirrors_extraction_output_exit_code_non_strict() {
        assert_eq!(watch_exit_code(&empty_output()), 0);

        let with_error = ExtractionOutput {
            diagnostics: vec![Diagnostic {
                severity: DiagnosticSeverity::Error,
                message: "boom".into(),
                file: None,
                line: None,
                column: None,
                help: None,
                code: DiagnosticCode::Unknown,
            }],
            ..empty_output()
        };
        assert_eq!(watch_exit_code(&with_error), 2);
    }

    const WIDGET: &str = "export function Widget(props: { label: string }) { return null; }\n";

    /// A session over `src`; the DTS cache lives in `scratch` so it never lands inside the watched tree.
    fn session_over(src: &TempDir, scratch: &TempDir) -> WatchSession {
        WatchSession::new(PipelineOptions {
            src_dirs: vec![Utf8PathBuf::from_path_buf(src.path().to_owned()).unwrap()],
            cache_dir: Some(Utf8PathBuf::from_path_buf(scratch.path().join("cache")).unwrap()),
            ..Default::default()
        })
    }

    fn prop_names(written: &serde_json::Value, component: &str) -> Vec<String> {
        written["components"][component]["props"].as_object().unwrap().keys().cloned().collect()
    }

    #[test]
    fn a_typescript_change_rewrites_out_with_the_cumulative_snapshot() {
        let (src, scratch) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let widget = src.path().join("Widget.tsx");
        std::fs::write(&widget, WIDGET).unwrap();
        let session = session_over(&src, &scratch);
        let _ = session.initialize();

        std::fs::write(&widget, "export function Widget(props: { label: string; size: number }) { return null; }\n")
            .unwrap();
        let out = scratch.path().join("out.json");
        let exit_code = AtomicI32::new(7);
        handle_change(&session, &widget, true, out.to_str(), &exit_code);

        let written: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(prop_names(&written, "Widget"), vec!["label", "size"]);
        assert_eq!(exit_code.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_non_typescript_event_changes_nothing() {
        let (src, scratch) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let session = session_over(&src, &scratch);
        let _ = session.initialize();
        let out = scratch.path().join("out.json");
        let exit_code = AtomicI32::new(7);

        handle_change(&session, &src.path().join("notes.md"), true, out.to_str(), &exit_code);

        assert!(!out.exists(), "a non-TS event must not rewrite --out");
        assert_eq!(exit_code.load(Ordering::Relaxed), 7);
    }

    #[test]
    fn a_clean_change_does_not_reset_the_exit_code_of_an_earlier_error() {
        let (src, scratch) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let widget = src.path().join("Widget.tsx");
        std::fs::write(&widget, WIDGET).unwrap();
        let session = session_over(&src, &scratch);
        let _ = session.initialize();
        let exit_code = AtomicI32::new(0);

        // An unreadable file is an error-severity diagnostic that stays in the session's snapshot.
        handle_change(&session, &src.path().join("Missing.tsx"), true, None, &exit_code);
        assert_eq!(exit_code.load(Ordering::Relaxed), 2);

        handle_change(&session, &widget, true, None, &exit_code);
        assert_eq!(exit_code.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn an_unwritable_out_path_is_reported_as_an_io_error_naming_the_path() {
        let scratch = TempDir::new().unwrap();
        let path = scratch.path().join("missing-dir").join("out.json");
        let path = path.to_str().unwrap();

        let failure = write_snapshot(path, &empty_output()).expect("writing into a missing directory must fail");

        assert_eq!(
            (failure.severity, failure.code, failure.file.as_deref()),
            (DiagnosticSeverity::Error, DiagnosticCode::IoError, Some(path))
        );
        assert!(failure.message.starts_with(&format!("Failed to write '{path}': ")), "message: {}", failure.message);
        assert_eq!(write_snapshot(scratch.path().join("ok.json").to_str().unwrap(), &empty_output()), None);
    }

    #[test]
    fn q_quits_with_the_tracked_exit_code_and_other_keys_keep_running() {
        let (src, scratch) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let session = session_over(&src, &scratch);
        let _ = session.initialize();
        let exit_code = AtomicI32::new(2);

        assert_eq!(handle_key(KeyCode::Char('q'), &session, &exit_code), Some(2));
        assert_eq!(handle_key(KeyCode::Char('x'), &session, &exit_code), None);
        assert_eq!(exit_code.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn r_refreshes_the_tracked_exit_code_from_the_session() {
        let (src, scratch) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let session = session_over(&src, &scratch);
        let _ = session.initialize();
        // An unreadable file leaves an error-severity diagnostic in the session.
        handle_change(&session, &src.path().join("Missing.tsx"), true, None, &AtomicI32::new(0));
        let exit_code = AtomicI32::new(0);

        assert_eq!(handle_key(KeyCode::Char('r'), &session, &exit_code), None);
        assert_eq!(exit_code.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn start_session_runs_the_first_extraction_quiet_or_not() {
        for quiet in [true, false] {
            let src = TempDir::new().unwrap();
            std::fs::write(src.path().join("Widget.tsx"), WIDGET).unwrap();
            let dir = src.path().to_str().unwrap().to_owned();
            let args = crate::WatchArgs { src: vec![dir.clone()], out: None };

            let (options, session, exit_code) = start_session(&args, quiet, None).unwrap();

            assert_eq!(options.src_dirs, vec![Utf8PathBuf::from(dir)]);
            let components: Vec<String> = session.snapshot().components.keys().cloned().collect();
            assert_eq!(components, vec!["Widget".to_owned()]);
            assert_eq!(exit_code.load(Ordering::Relaxed), 0);
        }
    }
}
