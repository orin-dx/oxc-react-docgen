//! Extraction pipeline — Phase 3b.
//!
//! Orchestrates: file discovery → parallel parse → global merge → parallel resolve → output.
//! Also manages DTS cache and incremental watch-mode state.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use camino::{Utf8Path, Utf8PathBuf};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::cache::DtsCache;
use crate::resolver::{resolve_component, ResolutionContext};
use crate::types::*;

mod discover;
pub mod watch;

use discover::{discover_files, should_skip};
pub use watch::WatchSession;

// ─── PipelineOptions ─────────────────────────────────────────────────────────

/// User-supplied known type override for custom library patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KnownTypeOverride {
    Opaque { label: Option<String> },
    Alias { arg_index: usize },
    Skip,
}

/// How much of an inherited HTML element's attribute surface to expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HtmlAttributeMode {
    /// ~15-20 curated, commonly-documented attributes per element (onClick,
    /// disabled, aria-*, etc.) — the default. Matches RDT's shape for consumers
    /// that filter node_modules-sourced props, just with a smaller, hand-picked set.
    Curated,
    /// Actually resolve @types/react's real HTMLAttributes/AriaAttributes/
    /// DOMAttributes/<Element>HTMLAttributes interface chain, matching RDT's full
    /// ~250-300 attributes per element.
    Full,
    /// Don't synthesize any inherited HTML attributes at all — own props only.
    None,
}

/// Configuration for a single extraction run.
#[derive(Debug, Clone)]
pub struct PipelineOptions {
    /// Source directories to scan.
    pub src_dirs: Vec<Utf8PathBuf>,
    /// Extra glob-like patterns to exclude (on top of built-in: stories, tests, snapshots).
    pub exclude_patterns: Vec<String>,
    /// Component name prefixes to skip.
    pub exclude_prefixes: Vec<String>,
    /// React version — default: React 19.
    pub react_version: crate::react_types::ReactVersion,
    /// Whether to resolve cross-package types.
    pub cross_package: bool,
    /// PandaCSS generated output dir, if applicable.
    pub pandacss_outdir: Option<Utf8PathBuf>,
    /// Extra function names to treat as cva-like variant functions.
    pub variant_functions: Vec<String>,
    /// How much of an inherited HTML element's attributes to expose.
    pub html_attributes: HtmlAttributeMode,
    // ── Fields from architectural review ─────────────────────────────────────
    /// Path to tsconfig.json (auto-detected if None).
    pub tsconfig_path: Option<Utf8PathBuf>,
    /// Monorepo path aliases: package name → list of candidate root dirs.
    pub extra_paths: FxHashMap<String, Vec<Utf8PathBuf>>,
    /// User-supplied overrides for specific named types.
    pub known_type_overrides: FxHashMap<String, KnownTypeOverride>,
    /// Additional builtin type names beyond the React standard set.
    pub extra_builtins: FxHashSet<compact_str::CompactString>,
    /// Enable vanilla-extract CSS-in-JS support.
    pub vanilla_extract: bool,
    /// Cache directory. Defaults to `node_modules/.cache/oxc-react-docgen`.
    pub cache_dir: Option<Utf8PathBuf>,
    /// Future: opt-in to typescript-go for conditional/mapped type resolution.
    pub resolve_complex_types: bool,
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self {
            src_dirs: vec![Utf8PathBuf::from("./src")],
            exclude_patterns: vec![],
            exclude_prefixes: vec![],
            react_version: crate::react_types::REACT_19,
            cross_package: true,
            pandacss_outdir: None,
            variant_functions: vec!["cva".into(), "tv".into(), "defineRecipe".into(), "recipe".into()],
            html_attributes: HtmlAttributeMode::Curated,
            tsconfig_path: None,
            extra_paths: Default::default(),
            known_type_overrides: Default::default(),
            extra_builtins: Default::default(),
            vanilla_extract: false,
            cache_dir: None,
            resolve_complex_types: false,
        }
    }
}

// ─── Incremental watch types ──────────────────────────────────────────────────

/// Result of processing a single file change in watch mode.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementalUpdate {
    pub updated_components: Vec<ComponentEntry>,
    pub affected_files: Vec<Utf8PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    pub duration_ms: u64,
}

/// Reverse dependency graph: file → list of files that import from it.
/// Built once from GlobalSourceData; used by WatchSession for BFS propagation.
pub struct ReverseDeps {
    pub(super) inner: FxHashMap<Utf8PathBuf, Vec<Utf8PathBuf>>,
}

