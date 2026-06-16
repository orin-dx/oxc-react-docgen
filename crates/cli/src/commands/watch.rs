use miette::{IntoDiagnostic, Result};

use crate::config::build_options;
use crate::output::print_summary;

pub fn cmd_watch(args: crate::WatchArgs, quiet: bool, config_path: Option<&str>) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};
    use owo_colors::OwoColorize;

    let options = build_options(&args.src, false, None, None, config_path);

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
        pb.set_style(ProgressStyle::default_spinner().template("{spinner:.cyan} {msg}").unwrap());
        pb.set_message("Extracting...");
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        Some(pb)
    } else {
        None
    };

    let session =
        std::sync::Arc::new(oxc_react_docgen_core::pipeline::WatchSession::new(options.clone()));
    let first = session.initialize();

    if let Some(ref pb) = pb {
        pb.finish_and_clear();
    }
    if !quiet {
        print_summary(&first, quiet);
    }

    // Keyboard input thread (q=quit, r=re-extract)
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

    // File watcher using watchexec 8.x (synchronous constructor, async main)
    let src_dirs: Vec<std::path::PathBuf> =
        options.src_dirs.iter().map(|p| p.as_std_path().to_owned()).collect();

    let rt = tokio::runtime::Runtime::new().into_diagnostic()?;
    rt.block_on(async move {
        use watchexec::Watchexec;

        let session_inner = session.clone();
        let quiet_inner = quiet;
        let out_path = args.out.clone();

        let wx = Watchexec::new(move |action| {
            for event in action.events.iter() {
                for (path, _) in event.paths() {
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if !matches!(ext, "ts" | "tsx") {
                        continue;
                    }
                    if let Ok(utf8) = camino::Utf8PathBuf::from_path_buf(path.to_owned()) {
                        let update = session_inner.update_file(&utf8);
                        if !quiet_inner {
                            use owo_colors::OwoColorize;
                            let names: Vec<_> = update
                                .updated_components
                                .iter()
                                .map(|c| c.display_name.as_str())
                                .collect();
                            if !names.is_empty() {
                                println!(
                                    "  {}  {}",
                                    utf8.file_name().unwrap_or("?").dimmed(),
                                    names.join(", ").bold()
                                );
                            }
                        }
                        if let Some(ref p) = out_path {
                            let snapshot = session_inner.snapshot();
                            if let Ok(json) = serde_json::to_string(&snapshot) {
                                let _ = std::fs::write(p, json);
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
    Ok(())
}
