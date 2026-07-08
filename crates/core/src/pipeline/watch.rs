use std::sync::{Arc, Mutex};
use std::time::Instant;

use arc_swap::ArcSwap;
use camino::{Utf8Path, Utf8PathBuf};
use dashmap::DashMap;
use rayon::prelude::*;

use crate::resolver::{resolve_component, ResolutionContext};
use crate::types::*;

use super::super::types::{ComponentMapping, GlobalSourceData};
use super::discover::discover_files;
use super::ReverseDeps;
use super::{extract, IncrementalUpdate, PipelineOptions};

/// Stateful session for incremental watch-mode extraction.
///
/// `global` and `component_cache` are updated atomically / concurrently on each
/// file change.  `reverse_deps` is rebuilt on structural changes (new file,
/// deleted file) via a new `WatchSession`.
pub struct WatchSession {
    pub options: PipelineOptions,
    /// Current merged source data — swapped atomically via `ArcSwap`.
    pub global: ArcSwap<GlobalSourceData>,
    /// Reverse-dependency graph built at initialisation time.
    pub reverse_deps: Arc<ReverseDeps>,
    /// Per-file SourceData cache (avoids re-reading files that haven't changed).
    pub source_cache: DashMap<Utf8PathBuf, SourceData>,
    /// Latest resolved component entries, keyed by display name.
    pub component_cache: DashMap<String, ComponentEntry>,
    /// Latest diagnostics per file, replaced (not appended) on each re-update so a
    /// fixed file's stale diagnostics don't linger — mirrors GlobalSourceData::remove_file's
    /// per-file replacement semantics.
    pub diagnostics: DashMap<Utf8PathBuf, Vec<Diagnostic>>,
    /// Guards initialize() so concurrent callers don't race to build caches.
    initialized: Mutex<bool>,
}

impl WatchSession {
    /// Create an empty WatchSession (call `initialize()` before first use).
    pub fn new(options: PipelineOptions) -> Self {
        Self {
            options,
            global: ArcSwap::new(Arc::new(GlobalSourceData::default())),
            reverse_deps: Arc::new(ReverseDeps { inner: Default::default() }),
            source_cache: DashMap::new(),
            component_cache: DashMap::new(),
            diagnostics: DashMap::new(),
            initialized: Mutex::new(false),
        }
    }