impl ReverseDeps {
    /// Build from a fully-merged GlobalSourceData.
    pub fn build(global: &GlobalSourceData) -> Self {
        let mut inner: FxHashMap<Utf8PathBuf, Vec<Utf8PathBuf>> = Default::default();

        for (consumer_file, imports) in &global.import_map {
            for import in imports {
                // Only chase relative imports — package imports can't be naively reversed.
                if import.specifier.starts_with('.') {
                    if let Some(parent) = consumer_file.parent() {
                        let target = parent.join(&import.specifier);
                        // Approximate extension normalization without I/O.
                        for ext in &[".ts", ".tsx", "/index.ts", "/index.tsx"] {
                            let candidate = Utf8PathBuf::from(format!("{}{}", target, ext));
                            inner.entry(candidate).or_default().push(consumer_file.clone());
                        }
                    }
                }
            }
        }

        Self { inner }
    }

    /// BFS: collect all files transitively affected by a change to `changed`.
    pub fn affected(&self, changed: &Utf8Path) -> FxHashSet<Utf8PathBuf> {
        let mut visited: FxHashSet<Utf8PathBuf> = Default::default();
        let mut queue: VecDeque<Utf8PathBuf> = VecDeque::new();
        queue.push_back(changed.to_owned());

        while let Some(file) = queue.pop_front() {
            if visited.insert(file.clone()) {
                for dep in self.inner.get(&file).into_iter().flatten() {
                    queue.push_back(dep.clone());
                }
            }
        }

        visited
    }
}

// ─── JSON serialization helpers ──────────────────────────────────────────────

/// Serialize an [`ExtractionOutput`] to a JSON string.
///
/// Defined in core (not in the NAPI crate) so the serde monomorphization for
/// `PropType` (which requires `#![recursion_limit = "2048"]`) happens once here,
/// not in every downstream crate.
pub fn extraction_output_to_json(output: &ExtractionOutput) -> Result<String, serde_json::Error> {
    serde_json::to_string(output)
}

/// Serialize an [`IncrementalUpdate`] to a JSON string (same reason as above).
pub fn incremental_update_to_json(update: &IncrementalUpdate) -> Result<String, serde_json::Error> {
    serde_json::to_string(update)
}

// ─── Main cold extraction ─────────────────────────────────────────────────────

/// Stateless extraction — suitable for CLI and NAPI cold runs.
pub fn extract(options: &PipelineOptions) -> ExtractionOutput {
    extract_with_global(options, false).0
}

