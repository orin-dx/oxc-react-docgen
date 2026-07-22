#![recursion_limit = "256"]

mod commands;
mod config;
mod output;

use clap::{Parser, Subcommand};
use miette::Result;

use commands::check::cmd_check;
use commands::completions::cmd_completions;
use commands::extract::cmd_extract;
use commands::inspect::cmd_inspect;
use commands::watch::cmd_watch;

#[derive(Parser)]
#[command(
    name = "oxc-react-docgen",
    about = "Fast React prop extraction powered by OXC",
    version,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,

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

    /// Validate extraction — exits 2 if any errors, or 1 if --strict and there are warnings. For CI.
    Check(CheckArgs),

    /// Show resolved props for a single component (debugging tool)
    Inspect(InspectArgs),

    /// Generate shell completions
    Completions(CompletionsArgs),
}

#[derive(clap::Args)]
pub struct ExtractArgs {
    /// Source directories to scan [default: ./src]
    #[arg(long, short, value_delimiter = ',')]
    pub src: Vec<String>,

    /// Output file [default: stdout]
    #[arg(long, short)]
    pub out: Option<String>,

    /// Output format
    #[arg(long, short, default_value = "canonical")]
    #[arg(value_enum)]
    pub format: OutputFormat,

    /// Disable cross-package type resolution
    #[arg(long)]
    pub no_cross_package: bool,

    /// React version override [default: auto-detect]
    #[arg(long)]
    pub react_version: Option<String>,

    /// Cache directory
    #[arg(long)]
    pub cache_dir: Option<String>,

    /// How much of an inherited HTML element's attributes to expose
    #[arg(long, value_enum)]
    pub html_attributes: Option<HtmlAttributeModeArg>,

    /// Extra type names to treat as recognized/builtin (no "unknown type" warning) —
    /// e.g. a library-specific type this tool doesn't already know
    #[arg(long, value_delimiter = ',')]
    pub extra_builtins: Vec<String>,

    /// Machine-readable JSON output: the canonical schema, compact, always to
    /// stdout, ignoring --out/--format (which govern the human/RDT/Storybook
    /// paths below), and suppresses the human-readable summary/diagnostics.
    #[arg(long, short = 'j')]
    pub json: bool,
}

#[derive(clap::ValueEnum, Clone)]
pub enum OutputFormat {
    /// oxc-react-docgen canonical schema (richest)
    Canonical,
    /// react-docgen-typescript compatible output
    Rdt,
    /// Storybook __docgenInfo blocks
    Storybook,
}

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum HtmlAttributeModeArg {
    /// ~15-20 curated, commonly-documented attributes per element [default]
    Curated,
    /// All of @types/react's real attributes for the element (matches RDT)
    Full,
    /// No inherited HTML attributes at all — own props only
    None,
}

#[derive(clap::Args)]
pub struct InspectArgs {
    /// Component name to inspect
    pub component: String,

    /// Source directories to scan [default: ./src]
    #[arg(long, short, value_delimiter = ',')]
    pub src: Vec<String>,
}

#[derive(clap::Args)]
pub struct WatchArgs {
    /// Source directories to watch [default: ./src]
    #[arg(long, short, value_delimiter = ',')]
    pub src: Vec<String>,

    /// Output file to write on each change
    #[arg(long, short)]
    pub out: Option<String>,
}

#[derive(clap::Args)]
pub struct CheckArgs {
    /// Source directories to scan [default: ./src]
    #[arg(long, short, value_delimiter = ',')]
    pub src: Vec<String>,

    /// Fail on warnings in addition to errors (exits 1, distinct from the exit-2 error path)
    #[arg(long)]
    pub strict: bool,

    /// Machine-readable JSON diagnostics instead of the human-readable table
    #[arg(long, short = 'j')]
    pub json: bool,
}

#[derive(clap::Args)]
pub struct CompletionsArgs {
    pub shell: clap_complete::Shell,
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();

    init_tracing(cli.verbose);

    let exit_code = match cli.command {
        Command::Extract(args) => cmd_extract(args, cli.quiet, cli.config.as_deref())?,
        Command::Watch(args) => {
            cmd_watch(args, cli.quiet, cli.config.as_deref())?;
            0
        }
        Command::Check(args) => cmd_check(args, cli.quiet, cli.config.as_deref())?,
        Command::Inspect(args) => {
            cmd_inspect(args, cli.config.as_deref())?;
            0
        }
        Command::Completions(args) => {
            cmd_completions(args)?;
            0
        }
    };

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
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