    /// Run a full cold extraction to populate this session's caches.
    ///
    /// Idempotent — concurrent or repeated calls return the existing snapshot
    /// without re-running extraction.
    pub fn initialize(&self) -> ExtractionOutput {
        let mut guard = self.initialized.lock().expect("init lock poisoned");
        if *guard {
            return self.snapshot();
        }

        // Cold extraction for the public-facing output.
        let mut output = extract(&self.options);

        // Rebuild GlobalSourceData locally so we can populate our own caches.
        let src_files = discover_files(&self.options.src_dirs, &self.options.exclude_patterns);
        let mut global = GlobalSourceData::default();

        for path in &src_files {
            let source = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    let diagnostic = Diagnostic {
                        severity: DiagnosticSeverity::Error,
                        message: format!("Failed to read '{}': {}", path, e),
                        file: Some(path.to_string()),
                        line: None,
                        column: None,
                        help: Some("Check file permissions and that the file exists.".into()),
                        code: DiagnosticCode::IoError,
                    };
                    output.diagnostics.push(diagnostic.clone());
                    self.diagnostics.insert(path.clone(), vec![diagnostic]);
                    String::new()
                }
            };
            let data = crate::extractor::parse_file(path, &source);
            self.source_cache.insert(path.clone(), data.clone());
            global.merge(path, data);
        }

        // Build reverse deps from the fully-merged global.
        // Note: we can't mutate self.reverse_deps (it's Arc, not ArcSwap).
        // On structural changes the caller should create a new WatchSession.
        let new_global = Arc::new(global);
        self.global.store(new_global);

        // Seed component cache from the cold output.
        for (name, entry) in &output.components {
            self.component_cache.insert(name.clone(), entry.clone());
        }

        *guard = true;
        output
    }

    /// Return the current full extraction state from the in-memory caches.
    /// Suitable for writing to `--out` after each incremental update.
    pub fn snapshot(&self) -> ExtractionOutput {
        let components: std::collections::BTreeMap<String, ComponentEntry> =
            self.component_cache.iter().map(|r| (r.key().clone(), r.value().clone())).collect();
        let global = self.global.load();
        let enums: std::collections::BTreeMap<String, Vec<EnumEntry>> =
            global.enums.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

        // DashMap iteration order is arbitrary — sort by path for deterministic output.
        let mut by_path: Vec<(Utf8PathBuf, Vec<Diagnostic>)> =
            self.diagnostics.iter().map(|r| (r.key().clone(), r.value().clone())).collect();
        by_path.sort_by(|a, b| a.0.cmp(&b.0));
        let diagnostics: Vec<Diagnostic> = by_path.into_iter().flat_map(|(_, ds)| ds).collect();

        ExtractionOutput { components, enums, diagnostics, stats: ExtractionStats::default() }
    }

    /// Handle a single file change — re-resolve only affected components.
    pub fn update_file(&self, changed: &Utf8Path) -> IncrementalUpdate {
        let start = Instant::now();

        // 1. Re-parse the changed file.
        let mut io_diagnostic: Option<Diagnostic> = None;
        let source = match std::fs::read_to_string(changed) {
            Ok(s) => s,
            Err(e) => {
                io_diagnostic = Some(Diagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: format!("Failed to read '{}': {}", changed, e),
                    file: Some(changed.to_string()),
                    line: None,
                    column: None,
                    help: Some("Check file permissions and that the file exists.".into()),
                    code: DiagnosticCode::IoError,
                });
                String::new()
            }
        };
        let new_data = crate::extractor::parse_file(changed, &source);
        self.source_cache.insert(changed.to_owned(), new_data.clone());

        // 2. Patch GlobalSourceData atomically using rcu (read-copy-update).
        // rcu retries if the pointer was swapped by a concurrent update_file call,
        // preventing lost-update races under parallel file change events.
        let new_global = self.global.rcu(|old| {
            let mut g = (**old).clone();
            g.remove_file(changed);
            g.merge(changed, new_data.clone());
            g
        });

        // 3. Find all transitively affected files via the reverse dep graph.
        let affected = self.reverse_deps.affected(changed);

        // 4. Re-resolve affected components.
        let affected_mappings: Vec<ComponentMapping> =
            new_global.component_mappings.iter().filter(|m| affected.contains(&m.file_path)).cloned().collect();

        let ctx = ResolutionContext::new(new_global.clone(), &self.options);
        let results: Vec<(ComponentEntry, Vec<Diagnostic>)> =
            affected_mappings.par_iter().map(|m| resolve_component(m, &ctx)).collect();

        let mut updated_components = Vec::new();
        let mut diagnostics: Vec<Diagnostic> = io_diagnostic.into_iter().collect();

        for (entry, diags) in results {
            self.component_cache.insert(entry.display_name.clone(), entry.clone());
            updated_components.push(entry);
            diagnostics.extend(diags);
        }

        // Replace (not append) this file's diagnostics — a fixed file's prior
        // errors shouldn't linger in subsequent snapshots.
        if diagnostics.is_empty() {
            self.diagnostics.remove(changed);
        } else {
            self.diagnostics.insert(changed.to_owned(), diagnostics.clone());
        }

        IncrementalUpdate {
            updated_components,
            affected_files: affected.into_iter().collect(),
            diagnostics,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::WatchSession;
    use crate::pipeline::PipelineOptions;

    fn empty_options() -> PipelineOptions {
        PipelineOptions { src_dirs: vec![], ..Default::default() }
    }

    #[test]
    fn initialize_is_idempotent() {
        let session = WatchSession::new(empty_options());
        let first = session.initialize();
        let second = session.initialize();
        assert_eq!(first.components.len(), second.components.len());
        assert_eq!(first.enums.len(), second.enums.len());
    }

    #[test]
    fn concurrent_initialize_no_double_init() {
        // Barrier ensures all 8 threads reach initialize() simultaneously,
        // maximising the race window for the Mutex<bool> guard.
        let session = Arc::new(WatchSession::new(empty_options()));
        let barrier = Arc::new(Barrier::new(8));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let s = Arc::clone(&session);
                let b = Arc::clone(&barrier);
                thread::spawn(move || {
                    b.wait();
                    s.initialize()
                })
            })
            .collect();

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let expected = results[0].components.len();
        assert!(results.iter().all(|r| r.components.len() == expected));
    }

    #[test]
    fn snapshot_after_initialize_with_no_files_has_no_diagnostics() {
        let session = WatchSession::new(empty_options());
        let first = session.initialize();
        assert!(first.diagnostics.is_empty(), "no files to read means nothing to report");
        let snap = session.snapshot();
        assert!(snap.diagnostics.is_empty());
    }

    #[test]
    fn snapshot_surfaces_update_file_diagnostics() {
        let session = WatchSession::new(empty_options());
        let _ = session.initialize();

        let missing = camino::Utf8PathBuf::from("/nonexistent/does-not-exist.tsx");
        let update = session.update_file(&missing);
        assert!(
            update.diagnostics.iter().any(|d| d.code == crate::types::DiagnosticCode::IoError),
            "expected an IoError diagnostic from update_file, got {:?}",
            update.diagnostics
        );

        let snap = session.snapshot();
        assert!(
            !snap.diagnostics.is_empty(),
            "snapshot() should surface diagnostics recorded by update_file, not hardcode empty"
        );
    }

    #[test]
    fn update_file_replaces_previous_diagnostics_for_same_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").join("widget.tsx");

        let session = WatchSession::new(empty_options());
        let _ = session.initialize();

        // File doesn't exist yet — update_file should record an IoError for it.
        session.update_file(&path);
        assert!(!session.snapshot().diagnostics.is_empty());

        // Same path now reads successfully — the stale IoError for this path
        // should be cleared, not accumulated alongside the new (empty) result.
        std::fs::write(path.as_std_path(), "export const Widget = () => null;").expect("write fixture");
        let update = session.update_file(&path);
        assert!(update.diagnostics.is_empty());
        assert!(
            session.snapshot().diagnostics.is_empty(),
            "fixed file's stale diagnostic should be cleared, not linger"
        );
    }
}
