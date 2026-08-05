use std::sync::{Arc, Mutex};
use std::time::Instant;

use arc_swap::ArcSwap;
use camino::{Utf8Path, Utf8PathBuf};
use dashmap::DashMap;
use rayon::prelude::*;

use crate::resolver::{resolve_component, ResolutionContext};
use crate::types::*;

use super::super::types::{ComponentMapping, GlobalSourceData};
use super::ReverseDeps;
use super::{IncrementalUpdate, PipelineOptions};

/// Canonicalize `path`, falling back to canonicalizing its parent directory
/// and rejoining the file name when the full path can't be resolved (e.g. the
/// file doesn't exist yet — created and then immediately edited before this
/// session ever saw it read successfully). Falls back to `path` unchanged only
/// when even the parent doesn't exist. Deliberately stable across a file's
/// create/delete transitions: the parent directory is what actually needs a
/// consistent identity across repeated `update_file` calls for the same file,
/// not the file itself.
fn canonicalize_best_effort(path: &Utf8Path) -> Utf8PathBuf {
    if let Ok(p) = std::fs::canonicalize(path.as_std_path()) {
        if let Ok(p) = Utf8PathBuf::from_path_buf(p) {
            return p;
        }
    }
    if let (Some(parent), Some(file_name)) = (path.parent(), path.file_name()) {
        if let Ok(p) = std::fs::canonicalize(parent.as_std_path()) {
            if let Ok(mut p) = Utf8PathBuf::from_path_buf(p) {
                p.push(file_name);
                return p;
            }
        }
    }
    path.to_owned()
}

/// Stateful session for incremental watch-mode extraction.
///
/// `global`, `reverse_deps`, and `component_cache` are updated atomically /
/// concurrently on each file change. `reverse_deps` is rebuilt in full on
/// structural changes (new file, deleted file) via a new `WatchSession` — it
/// does not track incremental import-graph edits within a session.
pub struct WatchSession {
    pub options: PipelineOptions,
    /// Current merged source data — swapped atomically via `ArcSwap`.
    pub global: ArcSwap<GlobalSourceData>,
    /// Reverse-dependency graph built at initialisation time — swapped
    /// atomically via `ArcSwap` so `initialize()` can populate it from the
    /// first fully-merged `global` (it doesn't exist yet at `new()`).
    pub reverse_deps: ArcSwap<ReverseDeps>,
    /// Per-file SourceData cache (avoids re-reading files that haven't changed).
    pub source_cache: DashMap<Utf8PathBuf, SourceData>,
    /// Latest resolved component entries, keyed by display name.
    pub component_cache: DashMap<String, ComponentEntry>,
    /// Latest diagnostics per file, replaced (not appended) on each re-update so a
    /// fixed file's stale diagnostics don't linger — mirrors GlobalSourceData::remove_file's
    /// per-file replacement semantics.
    pub diagnostics: DashMap<Utf8PathBuf, Vec<Diagnostic>>,
    /// TypeScript's own lib.d.ts paths, computed once here at `initialize()`
    /// rather than by every `ResolutionContext::new` inside `update_file` —
    /// `options.src_dirs` (the only input this depends on) never changes
    /// within a session, so re-walking the filesystem for it on every single
    /// file save would be pure waste.
    ambient_global_files: std::sync::OnceLock<Vec<Utf8PathBuf>>,
    /// Guards initialize() so concurrent callers don't race to build caches.
    initialized: Mutex<bool>,
}

impl WatchSession {
    /// Create an empty WatchSession (call `initialize()` before first use).
    pub fn new(options: PipelineOptions) -> Self {
        Self {
            options,
            global: ArcSwap::new(Arc::new(GlobalSourceData::default())),
            reverse_deps: ArcSwap::new(Arc::new(ReverseDeps { inner: Default::default() })),
            source_cache: DashMap::new(),
            component_cache: DashMap::new(),
            diagnostics: DashMap::new(),
            ambient_global_files: std::sync::OnceLock::new(),
            initialized: Mutex::new(false),
        }
    }

