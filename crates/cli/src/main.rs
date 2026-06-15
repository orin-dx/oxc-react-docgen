#![recursion_limit = "256"]

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Result, WrapErr};
use oxc_react_docgen_core::pipeline::PipelineOptions;

#[derive(Parser)]
#[command(
    name = "oxc-react-docgen",
    about = "Fast React prop extraction powered by OXC",
    version,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Machine-readable JSON output (suppresses human-readable output)
    #[arg(global = true, long, short = 'j')]
    json: bool,

    /// Verbose output (repeat for more: -v, -vv)
    #[arg(global = true, long, short, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress all non-error output
    #[arg(global = true, long, short)]
    quiet: bool,

    /// Path to docgen.config.ts
    #[arg(global = true, long)]
    config: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Extract prop types and write to stdout or --out file
    Extract(ExtractArgs),

    /// Watch for changes and re-extract
    Watch(WatchArgs),

    /// Validate extraction — exits 2 if any errors. For CI.
    Check(CheckArgs),

    /// Show resolved props for a single component (debugging tool)
    Inspect(InspectArgs),

    /// Generate shell completions
    Completions(CompletionsArgs),
}

#[derive(clap::Args)]
struct ExtractArgs {
    /// Source directories to scan [default: ./src]
    #[arg(long, short, value_delimiter = ',')]
    src: Vec<String>,

    /// Output file [default: stdout]
    #[arg(long, short)]
    out: Option<String>,

    /// Output format
    #[arg(long, short, default_value = "canonical")]
    #[arg(value_enum)]
    format: OutputFormat,

    /// Disable cross-package type resolution
    #[arg(long)]
    no_cross_package: bool,

    /// React version override [default: auto-detect]
    #[arg(long)]
    react_version: Option<String>,

    /// Cache directory
    #[arg(long)]
    cache_dir: Option<String>,
}

#[derive(clap::ValueEnum, Clone)]
enum OutputFormat {
    /// oxc-react-docgen canonical schema (richest)
    Canonical,
    /// react-docgen-typescript compatible output
    Rdt,
    /// Storybook __docgenInfo blocks
    Storybook,
}

#[derive(clap::Args)]
struct InspectArgs {
    /// Component name to inspect
    component: String,

    /// Source directories to scan [default: ./src]
    #[arg(long, short, value_delimiter = ',')]
    src: Vec<String>,
}

#[derive(clap::Args)]
struct WatchArgs {
    /// Source directories to watch [default: ./src]
    #[arg(long, short, value_delimiter = ',')]
    src: Vec<String>,

    /// Output file to write on each change
    #[arg(long, short)]
    out: Option<String>,
}

#[derive(clap::Args)]
struct CheckArgs {
    /// Source directories to scan [default: ./src]
    #[arg(long, short, value_delimiter = ',')]
    src: Vec<String>,

    /// Fail on warnings in addition to errors
    #[arg(long)]
    strict: bool,
}

#[derive(clap::Args)]
struct CompletionsArgs {
    shell: clap_complete::Shell,
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();

    init_tracing(cli.verbose);

    match cli.command {
        Command::Extract(args) => cmd_extract(args, cli.json, cli.quiet, cli.config.as_deref()),
        Command::Watch(args) => cmd_watch(args, cli.quiet, cli.config.as_deref()),
        Command::Check(args) => cmd_check(args, cli.quiet, cli.config.as_deref()),
        Command::Inspect(args) => cmd_inspect(args, cli.config.as_deref()),
        Command::Completions(args) => cmd_completions(args),
    }
}

fn init_tracing(verbose: u8) {
    if verbose == 0 {
        return;
    }
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = match verbose {
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    fmt().with_env_filter(EnvFilter::new(filter)).init();
}

// ─── extract ─────────────────────────────────────────────────────────────────

fn cmd_extract(
    args: ExtractArgs,
    json_mode: bool,
    quiet: bool,
    config_path: Option<&str>,
) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};

    let options = build_options(
        &args.src,
        args.no_cross_package,
        args.react_version.as_deref(),
        args.cache_dir.as_deref(),
        config_path,
    );

    let pb = if !quiet && !json_mode {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message("Extracting...");
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        Some(pb)
    } else {
        None
    };

    let output = oxc_react_docgen_core::pipeline::extract(&options);

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    if json_mode {
        println!("{}", serde_json::to_string(&output).into_diagnostic()?);
        return Ok(());
    }

    if !quiet {
        print_summary(&output, quiet);
        print_diagnostics(&output.diagnostics);
    }

    let json = match args.format {
        OutputFormat::Canonical => serde_json::to_string_pretty(&output).into_diagnostic()?,
        OutputFormat::Rdt => serialize_rdt(&output),
        OutputFormat::Storybook => serialize_storybook(&output),
    };

    match args.out {
        Some(ref path) => std::fs::write(path, &json)
            .into_diagnostic()
            .wrap_err(format!("Writing to {path}"))?,
        None => println!("{json}"),
    }

    if output
        .diagnostics
        .iter()
        .any(|d| matches!(d.severity, oxc_react_docgen_core::types::DiagnosticSeverity::Error))
    {
        std::process::exit(2);
    }

    Ok(())
}

