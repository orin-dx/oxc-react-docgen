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
    /// Skip HTML props filter.
    pub skip_html_props: bool,
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
            skip_html_props: false,
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
    let start = Instant::now();
    let mut diagnostics = Vec::new();

    // Phase 0: Validate that configured source directories exist. A typo'd --src (or a
    // stale docgen.config.ts srcDirs) must not silently produce an empty-but-valid-looking
    // result — see crates/core/CLAUDE.md non-negotiable #6.
    let missing_src_dirs: Vec<&Utf8PathBuf> =
        options.src_dirs.iter().filter(|dir| !dir.as_std_path().is_dir()).collect();
    if missing_src_dirs.len() == options.src_dirs.len() {
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
    for (path, mut data, io_diag) in source_data_vec {
        if let Some(d) = io_diag {
            diagnostics.push(d);
        }
        // Surface any diagnostics the extractor raised while parsing this file
        // (excessive nesting, syntax errors) — never drop them silently.
        diagnostics.append(&mut data.diagnostics);
        global.merge(&path, data);
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

    ExtractionOutput {
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
    }
}

// ─── Helper functions ─────────────────────────────────────────────────────────

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
        // The changed file itself must appear in affected_files.
        assert!(update.affected_files.contains(&button_path), "changed file should always appear in affected_files");
    }
}