    /// Run a full cold extraction to populate this session's caches.
    ///
    /// Idempotent — concurrent or repeated calls return the existing snapshot
    /// without re-running extraction.
    pub fn initialize(&self) -> ExtractionOutput {
        let mut guard = self.initialized.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if *guard {
            return self.snapshot();
        }

        // Single real pipeline run (discover, parse, merge, Full-mode
        // @types/react, ambient lib.d.ts, resolve) shared with the one-shot
        // extract() path — see extract_with_global's doc comment. Previously
        // this rebuilt GlobalSourceData by hand here, which skipped the
        // Full-mode/ambient-lib.d.ts merges entirely: the cold `output` below
        // was correct (it came from a real extract() call), but the `global`
        // this session persists for every subsequent update_file() was not.
        let (output, global, per_file_data) = super::extract_with_global(&self.options, true);

        for (path, data) in per_file_data {
            self.source_cache.insert(path, data);
        }

        let _ = self.ambient_global_files.set(crate::resolver::compute_ambient_global_files(&self.options));
        self.reverse_deps.store(Arc::new(ReverseDeps::build(&global)));
        self.global.store(global);

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

        // `discover_files`'s directory walk (via the `ignore` crate) resolves
        // symlinks in the paths it reports — e.g. macOS's `/var` -> `/private/var`
        // — so every path already stored in `global`/`reverse_deps` is in that
        // resolved form. A file-watcher-reported `changed` path is not
        // guaranteed to match unless it's canonicalized the same way; without
        // this, `reverse_deps.affected()` below would look up the wrong key and
        // silently find no dependents at all.
        let changed = canonicalize_best_effort(changed);
        let changed = changed.as_path();

        // 1. Re-parse the changed file.
        let mut io_diagnostic: Option<Diagnostic> = None;
        let source = match std::fs::read_to_string(changed) {
            Ok(s) => s,
            Err(e) => {
                io_diagnostic = Some(Diagnostic::io_read_error(changed, &e));
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
        let affected = self.reverse_deps.load().affected(changed);

        // 4. Re-resolve affected components.
        let affected_mappings: Vec<ComponentMapping> =
            new_global.component_mappings.iter().filter(|m| affected.contains(&m.file_path)).cloned().collect();

        let ambient_global_files = self.ambient_global_files.get().cloned().unwrap_or_default();
        let ctx = ResolutionContext::new_with_cached_ambient_global_files(
            new_global.clone(),
            &self.options,
            ambient_global_files,
        );
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
    fn initialize_recovers_from_a_poisoned_lock_instead_of_panicking() {
        let session = WatchSession::new(empty_options());

        // Poison the lock the same way an uncontained panic elsewhere inside
        // initialize() could, before panic containment landed on the call
        // sites initialize() reaches (Task 3/4/5).
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = session.initialized.lock().unwrap();
            panic!("simulated panic while holding the init lock");
        }));
        assert!(poisoned.is_err(), "the panic should have unwound past the lock guard");
        assert!(session.initialized.is_poisoned(), "the lock should now be poisoned");

        // Must recover instead of propagating a second panic via .expect(...).
        let output = session.initialize();
        assert!(output.components.is_empty());
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

    #[test]
    fn update_file_re_resolves_files_that_import_the_changed_file() {
        // Adversarial review finding: ReverseDeps::build was never called, so
        // reverse_deps stayed permanently empty and update_file's BFS
        // dependent-propagation only ever found the changed file itself —
        // never files that import it.
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let base_path = dir.join("Base.tsx");
        let consumer_path = dir.join("Consumer.tsx");

        std::fs::write(base_path.as_std_path(), "export interface BaseProps { label: string; }")
            .expect("write Base.tsx");
        std::fs::write(
            consumer_path.as_std_path(),
            r#"
                import { BaseProps } from './Base';
                interface ConsumerProps extends BaseProps { extra?: boolean; }
                export function Consumer(props: ConsumerProps) { return null; }
            "#,
        )
        .expect("write Consumer.tsx");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        let initial = session.initialize();
        assert!(
            initial.components.contains_key("Consumer"),
            "Consumer should be found on cold init, got {:?}",
            initial.components.keys().collect::<Vec<_>>()
        );

        // Base.tsx itself declares no component — only Consumer.tsx (which
        // imports it) should be re-resolved.
        let update = session.update_file(&base_path);
        let updated_names: Vec<&str> = update.updated_components.iter().map(|c| c.display_name.as_str()).collect();
        assert!(
            updated_names.contains(&"Consumer"),
            "expected Consumer (which imports Base.tsx) to be re-resolved when Base.tsx changes, got {:?}",
            updated_names
        );
    }

    #[test]
    fn update_file_still_resolves_ambient_globals_after_the_first_incremental_change() {
        // Adversarial review finding: WatchSession::initialize() used to rebuild
        // GlobalSourceData by hand, skipping Phase 3.5/3.6 (@types/react
        // Full-mode + ambient lib.d.ts merges) entirely. The cold `initialize()`
        // output was correct (it came from a real extract() call done
        // separately just for that output), but `self.global` — what every
        // subsequent update_file() resolves against — was missing those merges,
        // so the very next edit to the file itself would regress it to Opaque.
        // Uses a tempdir under CARGO_MANIFEST_DIR so the node_modules
        // ancestor-walk reaches this repo's real installed `typescript`
        // package, the same trick pipeline::tests's ambient-global tests use.
        let manifest_dir = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
        let tmp = tempfile::tempdir_in(manifest_dir).expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let foo_path = dir.join("Foo.tsx");

        let source = r#"
            interface FooProps {
              dir?: HTMLDivElement["dir"];
            }
            export function Foo(props: FooProps) { return null; }
        "#;
        std::fs::write(foo_path.as_std_path(), source).expect("write Foo.tsx");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        let _ = session.initialize();

        // Re-save Foo.tsx itself (content unchanged) — this exercises the
        // exact path a real edit takes: re-parse, remove_file + merge back
        // into self.global, then re-resolve against that same self.global.
        let update = session.update_file(&foo_path);
        let foo = update.updated_components.iter().find(|c| c.display_name == "Foo").expect("Foo not re-resolved");
        let dir_prop = foo.props.get("dir").expect("'dir' prop not found");
        assert_eq!(
            dir_prop.prop_type,
            crate::types::PropType::String,
            "expected 'dir' to still resolve to String after the first incremental update, got {:?}",
            dir_prop.prop_type
        );
    }
}