/// Shared implementation for `extract()` and `WatchSession::initialize()`.
/// Runs every phase (discover, parse, merge, Full-mode `@types/react`,
/// ambient lib.d.ts, resolve) exactly once and hands back everything a caller
/// might need beyond the public-facing output: the fully-merged
/// `GlobalSourceData` (so watch mode doesn't have to hand-rebuild a second,
/// easily-incomplete one just for incremental updates — see `WatchSession`'s
/// prior bug, where its own rebuild skipped the Full-mode/ambient-lib.d.ts
/// merges entirely) and each file's own parsed `SourceData` (so watch mode can
/// seed its per-file cache without re-parsing). `capture_source_data` is
/// `false` for the one-shot `extract()` path, which never looks at the third
/// tuple element — skip the extra per-file clone in that case.
pub(crate) fn extract_with_global(
    options: &PipelineOptions,
    capture_source_data: bool,
) -> (ExtractionOutput, Arc<GlobalSourceData>, Vec<(Utf8PathBuf, SourceData)>) {
    let start = Instant::now();
    let mut diagnostics = Vec::new();

    // Phase 0: Validate that configured source directories exist. A typo'd --src (or a
    // stale docgen.config.ts srcDirs) must not silently produce an empty-but-valid-looking
    // result — see crates/core/CLAUDE.md non-negotiable #6.
    let missing_src_dirs: Vec<&Utf8PathBuf> =
        options.src_dirs.iter().filter(|dir| !dir.as_std_path().is_dir()).collect();
    if !options.src_dirs.is_empty() && missing_src_dirs.len() == options.src_dirs.len() {
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            message: format!(
                "None of the configured source directories exist: {}",
                options.src_dirs.iter().map(|dir| dir.as_str()).collect::<Vec<_>>().join(", ")
            ),
            file: options.src_dirs.first().map(ToString::to_string),
            line: None,
            column: None,
            help: Some("Check --src (or docgen.config.ts srcDirs) for a typo'd or stale path.".into()),
            code: DiagnosticCode::IoError,
        });
    } else {
        for dir in missing_src_dirs {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                message: format!("Source directory does not exist: {dir}"),
                file: Some(dir.to_string()),
                line: None,
                column: None,
                help: Some("Check --src (or docgen.config.ts srcDirs) for a typo'd or stale path.".into()),
                code: DiagnosticCode::IoError,
            });
        }
    }

    // Load the DTS parse-result cache (silently empty if missing/stale).
    let cache = Arc::new(DtsCache::load_from_disk(options.cache_dir.as_deref()));
    let cache_ref = Arc::clone(&cache);

    // Phase 1: Discover source files.
    let src_files = discover_files(&options.src_dirs, &options.exclude_patterns);
    let files_parsed = src_files.len() as u32;

    // Counter for cache hits (atomic so rayon closures can increment safely).
    let cache_hits = AtomicU32::new(0);

    // Phase 2: Parallel parse with rayon — check DTS cache for .d.ts files.
    let source_data_vec: Vec<(Utf8PathBuf, SourceData, Option<Diagnostic>)> = src_files
        .par_iter()
        .map(|path| {
            let is_dts = path.as_str().ends_with(".d.ts");
            if is_dts {
                if let Some(cached) = cache_ref.get(path) {
                    cache_hits.fetch_add(1, Ordering::Relaxed);
                    return (path.clone(), cached, None);
                }
            }
            let (source, io_diag) = match std::fs::read_to_string(path) {
                Ok(s) => (s, None),
                Err(e) => (
                    String::new(),
                    Some(Diagnostic {
                        severity: DiagnosticSeverity::Error,
                        message: format!("Failed to read '{}': {}", path, e),
                        file: Some(path.to_string()),
                        line: None,
                        column: None,
                        help: Some("Check file permissions and that the file exists.".into()),
                        code: DiagnosticCode::IoError,
                    }),
                ),
            };
            let data = crate::extractor::parse_file(path, &source);
            if is_dts {
                cache_ref.insert(path, data.clone());
            }
            (path.clone(), data, io_diag)
        })
        .collect();

    let dts_cache_hits = cache_hits.load(Ordering::Relaxed);

    // Phase 3: Merge into GlobalSourceData (sequential — fast hash-map insertions).
    let mut global = GlobalSourceData::default();
    let mut per_file_data = Vec::new();
    for (path, mut data, io_diag) in source_data_vec {
        if let Some(d) = io_diag {
            diagnostics.push(d);
        }
        // Surface any diagnostics the extractor raised while parsing this file
        // (excessive nesting, syntax errors) — never drop them silently.
        diagnostics.append(&mut data.diagnostics);
        if capture_source_data {
            per_file_data.push((path.clone(), data.clone()));
        }
        global.merge(&path, data);
    }

    // Phase 3.5: Full HTML attribute mode needs @types/react's real interfaces
    // (HTMLAttributes, AriaAttributes, ButtonHTMLAttributes, etc.) merged in
    // before resolution runs, so the resolver can look them up like any other
    // interface instead of just recording an InheritedLayer. Cached the same way
    // as any other .d.ts — this cost is paid once per @types/react version, not
    // per extraction run.
    if options.html_attributes == HtmlAttributeMode::Full {
        // Must match what discovered file paths look like (always absolute — the
        // `ignore` walker absolutizes them regardless of whether --src was given
        // as relative or absolute), since the resolver looks this same specifier
        // up again per-component relative to each file's own absolute directory.
        // A relative src_dirs entry has too few path components for oxc_resolver's
        // ancestor walk to ever reach the real node_modules tree, so it silently
        // finds nothing instead of the intended real @types/react.
        let from_dir = options
            .src_dirs
            .first()
            .and_then(|dir| std::fs::canonicalize(dir).ok())
            .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
            .unwrap_or_else(|| Utf8PathBuf::from("."));
        match crate::resolver::resolve_package_dts_path(&from_dir, "react") {
            Some(react_dts_path) => {
                let react_dts_path = Utf8PathBuf::from(react_dts_path);
                merge_cached_dts_file(&react_dts_path, &cache, &mut global, &mut diagnostics);
            }
            None => {
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Warning,
                    message: "HtmlAttributeMode::Full requested but @types/react could not be resolved".into(),
                    file: None,
                    line: None,
                    column: None,
                    help: Some("Check that @types/react is installed. Falling back to curated attributes.".into()),
                    code: DiagnosticCode::UnresolvableImport,
                });
            }
        }
    }

    // Phase 3.6: TypeScript's own lib.es5.d.ts/lib.dom.d.ts declare native/DOM
    // ambient globals (Date, RegExp, Element, Node, ...) that never go through
    // an import — nothing else ever has a reason to resolve them the way an
    // import statement triggers @types/react above. Always attempted, not
    // mode-gated: a native global showing up as a bare, unexpandable Named
    // reference (exactly like HTMLAttributes already does) is correct for
    // every user in every mode. Silent no-op when `typescript` isn't reachable
    // (e.g. a project with no node_modules at all) — this is a best-effort
    // enhancement the user never opted into, so failure isn't worth a
    // diagnostic; the existing per-type "cannot resolve" diagnostics still
    // fire exactly as before in that case.
    let from_dir = options
        .src_dirs
        .first()
        .and_then(|dir| std::fs::canonicalize(dir).ok())
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok());
    if let Some(from_dir) = from_dir {
        for lib_path in crate::resolver::resolve_ts_lib_paths(&from_dir) {
            let lib_path = Utf8PathBuf::from(lib_path);
            merge_cached_dts_file(&lib_path, &cache, &mut global, &mut diagnostics);
        }
    }

    let global = Arc::new(global);

    // Phase 4: Resolve all components in parallel.
    let mappings: Vec<ComponentMapping> = global
        .component_mappings
        .iter()
        .filter(|m| !should_skip(&m.component_name, &options.exclude_prefixes))
        .cloned()
        .collect();

    let ctx = Arc::new(ResolutionContext::new(global.clone(), options));
    let results: Vec<(ComponentEntry, Vec<Diagnostic>)> =
        mappings.par_iter().map(|mapping| resolve_component(mapping, &ctx)).collect();

    // Phase 5: Collect output.
    let mut components = std::collections::BTreeMap::new();
    let mut seen_names: std::collections::HashMap<String, u32> = Default::default();

    for (entry, diags) in results {
        let base_name = entry.display_name.clone();
        let count = seen_names.entry(base_name.clone()).or_insert(0);
        *count += 1;

        let key = if *count == 1 {
            base_name
        } else {
            // Suffix with file stem to make unique
            let file_stem = entry.file_path.file_stem().unwrap_or("unknown");
            format!("{} ({})", base_name, file_stem)
        };

        components.insert(key, entry);
        diagnostics.extend(diags);
    }

    let enums = collect_public_enums(&global);

    // Persist cache for the next run.
    // Arc::try_unwrap succeeds because all rayon workers have finished; fallback
    // calls save_to_disk via the Arc deref since DtsCache::save_to_disk takes &self.
    match Arc::try_unwrap(cache) {
        Ok(c) => c.save_to_disk(),
        Err(arc) => arc.save_to_disk(),
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let components_extracted = components.len() as u32;

    let output = ExtractionOutput {
        components,
        enums,
        diagnostics,
        stats: ExtractionStats {
            components_extracted,
            files_parsed,
            dts_cache_hits,
            duration_ms,
            ..Default::default()
        },
    };
    (output, global, per_file_data)
}

