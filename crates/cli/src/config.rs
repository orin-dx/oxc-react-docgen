use miette::{IntoDiagnostic, Result, WrapErr};
use oxc_react_docgen_core::pipeline::{HtmlAttributeMode, PipelineOptions};
use serde::Deserialize;

/// The JSON-facing shape of `docgen.config.ts`'s default export.
///
/// A deliberate subset of `PipelineOptions` — everything except the fields that
/// need complex map/object values (`extraPaths`, `knownTypeOverrides`), which
/// aren't supported via config file yet. `deny_unknown_fields` means a config
/// using one of those (or a typo'd key) gets a clear error naming the field, not
/// a silently-ignored setting — same reasoning as this module's existing "found
/// but unsupported" hard error above.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DocgenConfigSchema {
    src_dirs: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    exclude_prefixes: Option<Vec<String>>,
    react_version: Option<String>,
    cross_package: Option<bool>,
    pandacss_outdir: Option<String>,
    variant_functions: Option<Vec<String>>,
    html_attributes: Option<String>,
    tsconfig_path: Option<String>,
    vanilla_extract: Option<bool>,
    cache_dir: Option<String>,
    extra_builtins: Option<Vec<String>>,
}

impl DocgenConfigSchema {
    /// `Err` names the bad `reactVersion` value — a typo must not silently
    /// fall back to react19 (non-negotiable #6: never fail silently).
    fn into_pipeline_options(self) -> Result<PipelineOptions, String> {
        let mut opts = PipelineOptions::default();
        if let Some(dirs) = self.src_dirs {
            opts.src_dirs = dirs.into_iter().map(Into::into).collect();
        }
        if let Some(patterns) = self.exclude_patterns {
            opts.exclude_patterns = patterns;
        }
        if let Some(prefixes) = self.exclude_prefixes {
            opts.exclude_prefixes = prefixes;
        }
        if let Some(v) = self.react_version.as_deref() {
            opts.react_version = oxc_react_docgen_core::react_types::parse_react_version(v)
                .map_err(|bad| format!("reactVersion is '{bad}', expected \"react18\" or \"react19\""))?;
        }
        if let Some(cross_package) = self.cross_package {
            opts.cross_package = cross_package;
        }
        if let Some(dir) = self.pandacss_outdir {
            opts.pandacss_outdir = Some(dir.into());
        }
        if let Some(fns) = self.variant_functions {
            opts.variant_functions = fns;
        }
        if let Some(mode) = self.html_attributes.as_deref() {
            opts.html_attributes = match mode {
                "full" => HtmlAttributeMode::Full,
                "none" => HtmlAttributeMode::None,
                _ => HtmlAttributeMode::Curated,
            };
        }
        if let Some(path) = self.tsconfig_path {
            opts.tsconfig_path = Some(path.into());
        }
        if let Some(vanilla_extract) = self.vanilla_extract {
            opts.vanilla_extract = vanilla_extract;
        }
        if let Some(dir) = self.cache_dir {
            opts.cache_dir = Some(dir.into());
        }
        if let Some(names) = self.extra_builtins {
            opts.extra_builtins = names.into_iter().map(Into::into).collect();
        }
        Ok(opts)
    }
}

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

/// Execute `docgen.config.ts` at `path` and map its default export onto
/// [`PipelineOptions`].
///
/// A config file that fails to evaluate, or evaluates to something that doesn't
/// match [`DocgenConfigSchema`] (a typo'd key, wrong value type, or a field this
/// module doesn't support yet), is a hard error rather than silently falling back
/// to defaults — CLAUDE.md non-negotiable #6: never fail silently. A
/// config a user explicitly wrote getting quietly ignored is a plausible-looking
/// but wrong result forever.
pub fn try_load_config(path: &std::path::Path) -> Result<Option<PipelineOptions>> {
    use std::io::Write;

    // Pass the config path via environment variable to avoid any path content
    // being interpreted as JavaScript (command injection via crafted filenames).
    let script = "import { pathToFileURL } from 'node:url';\
        const p = process.env.__DOCGEN_CONFIG_PATH;\
        const m = await import(pathToFileURL(p).href);\
        process.stdout.write(JSON.stringify(m.default ?? m));";

    let mut command = std::process::Command::new("node");
    command
        .args(["--input-type=module"])
        .env("NODE_OPTIONS", "--import=tsx/esm")
        .env("__DOCGEN_CONFIG_PATH", path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // Resolve `tsx` (and anything the config file itself imports) relative to the
    // config file's own directory, not wherever the CLI happened to be invoked
    // from — matters for `--config ../other-project/docgen.config.ts` pointing
    // outside the current project, and for a project whose `tsx` devDependency
    // isn't hoisted to whatever the caller's own cwd happens to be.
    if let Some(dir) = path.parent() {
        command.current_dir(dir);
    }
    let mut child = command
        .spawn()
        .into_diagnostic()
        .wrap_err("Failed to spawn node to evaluate docgen.config.ts — is node installed and on PATH?")?;

    // drop(stdin) below signals EOF so node's read finishes.
    let mut stdin =
        child.stdin.take().ok_or_else(|| miette::miette!("failed to open stdin for the node subprocess"))?;
    stdin.write_all(script.as_bytes()).into_diagnostic()?;
    drop(stdin);

    let output = child.wait_with_output().into_diagnostic()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette::miette!(
            help = "Check docgen.config.ts for syntax errors, or remove it to use defaults.",
            "docgen.config.ts at {} failed to evaluate ({}):\n{}",
            path.display(),
            output.status,
            stderr.trim(),
        ));
    }

    let schema: DocgenConfigSchema = serde_json::from_slice(&output.stdout).map_err(|e| {
        miette::miette!(
            help = "Check docgen.config.ts's exported fields against the supported schema \
                    (srcDirs, excludePatterns, excludePrefixes, reactVersion, crossPackage, \
                    pandacssOutdir, variantFunctions, htmlAttributes, tsconfigPath, \
                    vanillaExtract, cacheDir, extraBuiltins).",
            "docgen.config.ts at {} doesn't match the expected shape: {}",
            path.display(),
            e,
        )
    })?;

    let opts =
        schema.into_pipeline_options().map_err(|e| miette::miette!("docgen.config.ts at {}: {}", path.display(), e))?;
    Ok(Some(opts))
}

