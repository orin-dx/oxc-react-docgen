use miette::{IntoDiagnostic, Result};

use crate::config::{build_options, BuildOptionsArgs};
use crate::output::{print_diagnostics, print_summary};

/// Watch mode never runs `--strict` — there's no CLI flag for it — so this
/// is always `exit_code(false)`. Named seam so `cmd_watch`'s wiring has
/// something unit-testable without spinning up watchexec.
fn watch_exit_code(output: &oxc_react_docgen_core::types::ExtractionOutput) -> i32 {
    output.exit_code(false)
}

/// Writes `contents` to `path` via a same-directory temp file + rename, so a
/// mid-write failure (disk full, permission revoked) can never leave `path`
/// truncated or half-written. Returns the `io::Error` on failure instead of
/// swallowing it — callers must report it, not discard the `Result`.
fn write_atomic(path: &str, contents: &str) -> std::io::Result<()> {
    let target = std::path::Path::new(path);
    let dir = match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => std::path::Path::new("."),
    };
    let file_name = target.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("'{path}' has no file name component"))
    })?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(file_name);
    tmp_name.push(".tmp");
    let tmp_path = dir.join(tmp_name);
    std::fs::write(&tmp_path, contents)?;
    std::fs::rename(&tmp_path, target)?;
    Ok(())
}

pub fn cmd_watch(args: crate::WatchArgs, quiet: bool, config_path: Option<&str>) -> Result<i32> {
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

    let session = std::sync::Arc::new(oxc_react_docgen_core::pipeline::WatchSession::new(options.clone()));
    let first = session.initialize();

    if let Some(ref pb) = pb {
        pb.finish_and_clear();
    }
    if !quiet {
        print_summary(&first, quiet);
        print_diagnostics(&first.diagnostics);
    }

    let exit_code = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(watch_exit_code(&first)));

    // q=quit, r=re-extract
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();
    let session_clone = session.clone();

    std::thread::spawn(move || {
        use crossterm::event::{self, Event, KeyCode};
        let _ = crossterm::terminal::enable_raw_mode();
        while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(Event::Key(key)) = event::read() {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('c') => {
                        let _ = crossterm::terminal::disable_raw_mode();
                        std::process::exit(0);
                    }
                    KeyCode::Char('r') => {
                        let _ = session_clone.initialize();
                    }
                    _ => {}
                }
            }
        }
        let _ = crossterm::terminal::disable_raw_mode();
    });

    // watchexec's constructor is synchronous even though its event loop is async.
    let src_dirs: Vec<std::path::PathBuf> = options.src_dirs.iter().map(|p| p.as_std_path().to_owned()).collect();

    let rt = tokio::runtime::Runtime::new().into_diagnostic()?;
    let exit_code_outer = exit_code.clone();
    rt.block_on(async move {
        use watchexec::Watchexec;

        let session_inner = session.clone();
        let quiet_inner = quiet;
        let out_path = args.out.clone();
        let exit_code_inner = exit_code_outer.clone();

        let wx = Watchexec::new(move |action| {
            for event in action.events.iter() {
                for (path, _) in event.paths() {
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if !matches!(ext, "ts" | "tsx") {
                        continue;
                    }
                    if let Ok(utf8) = camino::Utf8PathBuf::from_path_buf(path.to_owned()) {
                        let update = session_inner.update_file(&utf8);
                        // Source from the session's cumulative snapshot (same
                        // diagnostics written to `--out`), not just this
                        // event's delta — otherwise a clean file update can
                        // reset the exit code to 0 even though an earlier
                        // file in this session still has an unresolved error.
                        let snapshot = session_inner.snapshot();
                        exit_code_inner.store(watch_exit_code(&snapshot), std::sync::atomic::Ordering::Relaxed);
                        if !quiet_inner {
                            use owo_colors::OwoColorize;
                            let names: Vec<_> =
                                update.updated_components.iter().map(|c| c.display_name.as_str()).collect();
                            if !names.is_empty() {
                                println!("  {}  {}", utf8.file_name().unwrap_or("?").dimmed(), names.join(", ").bold());
                            }
                            print_diagnostics(&update.diagnostics);
                        }
                        if let Some(ref p) = out_path {
                            if let Ok(json) = serde_json::to_string(&snapshot) {
                                if let Err(e) = write_atomic(p, &json) {
                                    print_diagnostics(&[oxc_react_docgen_core::types::Diagnostic {
                                        severity: oxc_react_docgen_core::types::DiagnosticSeverity::Error,
                                        message: format!("Failed to write '{p}': {e}"),
                                        file: Some(p.clone()),
                                        line: None,
                                        column: None,
                                        help: Some(
                                            "Check that the output path's parent directory exists and is writable."
                                                .into(),
                                        ),
                                        code: oxc_react_docgen_core::types::DiagnosticCode::IoError,
                                    }]);
                                }
                            }
                        }
                    }
                }
            }
            action
        })
        .into_diagnostic()?;

        wx.config.pathset(src_dirs);
        wx.main().await.into_diagnostic()??;
        Ok::<(), miette::Error>(())
    })?;

    running.store(false, std::sync::atomic::Ordering::Relaxed);
    Ok(exit_code.load(std::sync::atomic::Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_surfaces_error_when_parent_dir_is_missing() {
        let result = write_atomic("/nonexistent-rdt-watch-dir-xyz-123/out.json", "{}");
        assert!(result.is_err(), "write to a missing parent directory should surface an error, not succeed silently");
    }

    #[test]
    fn write_atomic_writes_via_temp_then_rename() {
        let dir = std::env::temp_dir().join(format!("rdt-watch-atomic-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create test dir");
        let target = dir.join("out.json");

        write_atomic(target.to_str().expect("utf8 path"), "{\"a\":1}").expect("write should succeed");

        let contents = std::fs::read_to_string(&target).expect("read back written file");
        assert_eq!(contents, "{\"a\":1}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn watch_exit_code_mirrors_extraction_output_exit_code_non_strict() {
        let clean = oxc_react_docgen_core::types::ExtractionOutput {
            components: Default::default(),
            enums: Default::default(),
            diagnostics: vec![],
            stats: Default::default(),
        };
        assert_eq!(watch_exit_code(&clean), 0);

        let with_error = oxc_react_docgen_core::types::ExtractionOutput {
            components: Default::default(),
            enums: Default::default(),
            diagnostics: vec![oxc_react_docgen_core::types::Diagnostic {
                severity: oxc_react_docgen_core::types::DiagnosticSeverity::Error,
                message: "boom".into(),
                file: None,
                line: None,
                column: None,
                help: None,
                code: oxc_react_docgen_core::types::DiagnosticCode::Unknown,
            }],
            stats: Default::default(),
        };
        assert_eq!(watch_exit_code(&with_error), 2);
    }
}