// ─── Helper functions ─────────────────────────────────────────────────────────

/// Parse-or-load-from-cache a single `.d.ts` file and merge it into `global`,
/// draining any diagnostics the parse produced into `diagnostics` first —
/// matching how the main per-project-file loop (Phase 3) already handles this.
/// Shared by the Full-mode `@types/react` merge (Phase 3.5) and the ambient
/// `lib.d.ts` merge (Phase 3.6), which previously each merged the parsed data
/// directly with no diagnostics drain at all: an excessive-nesting trip or a
/// parse error while parsing either file would have been silently discarded,
/// since `GlobalSourceData` itself has no diagnostics field to catch it later.
fn merge_cached_dts_file(
    path: &Utf8Path,
    cache: &DtsCache,
    global: &mut GlobalSourceData,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut data = match cache.get(path) {
        Some(cached) => cached,
        None => {
            let source = std::fs::read_to_string(path).unwrap_or_default();
            let data = crate::extractor::parse_file(path, &source);
            cache.insert(path, data.clone());
            data
        }
    };
    diagnostics.append(&mut data.diagnostics);
    global.merge(path, data);
}

/// Collect enum entries that are exported from their source files.
fn collect_public_enums(global: &GlobalSourceData) -> std::collections::BTreeMap<String, Vec<EnumEntry>> {
    let mut result = std::collections::BTreeMap::new();

    for (scoped_key, entries) in &global.enums {
        // scoped_key = "{file_path}:{name}"
        if let Some((file_path_str, name)) = scoped_key.split_once(':') {
            let file_path = Utf8Path::new(file_path_str);
            let is_exported = global
                .re_export_map
                .get(file_path)
                .map(|exports| {
                    exports.iter().any(|e| match e {
                        LexedExport::LocalDeclaration { name: n, .. } => n == name,
                        _ => false,
                    })
                })
                .unwrap_or(false);

            if is_exported {
                result.insert(name.to_owned(), entries.clone());
            }
        }
    }

    result
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn merge_cached_dts_file_does_not_drop_parse_diagnostics() {
        // Adversarial review finding: Phase 3.5 (@types/react Full-mode merge)
        // and Phase 3.6 (ambient lib.d.ts merge) each called global.merge()
        // directly on freshly-parsed data with no diagnostics drain first —
        // GlobalSourceData has no diagnostics field, so anything the extractor
        // flagged while parsing either file (an excessive-nesting trip, a
        // parse error) was silently lost. Mirrors
        // extractor::tests::test_excessive_nesting_guard's exact repro shape.
        let tmp = TempDir::new().unwrap();
        let nested = "(".repeat(2500) + &")".repeat(2500);
        let source = format!("const x = {nested};");
        let path = write_file(&tmp, "lib.d.ts", &source);

        let cache_dir = Utf8PathBuf::from_path_buf(tmp.path().join("cache")).unwrap();
        let cache = DtsCache::load_from_disk(Some(&cache_dir));
        let mut global = GlobalSourceData::default();
        let mut diagnostics = Vec::new();

        merge_cached_dts_file(&path, &cache, &mut global, &mut diagnostics);

        assert!(
            diagnostics.iter().any(|d| d.code == DiagnosticCode::ExcessiveNesting),
            "expected the excessive-nesting diagnostic to survive the merge, got {:?}",
            diagnostics
        );
    }

    /// Write a file into a temp dir and return its Utf8PathBuf.
    fn write_file(dir: &TempDir, name: &str, content: &str) -> Utf8PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        Utf8PathBuf::from_path_buf(path).unwrap()
    }

    // ── test_discover_files ───────────────────────────────────────────────────

    #[test]
    fn test_discover_files() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "Button.tsx", "export const Button = () => null;");
        write_file(&tmp, "types.ts", "export type Foo = string;");
        write_file(&tmp, "styles.css", ".btn {}");
        write_file(&tmp, "README.md", "# docs");

        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let files = discover_files(&[dir], &[]);

        let names: Vec<&str> = files.iter().map(|f| f.file_name().unwrap()).collect();
        assert!(names.contains(&"Button.tsx"), "should find .tsx files");
        assert!(names.contains(&"types.ts"), "should find .ts files");
        assert!(!names.contains(&"styles.css"), "should skip .css files");
        assert!(!names.contains(&"README.md"), "should skip .md files");
    }

    // ── test_exclude_stories ──────────────────────────────────────────────────

    #[test]
    fn test_exclude_stories() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "Button.tsx", "export const Button = () => null;");
        write_file(&tmp, "Button.stories.tsx", "export default {};");
        write_file(&tmp, "Button.test.tsx", "it('works', () => {});");
        write_file(&tmp, "Button.spec.ts", "");

        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let files = discover_files(&[dir], &[]);

        let names: Vec<&str> = files.iter().map(|f| f.file_name().unwrap()).collect();
        assert!(names.contains(&"Button.tsx"), "implementation should be included");
        assert!(!names.contains(&"Button.stories.tsx"), ".stories. should be excluded");
        assert!(!names.contains(&"Button.test.tsx"), ".test. should be excluded");
        assert!(!names.contains(&"Button.spec.ts"), ".spec. should be excluded");
    }

    // ── test_extract_empty_src ────────────────────────────────────────────────

    #[test]
    fn test_extract_empty_src() {
        let tmp = TempDir::new().unwrap();
        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();

        let options = PipelineOptions {
            src_dirs: vec![dir],
            cache_dir: Some(Utf8PathBuf::from_path_buf(tmp.path().join("cache")).unwrap()),
            ..Default::default()
        };

        let output = extract(&options);

        assert!(output.components.is_empty(), "no components from empty dir");
        assert!(output.enums.is_empty(), "no enums from empty dir");
        assert!(output.diagnostics.is_empty(), "no diagnostics from empty dir");
        assert_eq!(output.stats.files_parsed, 0);
        assert_eq!(output.stats.components_extracted, 0);
    }

    // ── test_extract_missing_src_dir ──────────────────────────────────────────

    #[test]
    fn test_extract_missing_src_dir() {
        // A typo'd/stale --src must surface as an Error diagnostic, not silently
        // produce an empty-but-valid-looking result (crates/core/CLAUDE.md #6).
        let tmp = TempDir::new().unwrap();
        let missing = Utf8PathBuf::from_path_buf(tmp.path().join("does-not-exist")).unwrap();

        let options = PipelineOptions {
            src_dirs: vec![missing.clone()],
            cache_dir: Some(Utf8PathBuf::from_path_buf(tmp.path().join("cache")).unwrap()),
            ..Default::default()
        };

        let output = extract(&options);

        let error = output
            .diagnostics
            .iter()
            .find(|d| matches!(d.severity, DiagnosticSeverity::Error) && d.code == DiagnosticCode::IoError)
            .expect("missing src dir should produce an Error/IoError diagnostic");
        assert!(error.message.contains(missing.as_str()), "message should name the missing path");
    }

    // ── test_html_attribute_mode_full_resolves_real_button_attrs_end_to_end ───

    #[test]
    fn test_html_attribute_mode_full_resolves_real_button_attrs_end_to_end() {
        // Real end-to-end proof, not synthetic GlobalSourceData: Full mode should
        // actually find and parse this repo's real @types/react and merge a real
        // ButtonHTMLAttributes field into a real component's props. Placed inside
        // the crate dir (not a bare system tempdir) so ancestor-walking node_modules
        // resolution reaches this repo's real, installed @types/react.
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = TempDir::new_in(manifest_dir).unwrap();
        write_file(
            &tmp,
            "Button.tsx",
            r#"
import * as React from "react";
interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary";
}
export function Button(props: ButtonProps) { return null; }
"#,
        );

        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let options = PipelineOptions {
            src_dirs: vec![dir],
            cache_dir: Some(Utf8PathBuf::from_path_buf(tmp.path().join("cache")).unwrap()),
            html_attributes: HtmlAttributeMode::Full,
            ..Default::default()
        };

        let output = extract(&options);

        // @types/react itself was found and parsed — this must never fail with
        // "could not be resolved" (that's the graceful fallback for @types/react
        // genuinely not being installed, which isn't the case here).
        let unresolvable_react =
            output.diagnostics.iter().find(|d| d.message.contains("@types/react could not be resolved"));
        assert!(unresolvable_react.is_none(), "expected @types/react to resolve in this repo's real node_modules");

        let button = output.components.get("Button").expect("Button component not found");
        // A handful of individual cross-referenced fields inside @types/react's own
        // interface chain (AriaAttributes referenced bare from within the same
        // `declare namespace React {}` block, not through an explicit `React.`
        // qualifier) don't resolve yet — a narrower, separate gap in same-namespace
        // sibling reference resolution, not a regression of this feature. The load-
        // bearing claim is that the bulk of a real element's real attributes merge
        // in as genuine own props, matching RDT's flat behavior.
        assert!(
            button.props.len() > 200,
            "expected the bulk of ButtonHTMLAttributes' real ~235 fields to resolve, got {} props",
            button.props.len()
        );
        assert!(
            button.props.contains_key("formAction"),
            "expected a real ButtonHTMLAttributes field in Button's own props, got {:?}",
            button.props.keys().collect::<Vec<_>>()
        );
        assert!(button.props.contains_key("variant"), "own prop 'variant' should still be present");
    }

    // ── test_named_type_only_import_from_react_resolves_to_real_dts ──────────
    //
    // Regression test for: `import type { AriaAttributes } from "react"` used
    // directly as an `extends` target failed for two independent reasons (this
    // uses AriaAttributes rather than HTMLAttributes specifically because
    // "HTMLAttributes" gets special-cased as a synthetic "div" element by
    // `react_types::html_element_for`, which routes through an unrelated,
    // already-working code path — AriaAttributes has no such shortcut, so it
    // actually exercises general named-import resolution):
    //   1. "react" resolved to its own `index.js` (react's package.json has no
    //      "types"/"typings" field and no "types" export condition — its real
    //      type declarations live in the separate `@types/react` package),
    //      instead of falling back to `@types/react`.
    //   2. Even once the right file is found, `@types/react` declares
    //      everything inside `declare namespace React { ... }`, so
    //      `AriaAttributes` is keyed as "React.AriaAttributes" there — a bare
    //      lookup for "AriaAttributes" (the name as actually imported/used)
    //      never matches.

    #[test]
    fn test_named_type_only_import_from_react_resolves_to_real_dts() {
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = TempDir::new_in(manifest_dir).unwrap();
        write_file(
            &tmp,
            "Panel.tsx",
            r#"
import type { AriaAttributes } from "react";
interface PanelProps extends AriaAttributes {
  collapsible?: boolean;
}
export function Panel(props: PanelProps) { return null; }
"#,
        );

        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let options = PipelineOptions {
            src_dirs: vec![dir],
            cache_dir: Some(Utf8PathBuf::from_path_buf(tmp.path().join("cache")).unwrap()),
            html_attributes: HtmlAttributeMode::Full,
            ..Default::default()
        };

        let output = extract(&options);

        let unresolvable =
            output.diagnostics.iter().find(|d| d.message.contains("Cannot resolve type 'AriaAttributes'"));
        assert!(
            unresolvable.is_none(),
            "expected 'react' to resolve to @types/react's real .d.ts, got diagnostic: {:?}",
            unresolvable
        );

        let panel = output.components.get("Panel").expect("Panel component not found");
        assert!(
            panel.props.contains_key("aria-label"),
            "expected a real AriaAttributes field in Panel's own props, got {:?}",
            panel.props.keys().collect::<Vec<_>>()
        );
        assert!(panel.props.contains_key("collapsible"), "own prop 'collapsible' should still be present");
    }

    // ── test_namespace_import_member_access_resolves_to_real_dts ─────────────
    //
    // Regression test for: `import * as React from "react"` then a plain field
    // reference to `React.DependencyList` failed even in `--html-attributes
    // full` mode, where @types/react genuinely is parsed. Root cause:
    // `resolve_to_canonical` only stripped the "React." prefix for the
    // is_react_builtin check (named.rs step 1) — it never routed a `React.X`
    // member-expression reference through the "react" import at all, since
    // `find_import` only tracks the namespace binding itself ("React", with
    // `exported_name == "*"`), not literal dotted names like
    // "React.DependencyList". It always fell back to a same-file lookup for
    // that literal dotted name, which of course never resolves.

    #[test]
    fn test_namespace_import_member_access_resolves_to_real_dts() {
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = TempDir::new_in(manifest_dir).unwrap();
        write_file(
            &tmp,
            "Table.tsx",
            r#"
import * as React from "react";
interface TableProps {
  cellRendererDependencies?: React.DependencyList;
}
export function Table(props: TableProps) { return null; }
"#,
        );

        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let options = PipelineOptions {
            src_dirs: vec![dir],
            cache_dir: Some(Utf8PathBuf::from_path_buf(tmp.path().join("cache")).unwrap()),
            html_attributes: HtmlAttributeMode::Full,
            ..Default::default()
        };

        let output = extract(&options);

        let unresolvable = output.diagnostics.iter().find(|d| d.message.contains("DependencyList"));
        assert!(
            unresolvable.is_none(),
            "expected React.DependencyList to resolve via the 'react' namespace import, got diagnostic: {:?}",
            unresolvable
        );

        let table = output.components.get("Table").expect("Table component not found");
        assert!(
            table.props.contains_key("cellRendererDependencies"),
            "expected 'cellRendererDependencies' prop, got: {:?}",
            table.props.keys().collect::<Vec<_>>()
        );
    }

    // ── test_native_js_global_resolves_via_real_typescript_lib_files ─────────
    //
    // Regression test for: `Date` (and any other native/DOM ambient global —
    // never imported, so nothing ever had a reason to resolve it) spuriously
    // triggered "Cannot resolve type 'Date' — it will appear as opaque",
    // hundreds of times on a real date-picker-shaped fixture. Real TypeScript
    // declares these in its own lib.es5.d.ts/lib.dom.d.ts (no export/import at
    // all — ambient script context, not a module) — this test proves the
    // pipeline actually finds and parses this repo's real `typescript` package
    // (not a synthetic stand-in) and resolves `Date` through it, structurally,
    // rather than via a hardcoded name list.

    #[test]
    fn test_native_js_global_resolves_via_real_typescript_lib_files() {
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = TempDir::new_in(manifest_dir).unwrap();
        write_file(
            &tmp,
            "DatePicker.tsx",
            r#"
interface DatePickerProps {
  selected?: Date;
  onSelect?: (date: Date) => void;
}
export function DatePicker(props: DatePickerProps) { return null; }
"#,
        );

        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let options = PipelineOptions {
            src_dirs: vec![dir],
            cache_dir: Some(Utf8PathBuf::from_path_buf(tmp.path().join("cache")).unwrap()),
            ..Default::default()
        };

        let output = extract(&options);

        let unresolvable = output.diagnostics.iter().find(|d| d.message.contains("'Date'"));
        assert!(
            unresolvable.is_none(),
            "expected 'Date' to resolve via this repo's real typescript lib.es5.d.ts, got diagnostic: {:?}",
            unresolvable
        );

        let picker = output.components.get("DatePicker").expect("DatePicker component not found");
        assert!(
            picker.props.contains_key("selected"),
            "expected 'selected' prop, got: {:?}",
            picker.props.keys().collect::<Vec<_>>()
        );
    }

    // ── test_indexed_access_into_ambient_dom_interface_resolves_via_real_lib ──
    //
    // Regression test for: day-picker's `HTMLDivElement["dir"]` (also "nonce",
    // "title", "lang") degraded to Opaque even after `lib.dom.d.ts` became
    // parseable. Two compounding bugs, both now fixed: (1) `lib.dom.d.ts` was
    // silently skipped entirely because `max_bracket_nesting_depth` miscounted
    // JSDoc comments containing unmatched brackets (see
    // `extractor::tests::test_nesting_guard_ignores_brackets_inside_comments`);
    // (2) `dir` isn't declared directly on `HTMLDivElement` — only inherited
    // via `extends HTMLElement` — and `resolve_indexed_access` only searched
    // an interface's own fields, never its extends chain. This test proves
    // both fixes compose correctly against this repo's real installed
    // `typescript` package, not just fabricated in-memory data.

    #[test]
    fn test_indexed_access_into_ambient_dom_interface_resolves_via_real_lib() {
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = TempDir::new_in(manifest_dir).unwrap();
        write_file(
            &tmp,
            "Foo.tsx",
            r#"
interface FooProps {
  dir?: HTMLDivElement["dir"];
}
export function Foo(props: FooProps) { return null; }
"#,
        );

        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let options = PipelineOptions {
            src_dirs: vec![dir],
            cache_dir: Some(Utf8PathBuf::from_path_buf(tmp.path().join("cache")).unwrap()),
            ..Default::default()
        };

        let output = extract(&options);

        let unresolvable = output.diagnostics.iter().find(|d| d.message.contains("HTMLDivElement"));
        assert!(
            unresolvable.is_none(),
            "expected HTMLDivElement[\"dir\"] to resolve via this repo's real lib.dom.d.ts, got diagnostic: {:?}",
            unresolvable
        );

        let foo = output.components.get("Foo").expect("Foo component not found");
        let dir_prop = foo.props.get("dir").expect("'dir' prop not found");
        assert_eq!(dir_prop.prop_type, PropType::String, "expected String, got {:?}", dir_prop.prop_type);
    }

    // ── test_static_default_props_assignment_reaches_parsed_prop ─────────────
    //
    // Regression test for: `Button.defaultProps = { size: 'md' }` (deprecated
    // but still real — MUI ships it) was never read at all; only destructured
    // defaults (`function Button({ size = 'md' })`) populated
    // ComponentMapping.param_defaults. Proves the extraction fix reaches all
    // the way through to ExtractionOutput, not just SourceData.

    #[test]
    fn test_static_default_props_assignment_reaches_parsed_prop() {
        let tmp = TempDir::new().unwrap();
        write_file(
            &tmp,
            "Button.tsx",
            r#"
interface ButtonProps {
  size?: string;
}
export function Button(props: ButtonProps) { return null; }
Button.defaultProps = { size: 'md' };
"#,
        );

        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };

        let output = extract(&options);

        let button = output.components.get("Button").expect("Button component not found");
        let size_prop = button.props.get("size").expect("'size' prop not found");
        let default = size_prop.default_value.as_ref().expect("expected a default value for 'size'");
        assert_eq!(default.value, "\"md\"");
        assert!(!default.computed);
    }

    // ── test_pipeline_options_default ─────────────────────────────────────────

    #[test]
    fn test_pipeline_options_default() {
        let opts = PipelineOptions::default();
        let fns = &opts.variant_functions;
        assert!(fns.contains(&"cva".to_string()), "cva should be a default variant function");
        assert!(fns.contains(&"tv".to_string()), "tv should be a default variant function");
        assert!(fns.contains(&"defineRecipe".to_string()), "defineRecipe should be a default variant function");
        assert!(fns.contains(&"recipe".to_string()), "recipe should be a default variant function");
    }

    // ── test_reverse_deps_bfs ─────────────────────────────────────────────────

    #[test]
    fn test_reverse_deps_bfs() {
        // Construct a dependency graph:
        //   a.ts  ← b.ts  ← c.ts
        // If a.ts changes, b.ts and c.ts are affected.
        let mut inner: FxHashMap<Utf8PathBuf, Vec<Utf8PathBuf>> = Default::default();
        let a = Utf8PathBuf::from("/project/a.ts");
        let b = Utf8PathBuf::from("/project/b.ts");
        let c = Utf8PathBuf::from("/project/c.ts");

        // b.ts imports a.ts → a is reverse-depended-on by b
        inner.entry(a.clone()).or_default().push(b.clone());
        // c.ts imports b.ts
        inner.entry(b.clone()).or_default().push(c.clone());

        let rev = ReverseDeps { inner };
        let affected = rev.affected(&a);

        assert!(affected.contains(&a), "changed file should be in affected set");
        assert!(affected.contains(&b), "direct importer should be affected");
        assert!(affected.contains(&c), "transitive importer should be affected");
    }

    // ── test_watch_session_update ─────────────────────────────────────────────

    #[test]
    fn test_watch_session_update() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "Button.tsx", "export const Button = () => null;");

        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_owned()).unwrap();
        let options = PipelineOptions {
            src_dirs: vec![dir],
            cache_dir: Some(Utf8PathBuf::from_path_buf(tmp.path().join("cache")).unwrap()),
            ..Default::default()
        };

        let session = WatchSession::new(options);
        let _ = session.initialize();

        // Modify the file and trigger an incremental update.
        let button_path = Utf8PathBuf::from_path_buf(tmp.path().join("Button.tsx")).unwrap();
        write_file(&tmp, "Button.tsx", "export const Button = ({ label }: { label: string }) => null;");

        let update = session.update_file(&button_path);

        // With the stub resolver, no components are returned (mappings list may be
        // empty for a simple stub file), but the call must complete without panic.
        assert!(update.duration_ms < 5_000, "update should complete quickly");
        // The changed file itself must appear in affected_files. update_file
        // canonicalizes its input (matching discover_files' own symlink-
        // resolved output, e.g. macOS's /var -> /private/var) so compare
        // against the canonicalized form, not necessarily the raw input.
        let canonical_button_path =
            Utf8PathBuf::from_path_buf(std::fs::canonicalize(button_path.as_std_path()).unwrap()).unwrap();
        assert!(
            update.affected_files.contains(&canonical_button_path),
            "changed file should always appear in affected_files"
        );
    }
}