fn serialize_rdt(output: &oxc_react_docgen_core::types::ExtractionOutput) -> String {
    // RDT-compatible shape: flatten to Record<string, ComponentDoc>
    let mut map = serde_json::Map::new();
    for (name, entry) in &output.components {
        let props: serde_json::Map<String, serde_json::Value> = entry
            .props
            .iter()
            .map(|(k, prop)| {
                let obj = serde_json::json!({
                    "name": prop.name,
                    "type": { "name": prop.prop_type.raw_string() },
                    "required": prop.required,
                    "defaultValue": prop.default_value.as_ref().map(|d| serde_json::json!({"value": d.value, "computed": d.computed})),
                    "description": prop.description,
                    "parent": prop.parent.as_ref().map(|p| serde_json::json!({"name": p.name, "fileName": p.file_name})),
                });
                (k.clone(), obj)
            })
            .collect();
        map.insert(
            name.clone(),
            serde_json::json!({
                "displayName": entry.display_name,
                "props": serde_json::Value::Object(props),
                "description": entry.description,
            }),
        );
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap_or_default()
}

fn serialize_storybook(output: &oxc_react_docgen_core::types::ExtractionOutput) -> String {
    // Storybook __docgenInfo shape
    let mut map = serde_json::Map::new();
    for (name, entry) in &output.components {
        let props: serde_json::Map<String, serde_json::Value> = entry
            .props
            .iter()
            .map(|(k, prop)| {
                let obj = serde_json::json!({
                    "name": prop.name,
                    "type": { "name": prop.prop_type.raw_string() },
                    "required": prop.required,
                    "defaultValue": prop.default_value.as_ref().map(|d| serde_json::json!({"value": d.value})),
                    "description": prop.description,
                });
                (k.clone(), obj)
            })
            .collect();
        map.insert(
            name.clone(),
            serde_json::json!({
                "displayName": entry.display_name,
                "props": serde_json::Value::Object(props),
                "description": entry.description,
                "methods": [],
            }),
        );
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap_or_default()
}

// ─── inspect ─────────────────────────────────────────────────────────────────

fn cmd_inspect(args: InspectArgs, config_path: Option<&str>) -> Result<()> {
    use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};
    use owo_colors::OwoColorize;

    let options = build_options(&args.src, false, None, None, config_path);
    let output = oxc_react_docgen_core::pipeline::extract(&options);

    let component = output
        .components
        .get(&args.component)
        .ok_or_else(|| {
            miette::miette!(
                "Component '{}' not found.\nAvailable: {}",
                args.component,
                output
                    .components
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    println!();
    println!(
        "  {}  {}",
        component.display_name.bold(),
        component.file_path.to_string().dimmed()
    );
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
        let req_str = if prop.required {
            "✓".to_string()
        } else {
            "–".to_string()
        };
        let default_str = prop
            .default_value
            .as_ref()
            .map(|d| d.value.clone())
            .unwrap_or_else(|| "–".into());
        let from_str = prop
            .parent
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default();

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
            let element_note = layer
                .html_element
                .as_ref()
                .map(|e| format!(" (<{e}>)"))
                .unwrap_or_default();
            println!(
                "  {} {}{}",
                "↳".dimmed(),
                layer.type_name.dimmed(),
                element_note.dimmed()
            );
        }

        let notable_names: Vec<&str> = component
            .notable_inherited
            .keys()
            .map(|s| s.as_str())
            .collect();
        println!("    Notable: {}", notable_names.join("  ").dimmed());
    }

    println!();
    Ok(())
}

// ─── watch ────────────────────────────────────────────────────────────────────

fn cmd_watch(args: WatchArgs, quiet: bool, config_path: Option<&str>) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};
    use owo_colors::OwoColorize;

    let options = build_options(&args.src, false, None, None, config_path);

    if !quiet {
        println!();
        println!(
            "  {}  {} watching {}  {}",
            "⚡".yellow(),
            "oxc-react-docgen".bold(),
            options
                .src_dirs
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(", ")
                .cyan(),
            "(press q to quit, r to re-extract)".dimmed()
        );
        println!();
    }

    let pb = if !quiet {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message("Extracting...");
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        Some(pb)
    } else {
        None
    };

    let session = std::sync::Arc::new(oxc_react_docgen_core::pipeline::WatchSession::new(
        options.clone(),
    ));
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
                        session_clone.initialize();
                    }
                    _ => {}
                }
            }
        }
        let _ = crossterm::terminal::disable_raw_mode();
    });

    // File watcher using watchexec 8.x (synchronous constructor, async main)
    let src_dirs: Vec<std::path::PathBuf> = options
        .src_dirs
        .iter()
        .map(|p| p.as_std_path().to_owned())
        .collect();

    let rt = tokio::runtime::Runtime::new().into_diagnostic()?;
    rt.block_on(async move {
        use watchexec::Watchexec;

        let session_inner = session.clone();
        let quiet_inner = quiet;
        let out_path = args.out.clone();

        let wx = Watchexec::new(move |action| {
            for event in action.events.iter() {
                for (path, _) in event.paths() {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
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

// ─── check ───────────────────────────────────────────────────────────────────

fn cmd_check(args: CheckArgs, quiet: bool, config_path: Option<&str>) -> Result<()> {
    let options = build_options(&args.src, false, None, None, config_path);
    let output = oxc_react_docgen_core::pipeline::extract(&options);

    let errors: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                oxc_react_docgen_core::types::DiagnosticSeverity::Error
            )
        })
        .collect();
    let warnings: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                oxc_react_docgen_core::types::DiagnosticSeverity::Warning
            )
        })
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

// ─── completions ──────────────────────────────────────────────────────────────

fn cmd_completions(args: CompletionsArgs) -> Result<()> {
    use clap::CommandFactory;
    clap_complete::generate(
        args.shell,
        &mut Cli::command(),
        "oxc-react-docgen",
        &mut std::io::stdout(),
    );
    Ok(())
}

// ─── Config file loading ──────────────────────────────────────────────────────

/// Find and load docgen.config.ts, walking up from start_dir to workspace root.
fn load_config_file(start_dir: &std::path::Path) -> Option<PipelineOptions> {
    let mut dir = start_dir;
    loop {
        let candidate = dir.join("docgen.config.ts");
        if candidate.exists() {
            return try_load_config(&candidate);
        }
        // Stop at workspace root signals
        for signal in &["pnpm-workspace.yaml", "turbo.json", ".moon/workspace.yml"] {
            if dir.join(signal).exists() {
                return None;
            }
        }
        dir = dir.parent()?;
    }
}

fn try_load_config(path: &std::path::Path) -> Option<PipelineOptions> {
    use std::io::Write;

    // Pass the config path via environment variable to avoid any path content
    // being interpreted as JavaScript (command injection via crafted filenames).
    let script = "import { pathToFileURL } from 'node:url';\
        const p = process.env.__DOCGEN_CONFIG_PATH;\
        const m = await import(pathToFileURL(p).href);\
        process.stdout.write(JSON.stringify(m.default ?? m));";

    let mut child = std::process::Command::new("node")
        .args(["--input-type=module"])
        .env("NODE_OPTIONS", "--import=tsx/esm")
        .env("__DOCGEN_CONFIG_PATH", path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    // Write the script to stdin, then close stdin to signal EOF.
    child.stdin.as_mut()?.write_all(script.as_bytes()).ok()?;
    drop(child.stdin.take());

    let output = child.wait_with_output().ok()?;

    if !output.status.success() {
        return None;
    }

    // Parse as JSON value to validate; full mapping is a future TODO
    serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()?;
    // TODO: map JSON fields to PipelineOptions
    None // stub — returns None until full config schema is mapped
}

// ─── build_options helper ─────────────────────────────────────────────────────

fn build_options(
    src: &[String],
    no_cross_package: bool,
    react_version: Option<&str>,
    cache_dir: Option<&str>,
    config_path: Option<&str>,
) -> PipelineOptions {
    let cwd = std::env::current_dir().unwrap_or_default();
    let config_override = config_path
        .map(std::path::PathBuf::from)
        .and_then(|p| try_load_config(&p))
        .or_else(|| load_config_file(&cwd));

    let mut opts = config_override.unwrap_or_default();

    if !src.is_empty() {
        opts.src_dirs = src.iter().map(|s| s.into()).collect();
    }
    if no_cross_package {
        opts.cross_package = false;
    }
    if let Some(v) = react_version {
        opts.react_version = if v == "react18" {
            oxc_react_docgen_core::react_types::REACT_18
        } else {
            oxc_react_docgen_core::react_types::REACT_19
        };
    }
    if let Some(dir) = cache_dir {
        opts.cache_dir = Some(dir.into());
    }

    opts
}

// ─── Output helpers ───────────────────────────────────────────────────────────

fn print_summary(output: &oxc_react_docgen_core::types::ExtractionOutput, quiet: bool) {
    if quiet {
        return;
    }
    use owo_colors::OwoColorize;

    let errors = output
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                oxc_react_docgen_core::types::DiagnosticSeverity::Error
            )
        })
        .count();
    let warnings = output
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                oxc_react_docgen_core::types::DiagnosticSeverity::Warning
            )
        })
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

fn print_diagnostics(diagnostics: &[oxc_react_docgen_core::types::Diagnostic]) {
    use owo_colors::OwoColorize;
    for d in diagnostics {
        let prefix = match d.severity {
            oxc_react_docgen_core::types::DiagnosticSeverity::Error => "error".red().to_string(),
            oxc_react_docgen_core::types::DiagnosticSeverity::Warning => {
                "warn".yellow().to_string()
            }
            oxc_react_docgen_core::types::DiagnosticSeverity::Info => "info".dimmed().to_string(),
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
