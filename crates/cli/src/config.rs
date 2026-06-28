use oxc_react_docgen_core::pipeline::PipelineOptions;

/// Find and load docgen.config.ts, walking up from start_dir to workspace root.
pub fn load_config_file(start_dir: &std::path::Path) -> Option<PipelineOptions> {
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

pub fn try_load_config(path: &std::path::Path) -> Option<PipelineOptions> {
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

pub fn build_options(
    src: &[String],
    no_cross_package: bool,
    react_version: Option<&str>,
    cache_dir: Option<&str>,
    config_path: Option<&str>,
) -> PipelineOptions {
    let cwd = std::env::current_dir().unwrap_or_default();
    let config_override =
        config_path.map(std::path::PathBuf::from).and_then(|p| try_load_config(&p)).or_else(|| load_config_file(&cwd));

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
