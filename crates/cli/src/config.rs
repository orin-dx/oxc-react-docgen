use miette::{IntoDiagnostic, Result};
use oxc_react_docgen_core::pipeline::PipelineOptions;

/// Find and load docgen.config.ts, walking up from start_dir to workspace root.
///
/// Returns `Ok(None)` when no config file exists anywhere between `start_dir` and the
/// workspace root — callers fall back to [`PipelineOptions::default`]. Returns `Err` when
/// a config file IS found but can't be honored; see [`try_load_config`].
pub fn load_config_file(start_dir: &std::path::Path) -> Result<Option<PipelineOptions>> {
    let mut dir = start_dir;
    loop {
        let candidate = dir.join("docgen.config.ts");
        if candidate.exists() {
            return try_load_config(&candidate);
        }
        // Stop at workspace root signals
        for signal in &["pnpm-workspace.yaml", "turbo.json", ".moon/workspace.yml"] {
            if dir.join(signal).exists() {
                return Ok(None);
            }
        }
        let Some(parent) = dir.parent() else { return Ok(None) };
        dir = parent;
    }
}

/// Execute and validate `docgen.config.ts` at `path`.
///
/// Mapping the config's JSON schema onto [`PipelineOptions`] isn't implemented yet, so a
/// config that evaluates successfully still can't be honored. Returning `Ok(None)` here
/// would mean a config a user explicitly wrote (e.g. with different `srcDirs`) gets
/// silently ignored in favor of defaults — a plausible-looking but wrong result forever
/// (crates/core/CLAUDE.md non-negotiable #6: never fail silently). So any config file that
/// is *found* is a hard error until schema mapping ships.
pub fn try_load_config(path: &std::path::Path) -> Result<Option<PipelineOptions>> {
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
        .into_diagnostic()?;

    // Write the script to stdin, then close stdin to signal EOF.
    let mut stdin =
        child.stdin.take().ok_or_else(|| miette::miette!("failed to open stdin for the node subprocess"))?;
    stdin.write_all(script.as_bytes()).into_diagnostic()?;
    drop(stdin);

    let output = child.wait_with_output().into_diagnostic()?;

    if !output.status.success() {
        return Err(miette::miette!(
            help = "Check docgen.config.ts for syntax errors, or remove it to use defaults.",
            "docgen.config.ts at {} failed to evaluate ({})",
            path.display(),
            output.status,
        ));
    }

    // Parse as JSON to confirm the config at least evaluates to a valid object.
    serde_json::from_slice::<serde_json::Value>(&output.stdout).into_diagnostic()?;

    Err(miette::miette!(
        help = "Remove docgen.config.ts (or drop --config) to use CLI flags and defaults instead.",
        "docgen.config.ts was found at {} but config file support is not yet implemented in this version",
        path.display(),
    ))
}

pub fn build_options(
    src: &[String],
    no_cross_package: bool,
    react_version: Option<&str>,
    cache_dir: Option<&str>,
    config_path: Option<&str>,
) -> Result<PipelineOptions> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let config_override = match config_path {
        Some(p) => try_load_config(std::path::Path::new(p))?,
        None => load_config_file(&cwd)?,
    };

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

    Ok(opts)
}