/// CLI-flag overrides for [`build_options`], bundled to stay under this crate's
/// `too-many-arguments-threshold` of 6 (see `.clippy.toml`).
pub struct BuildOptionsArgs<'a> {
    pub src: &'a [String],
    pub no_cross_package: bool,
    pub react_version: Option<&'a str>,
    pub cache_dir: Option<&'a str>,
    pub html_attributes: Option<&'a str>,
    pub config_path: Option<&'a str>,
    pub extra_builtins: &'a [String],
}

pub fn build_options(args: BuildOptionsArgs) -> Result<PipelineOptions> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let config_override = match args.config_path {
        Some(p) => try_load_config(std::path::Path::new(p))?,
        None => load_config_file(&cwd)?,
    };

    let mut opts = config_override.unwrap_or_default();

    if !args.src.is_empty() {
        opts.src_dirs = args.src.iter().map(|s| s.into()).collect();
    }
    if args.no_cross_package {
        opts.cross_package = false;
    }
    if let Some(v) = args.react_version {
        opts.react_version = oxc_react_docgen_core::react_types::parse_react_version(v).map_err(|bad| {
            miette::miette!(
                help = "Expected \"react18\" or \"react19\".",
                "--react-version is '{}', which isn't a recognized value",
                bad
            )
        })?;
    }
    if let Some(dir) = args.cache_dir {
        opts.cache_dir = Some(dir.into());
    }
    if let Some(mode) = args.html_attributes {
        opts.html_attributes = match mode {
            "full" => oxc_react_docgen_core::pipeline::HtmlAttributeMode::Full,
            "none" => oxc_react_docgen_core::pipeline::HtmlAttributeMode::None,
            _ => oxc_react_docgen_core::pipeline::HtmlAttributeMode::Curated,
        };
    }
    if !args.extra_builtins.is_empty() {
        opts.extra_builtins = args.extra_builtins.iter().map(Into::into).collect();
    }

    Ok(opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_schema_maps_recognized_fields_onto_pipeline_options() {
        let json = r#"{
            "srcDirs": ["app/components"],
            "crossPackage": false,
            "htmlAttributes": "full",
            "reactVersion": "react18",
            "extraBuiltins": ["MyCustomType", "AnotherType"]
        }"#;
        let schema: DocgenConfigSchema = serde_json::from_str(json).expect("valid config JSON");
        let opts = schema.into_pipeline_options().expect("valid reactVersion");

        assert_eq!(opts.src_dirs, vec![camino::Utf8PathBuf::from("app/components")]);
        assert!(!opts.cross_package);
        assert_eq!(opts.html_attributes, HtmlAttributeMode::Full);
        // react18: children implicit, ref requires forwardRef.
        assert!(opts.react_version.implicit_children);
        assert!(!opts.react_version.ref_as_prop);
        let extra_builtins: std::collections::BTreeSet<String> =
            opts.extra_builtins.iter().map(|s| s.to_string()).collect();
        assert_eq!(extra_builtins, ["AnotherType", "MyCustomType"].into_iter().map(String::from).collect());
    }

    #[test]
    fn config_schema_rejects_unknown_fields() {
        // Matches this module's "never fail silently" stance: a typo'd or
        // not-yet-supported key must be a clear error, not silently dropped.
        let json = r#"{ "srcDirs": ["src"], "totallyMadeUpField": true }"#;
        let result: std::result::Result<DocgenConfigSchema, _> = serde_json::from_str(json);
        assert!(result.is_err(), "expected an unknown field to be rejected");
    }

    #[test]
    fn config_schema_rejects_a_typo_d_react_version_instead_of_defaulting() {
        // Adversarial review finding: a typo'd reactVersion silently defaulted
        // to react19 with no error at all.
        let json = r#"{ "reactVersion": "react20" }"#;
        let schema: DocgenConfigSchema = serde_json::from_str(json).expect("valid config JSON");
        let err = schema.into_pipeline_options().expect_err("react20 is not a recognized reactVersion");
        assert!(err.contains("react20"), "expected the bad value named in the error, got: {err}");
    }

    #[test]
    fn config_schema_defaults_are_used_when_a_field_is_absent() {
        let schema: DocgenConfigSchema = serde_json::from_str("{}").expect("empty object is valid");
        let opts = schema.into_pipeline_options().expect("no reactVersion means no validation to fail");
        let defaults = PipelineOptions::default();

        assert_eq!(opts.src_dirs, defaults.src_dirs);
        assert_eq!(opts.html_attributes, defaults.html_attributes);
    }

    #[test]
    fn try_load_config_maps_a_real_docgen_config_ts_file() {
        // Real end-to-end proof: an actual docgen.config.ts, evaluated by the real
        // node+tsx subprocess this module spawns, not just the pure schema mapping
        // tested above. Placed under apps/validate/ (not a bare system tempdir) —
        // tsx is only a devDependency there, not hoisted to the repo root, and the
        // subprocess resolves it via Node's own ancestor-walking package resolution
        // starting from the config file's directory.
        let validate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/validate");
        let tmp = tempfile::TempDir::new_in(&validate_dir).unwrap();
        let config_path = tmp.path().join("docgen.config.ts");
        std::fs::write(
            &config_path,
            r#"
export default {
  srcDirs: ["src/components"],
  htmlAttributes: "full",
  crossPackage: false,
};
"#,
        )
        .unwrap();

        let opts = try_load_config(&config_path).expect("real config file should load successfully");
        let opts = opts.expect("expected Some(PipelineOptions)");

        assert_eq!(opts.src_dirs, vec![camino::Utf8PathBuf::from("src/components")]);
        assert_eq!(opts.html_attributes, HtmlAttributeMode::Full);
        assert!(!opts.cross_package);
    }

    #[test]
    fn try_load_config_surfaces_the_real_syntax_error_not_just_an_exit_status() {
        // Adversarial review finding: node/tsx's stderr was sent to
        // Stdio::null(), so a config file that failed to evaluate produced a
        // generic "failed to evaluate (exit status: 1)" with the actual
        // syntax error/stack trace thrown away — the one piece of
        // information that would actually tell the user what's wrong.
        let validate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/validate");
        let tmp = tempfile::TempDir::new_in(&validate_dir).unwrap();
        let config_path = tmp.path().join("docgen.config.ts");
        std::fs::write(&config_path, "export default {{{{ this is not valid javascript").unwrap();

        let err = try_load_config(&config_path).expect_err("a real syntax error should fail to evaluate");
        let message = format!("{err:?}");
        // This repo's apps/validate uses tsx's esbuild-based transform, which
        // reports "ERROR: Expected identifier..."; a native TS-stripping
        // toolchain would instead say "SyntaxError" — accept either so this
        // isn't pinned to one specific transpiler's exact wording.
        assert!(
            message.contains("Expected identifier") || message.contains("SyntaxError"),
            "expected the real node/tsx error output in the message, got: {message}"
        );
    }

    #[test]
    fn build_options_maps_extra_builtins_cli_flag() {
        let extra_builtins = vec!["Foo".to_string(), "Bar".to_string()];
        let opts = build_options(BuildOptionsArgs {
            src: &[],
            no_cross_package: false,
            react_version: None,
            cache_dir: None,
            html_attributes: None,
            config_path: None,
            extra_builtins: &extra_builtins,
        })
        .expect("build_options should succeed with no config file");

        let names: std::collections::BTreeSet<String> = opts.extra_builtins.iter().map(|s| s.to_string()).collect();
        assert_eq!(names, ["Bar", "Foo"].into_iter().map(String::from).collect());
    }

    #[test]
    fn build_options_extra_builtins_cli_flag_overrides_config_file() {
        // Real end-to-end proof (same apps/validate tempdir rationale as
        // try_load_config_maps_a_real_docgen_config_ts_file above): the CLI flag
        // must win over a config-file value, not merge with or lose to it.
        let validate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/validate");
        let tmp = tempfile::TempDir::new_in(&validate_dir).unwrap();
        let config_path = tmp.path().join("docgen.config.ts");
        std::fs::write(
            &config_path,
            r#"
export default {
  srcDirs: ["src/components"],
  extraBuiltins: ["FromConfig"],
};
"#,
        )
        .unwrap();

        let extra_builtins = vec!["FromCli".to_string()];
        let opts = build_options(BuildOptionsArgs {
            src: &[],
            no_cross_package: false,
            react_version: None,
            cache_dir: None,
            html_attributes: None,
            config_path: Some(config_path.to_str().unwrap()),
            extra_builtins: &extra_builtins,
        })
        .expect("build_options should succeed");

        let names: Vec<String> = opts.extra_builtins.iter().map(|s| s.to_string()).collect();
        assert_eq!(names, vec!["FromCli".to_string()]);
    }
}
