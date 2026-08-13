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
    /// Fast `(file, name)` interface/type-alias index — mirrors `self.global`'s
    /// incremental per-file patching (`remove_file` + merge on each edit)
    /// rather than being rebuilt from the whole project on every
    /// `update_file` call. `Mutex<Arc<_>>` (not `ArcSwap`, unlike `global`):
    /// `update_file` mutates it in place via `Arc::make_mut` when nothing else
    /// still holds a reference (the common case — no full clone needed), and
    /// falls back to cloning only if a concurrent call is still using the
    /// previous snapshot. `ArcSwap::rcu` can't express "mutate in place when
    /// possible" — its closure always produces a brand new value.
    named_types: Mutex<Arc<crate::named_type_index::NamedTypeIndex>>,
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
            named_types: Mutex::new(Arc::new(crate::named_type_index::NamedTypeIndex::default())),
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
        *self.named_types.lock().unwrap_or_else(|p| p.into_inner()) =
            Arc::new(crate::named_type_index::NamedTypeIndex::build(&global));
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

        // Only `components_extracted` is populated here — the other ExtractionStats
        // fields (files_parsed, dts_cache_hits, duration_ms, tier1/tier3/opaque
        // counts) describe a single extract() run's own bookkeeping, which this
        // session doesn't accumulate across incremental update_file() calls. A
        // consumer reading watch --out's stats block for anything but the
        // component count sees zeros — a known, narrower gap than reporting
        // componentsExtracted itself as 0 regardless of how many components exist.
        let stats = ExtractionStats { components_extracted: components.len() as u32, ..ExtractionStats::default() };

        ExtractionOutput { components, enums, diagnostics, stats }
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
        // rcu's return value is the PRE-swap value (arc-swap's compare_and_swap
        // convention), not the value the closure just produced — the closure's
        // result must be re-read via load_full() to see the post-update state.
        self.global.rcu(|old| {
            let mut g = (**old).clone();
            g.remove_file(changed);
            g.merge(changed, new_data.clone());
            g
        });
        let new_global = self.global.load_full();

        // Keep the named-type index in sync with `global`'s own unconditional
        // per-file patch above (same remove-then-merge shape, same lack of a
        // read-failure gate — this index derives from `global`, not from
        // `component_cache`'s separately-decided AC-019 persistence policy).
        // `Arc::make_mut` mutates in place when this call holds the only
        // reference (the common case), and clones only if a concurrent
        // `update_file` call is still using the previous snapshot.
        let named_types = {
            let mut guard = self.named_types.lock().unwrap_or_else(|p| p.into_inner());
            let index = Arc::make_mut(&mut guard);
            index.remove_file(changed);
            index.merge_file(&new_data);
            guard.clone()
        };

        // 3. Find all transitively affected files via the reverse dep graph.
        let affected = self.reverse_deps.load().affected(changed);

        // 4. Re-resolve affected components.
        // Borrowed, not cloned — same reasoning as the one-shot `extract()`
        // path's identical fix (pipeline/mod.rs): `resolve_component` already
        // takes `&ComponentMapping`, and `.par_iter()` below only ever
        // produces borrows anyway.
        let affected_mappings: Vec<&ComponentMapping> =
            new_global.component_mappings.iter().filter(|m| affected.contains(&m.file_path)).collect();

        let ambient_global_files = self.ambient_global_files.get().cloned().unwrap_or_default();
        let ctx = ResolutionContext::new_with_cached_ambient_global_files(
            new_global.clone(),
            &self.options,
            ambient_global_files,
            named_types,
        );
        // Wrapped in `contain_panic` for the same reason as the one-shot `extract()`
        // path (pipeline/mod.rs) — one component's resolution panicking must degrade
        // to a per-component diagnostic instead of poisoning this `.collect()` and
        // losing every other affected component's re-resolution for this file change.
        let results: Vec<(ComponentEntry, Vec<Diagnostic>)> = affected_mappings
            .par_iter()
            .copied()
            .map(|m| {
                let label = format!("resolve:{}", m.component_name);
                crate::panic_guard::contain_panic(&label, || {
                    #[cfg(test)]
                    if m.component_name == super::RESOLVE_PANIC_TEST_SENTINEL {
                        panic!("simulated resolve-phase panic (test-only sentinel)");
                    }

                    resolve_component(m, &ctx)
                })
                .unwrap_or_else(|diag| {
                    let stub = ComponentEntry {
                        display_name: m.component_name.clone(),
                        file_path: m.file_path.clone(),
                        description: String::new(),
                        props: Default::default(),
                        inheritance: vec![],
                        notable_inherited: Default::default(),
                        discriminant_prop: None,
                        composes: vec![],
                        tags: Default::default(),
                        methods: vec![],
                    };
                    (stub, vec![diag])
                })
            })
            .collect();

        // A failed read (deletion, permission error, non-UTF8 content) must
        // NOT evict this file's cache entries — SPEC-PIPELINE-001's AC-019
        // deliberately specifies that a deleted file's previously-resolved
        // component stays in component_cache/snapshot() until a new
        // WatchSession is constructed, a documented (not accidental)
        // staleness gap distinct from the rename case below. Only a
        // successful re-read, which can genuinely produce a different set of
        // component names for this file, should evict what it no longer owns.
        let file_read_failed = io_diagnostic.is_some();

        let mut updated_components = Vec::new();
        let mut diagnostics: Vec<Diagnostic> = io_diagnostic.into_iter().collect();

        // Evict every existing component_cache entry that belonged to this
        // file — by whatever key it was stored under, bare or disambiguated —
        // before inserting its freshly-resolved entries. A bare `insert(
        // entry.display_name, ...)` here left two kinds of stale entries
        // behind forever: a renamed component's old-name key never got
        // removed, and a disambiguated key from initialize()'s cold Phase 5
        // pass was never touched by an update that only ever wrote the bare
        // key. Both traced to the same root cause — no eviction step existed
        // at all. This resolves the file's keys as a batch first so a
        // same-file rename doesn't race against its own insert below.
        if !file_read_failed {
            let stale_keys: Vec<String> = self
                .component_cache
                .iter()
                .filter(|r| r.value().file_path == changed)
                .map(|r| r.key().clone())
                .collect();
            for key in stale_keys {
                self.component_cache.remove(&key);
            }
        }

        for (entry, diags) in results {
            // Disambiguate against whatever's left in the cache after this
            // file's own stale keys were just evicted above — mirrors Phase
            // 5's bare-then-disambiguated rule (extract()'s one-shot path),
            // checked incrementally instead of via a session-wide counter.
            // Deliberately narrower than Phase 5 in one respect: if a rename
            // elsewhere frees up a bare key another file's component was
            // disambiguated against earlier, that other entry stays under
            // its disambiguated key until its own file is next touched —
            // still correctly retrievable, just not repacked to the
            // now-available bare key. Re-optimizing every other cached
            // entry's key on every single-file edit would defeat the point
            // of incremental updates; only this file's own stale keys are
            // this method's responsibility.
            let key = if self.component_cache.iter().any(|r| r.value().display_name == entry.display_name) {
                format!("{} ({})", entry.display_name, entry.file_path)
            } else {
                entry.display_name.clone()
            };
            self.component_cache.insert(key, entry.clone());
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

    use camino::Utf8PathBuf;

    use super::WatchSession;
    use crate::pipeline::PipelineOptions;

    // Points at a real, empty directory rather than an empty `src_dirs` list —
    // the latter is now its own error case (see
    // pipeline::tests::test_extract_empty_src_dirs_produces_diagnostic), distinct
    // from "a configured directory that happens to contain no files".
    fn empty_options() -> PipelineOptions {
        let dir = tempfile::TempDir::new().unwrap().keep();
        PipelineOptions { src_dirs: vec![Utf8PathBuf::from_path_buf(dir).unwrap()], ..Default::default() }
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
    fn snapshot_stats_components_extracted_reflects_the_real_component_count() {
        // Found while validating SPEC-CLI-001b's AC-5: snapshot() always
        // returned ExtractionStats::default(), so componentsExtracted read 0
        // no matter how many components a watch session actually had.
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        std::fs::write(dir.join("A.tsx").as_std_path(), "export function A(props: { x: string }) { return null; }\n")
            .expect("write A.tsx");
        std::fs::write(dir.join("B.tsx").as_std_path(), "export function B(props: { y: string }) { return null; }\n")
            .expect("write B.tsx");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        let _ = session.initialize();

        let snap = session.snapshot();
        assert_eq!(snap.components.len(), 2);
        assert_eq!(
            snap.stats.components_extracted, 2,
            "stats.components_extracted should match the real component count, got {:?}",
            snap.stats
        );
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
    fn a_panic_during_incremental_resolve_degrades_to_a_diagnostic_not_a_crash() {
        // Found while grounding SPEC-PIPELINE-001 in real source: extract()'s
        // one-shot resolve phase (pipeline/mod.rs) was wrapped in
        // `contain_panic`, but this incremental path was not — one component
        // panicking during an editor keystroke would have crashed the whole
        // watch session instead of degrading to a diagnostic for that file.
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let button_path = dir.join("Button.tsx");
        let other_path = dir.join("Other.tsx");

        std::fs::write(
            button_path.as_std_path(),
            "export function Button(props: { label: string }) { return null; }\n",
        )
        .expect("write Button.tsx");
        std::fs::write(
            other_path.as_std_path(),
            format!(
                "export function {}(props: {{ label: string }}) {{ return null; }}\n",
                super::super::RESOLVE_PANIC_TEST_SENTINEL
            ),
        )
        .expect("write Other.tsx");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        let _ = session.initialize();

        // Must not panic the test process — that's the whole point.
        let update = session.update_file(&other_path);

        assert!(
            update.diagnostics.iter().any(|d| d.code == crate::types::DiagnosticCode::InternalPanic),
            "expected an InternalPanic diagnostic, got {:?}",
            update.diagnostics
        );
    }

    #[test]
    fn update_file_reflects_the_edited_files_new_content_not_its_pre_edit_content() {
        // Found while validating SPEC-PIPELINE-001: ArcSwap::rcu returns the
        // PRE-swap value (arc-swap's compare_and_swap convention), not the value
        // the closure just produced. `new_global` was bound directly to rcu's
        // return, so every incremental update resolved against the file's
        // content from BEFORE this edit — watch mode was permanently one edit
        // behind. A test asserting only component *identity* (name present) is
        // invariant under this bug; this test asserts prop *content* instead.
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let path = dir.join("Widget.tsx");

        std::fs::write(
            path.as_std_path(),
            "export interface WidgetProps { label: string; }\nexport function Widget(props: WidgetProps) { return null; }\n",
        )
        .expect("write initial fixture");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        let initial = session.initialize();
        assert!(
            initial.components.get("Widget").is_some_and(|c| c.props.contains_key("label")),
            "expected initial extraction to see 'label', got {:?}",
            initial.components.get("Widget")
        );

        std::fs::write(
            path.as_std_path(),
            "export interface WidgetProps { title: string; }\nexport function Widget(props: WidgetProps) { return null; }\n",
        )
        .expect("write edited fixture");

        let update = session.update_file(&path);
        let widget = update.updated_components.iter().find(|c| c.display_name == "Widget");
        assert!(
            widget.is_some_and(|c| c.props.contains_key("title") && !c.props.contains_key("label")),
            "expected update_file to resolve against the EDITED content ('title'), not the pre-edit content ('label'); got {:?}",
            widget
        );

        let snapshot = session.snapshot();
        let widget = snapshot.components.get("Widget");
        assert!(
            widget.is_some_and(|c| c.props.contains_key("title") && !c.props.contains_key("label")),
            "expected snapshot() to reflect the edited content too, got {:?}",
            widget
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

    // ── SPEC-PIPELINE-001 AC-014: initialize() called twice does not re-run
    // extraction — a plugin hook counter is unchanged on the second call, and
    // the second call's diagnostics are empty regardless of the first call's.

    #[test]
    fn initialize_called_twice_does_not_rerun_extraction_or_replay_diagnostics() {
        use crate::plugin::{DocgenPlugin, PluginRegistry};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc as StdArc;

        struct CountingPlugin(StdArc<AtomicUsize>);
        impl DocgenPlugin for CountingPlugin {
            fn name(&self) -> &str {
                "counting-plugin"
            }
            fn on_file_extracted(&self, _path: &camino::Utf8Path, _data: &mut crate::types::SourceData) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        std::fs::write(dir.join("Button.tsx").as_std_path(), "export const Button = () => null;")
            .expect("write Button.tsx");

        let count = StdArc::new(AtomicUsize::new(0));
        let mut plugins = PluginRegistry::new();
        plugins.register(CountingPlugin(count.clone()));

        let options = PipelineOptions { src_dirs: vec![dir], plugins, ..Default::default() };
        let session = WatchSession::new(options);

        let first = session.initialize();
        let count_after_first = count.load(Ordering::SeqCst);
        assert!(count_after_first > 0, "expected the plugin hook to have run at least once");

        let second = session.initialize();
        assert_eq!(
            count.load(Ordering::SeqCst),
            count_after_first,
            "second initialize() call must not re-parse/re-extract"
        );
        assert_eq!(second.components.len(), first.components.len());
        assert!(
            second.diagnostics.is_empty(),
            "second call's diagnostics must be empty regardless of the first call's, got {:?}",
            second.diagnostics
        );
    }

    // ── SPEC-PIPELINE-001 AC-016b: after a contained panic in one
    // update_file() call, a subsequent call on a different, unrelated file
    // still returns a normal IncrementalUpdate — session state isn't
    // corrupted or deadlocked.

    #[test]
    fn session_recovers_after_a_contained_panic_and_serves_unrelated_files_normally() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let panicking_path = dir.join("Boom.tsx");
        let other_path = dir.join("Other.tsx");

        std::fs::write(
            panicking_path.as_std_path(),
            format!(
                "export function {}(props: {{ label: string }}) {{ return null; }}\n",
                super::super::RESOLVE_PANIC_TEST_SENTINEL
            ),
        )
        .expect("write Boom.tsx");
        std::fs::write(other_path.as_std_path(), "export function Other(props: { x: string }) { return null; }\n")
            .expect("write Other.tsx");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        let _ = session.initialize();

        // Must not panic the test process.
        let boom_update = session.update_file(&panicking_path);
        assert!(boom_update.diagnostics.iter().any(|d| d.code == crate::types::DiagnosticCode::InternalPanic));

        // Session must still be usable afterward.
        std::fs::write(
            other_path.as_std_path(),
            "export function Other(props: { x: string; y: string }) { return null; }\n",
        )
        .expect("rewrite Other.tsx");
        let other_update = session.update_file(&other_path);
        let other = other_update.updated_components.iter().find(|c| c.display_name == "Other");
        assert!(
            other.is_some(),
            "session should still serve unrelated files normally after a contained panic, got {:?}",
            other_update.updated_components
        );
        // Content, not presence: "Other" existed before the rewrite too, so checking
        // only its name survives is invariant under the exact rcu stale-global bug
        // this session guards against (the panic-containment path resolving against
        // pre-edit content would still produce an "Other" component — just the old
        // one, missing "y"). Checking the new prop proves this call resolved against
        // the just-rewritten content, not a stale snapshot from before the panic.
        assert!(
            other.unwrap().props.contains_key("y"),
            "expected the rewritten 'y' prop to be present, proving this update resolved against the new \
             content rather than a stale pre-rewrite snapshot, got props {:?}",
            other.unwrap().props.keys().collect::<Vec<_>>()
        );
    }

    // ── SPEC-PIPELINE-001 AC-017: update_file() with a non-canonical path
    // (e.g. a symlinked directory segment reported unresolved) still finds
    // the canonical entry via reverse_deps.
    //
    // A prior version of this test only asserted that the CHANGED file's own
    // component reappeared after the update — but `ReverseDeps::affected()`
    // seeds its BFS with `changed` itself (mod.rs's `affected()` pushes
    // `changed` onto the queue before ever consulting the reverse-dep map),
    // so the changed file is *always* present in the affected set regardless
    // of whether it was canonicalized correctly. That assertion passed even
    // with `canonicalize_best_effort` deleted entirely, because `merge()`
    // also stores the new data under whatever path `changed` happens to be —
    // canonical or not — so the changed file's own mapping and the (trivial,
    // self-seeded) affected set always matched. `canonicalize_best_effort`
    // only matters for a file that TRANSITIVELY IMPORTS the changed file:
    // `reverse_deps.inner` is keyed by the canonical paths `discover_files`
    // reported at `initialize()` time, so looking it up with a non-canonical
    // key finds nothing. This version adds a second file that imports from
    // the symlinked one, so the assertion can actually fail without the fix.

    #[test]
    fn update_file_with_a_non_canonical_path_still_finds_the_canonical_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let real_dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").join("real");
        std::fs::create_dir_all(real_dir.as_std_path()).expect("create real dir");
        let file_path = real_dir.join("Button.tsx");
        std::fs::write(
            file_path.as_std_path(),
            "export interface ButtonProps { label: string; }\n\
             export function Button(props: ButtonProps) { return null; }\n",
        )
        .expect("write Button.tsx");

        // A second file whose component's props come from Button.tsx via a
        // relative import — this is the file that must be re-resolved as a
        // TRANSITIVE dependent of the (non-canonically-referenced) change,
        // which only happens if `canonicalize_best_effort` translates the
        // symlinked path back to the canonical one before the reverse-dep
        // lookup.
        let consumer_path = real_dir.join("Consumer.tsx");
        std::fs::write(
            consumer_path.as_std_path(),
            "import { ButtonProps } from './Button';\n\
             export function Consumer(props: ButtonProps) { return null; }\n",
        )
        .expect("write Consumer.tsx");

        let link_dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(real_dir.as_std_path(), link_dir.as_std_path()).expect("create symlink");
        #[cfg(not(unix))]
        {
            // No portable symlink API on this platform — skip rather than fail.
            return;
        }

        let options = PipelineOptions { src_dirs: vec![real_dir], ..Default::default() };
        let session = WatchSession::new(options);
        let _ = session.initialize();

        // Reference the changed file via its non-canonical (symlinked) path.
        let non_canonical_path = link_dir.join("Button.tsx");
        let update = session.update_file(&non_canonical_path);

        assert!(
            update.updated_components.iter().any(|c| c.display_name == "Button"),
            "a non-canonical path should still resolve to the canonical entry, got {:?}",
            update.updated_components
        );
        assert!(
            update.updated_components.iter().any(|c| c.display_name == "Consumer"),
            "Consumer.tsx transitively imports from the non-canonically-referenced file and must also be \
             re-resolved — this fails without canonicalize_best_effort, since reverse_deps is keyed by the \
             canonical path discover_files reported at initialize() time, got {:?}",
            update.updated_components
        );
    }

    // ── SPEC-PIPELINE-001 AC-020: snapshot()/update_file() on a session whose
    // initialize() was never called must not panic.

    #[test]
    fn snapshot_and_update_file_on_an_uninitialized_session_do_not_panic() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let file_path = dir.join("Widget.tsx");
        // onClick's type requires resolving React's own ambient HTMLAttributes-family
        // types — only wired up by initialize()'s ambient_global_files computation,
        // never called here — proving the cross-file/ambient-resolution degrade half
        // of AC-020, not just "doesn't panic".
        std::fs::write(
            file_path.as_std_path(),
            "export function Widget(props: { onClick: React.MouseEventHandler<HTMLButtonElement> }) { return null; }\n",
        )
        .expect("write Widget.tsx");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        // initialize() deliberately never called.

        let snap = session.snapshot();
        assert!(snap.components.is_empty(), "expected zero components, got {:?}", snap.components);
        assert!(snap.enums.is_empty());
        assert!(snap.diagnostics.is_empty());

        let update = session.update_file(&file_path);
        let widget = update.updated_components.iter().find(|c| c.display_name == "Widget");
        assert!(
            widget.is_some(),
            "expected the just-parsed file's own component present, got {:?}",
            update.updated_components
        );
    }

    // ── SPEC-PIPELINE-001 AC-015: update_file() after an editor's atomic
    // replace (write to temp, rename over the original) reflects the new
    // content — the session must not misinterpret it as a deletion.

    #[test]
    fn update_file_after_an_atomic_replace_reflects_the_new_content_not_a_deletion() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let file_path = dir.join("Widget.tsx");
        std::fs::write(file_path.as_std_path(), "export function Widget(props: { a: string }) { return null; }\n")
            .expect("write Widget.tsx");

        let options = PipelineOptions { src_dirs: vec![dir.clone()], ..Default::default() };
        let session = WatchSession::new(options);
        let _ = session.initialize();

        // Atomic replace: write the new content to a same-directory temp file,
        // then rename it over the original path — the pattern editors (and
        // write_atomic itself) use, distinct from an in-place truncate+write.
        let tmp_file = dir.join(".Widget.tsx.tmp");
        std::fs::write(tmp_file.as_std_path(), "export function Widget(props: { b: string }) { return null; }\n")
            .expect("write temp file");
        std::fs::rename(tmp_file.as_std_path(), file_path.as_std_path()).expect("atomic rename over original");

        let update = session.update_file(&file_path);
        let widget = update.updated_components.iter().find(|c| c.display_name == "Widget");
        assert!(
            widget.is_some(),
            "expected Widget still present after the atomic replace, got {:?}",
            update.updated_components
        );
        assert!(
            widget.unwrap().props.contains_key("b"),
            "expected the replaced content's 'b' prop, not the pre-replace 'a' — a rename-based edit must not \
             be misread as a deletion, got props {:?}",
            widget.unwrap().props.keys().collect::<Vec<_>>()
        );

        let snap = session.snapshot();
        let widget_snap = snap.components.get("Widget").expect("expected Widget in snapshot after replace");
        assert!(widget_snap.props.contains_key("b"), "snapshot should also reflect the replaced content");
    }

    // ── SPEC-PIPELINE-001 AC-019: deleting a file surfaces an IoError for
    // that file, its own stale components linger until a new session (a
    // documented gap), and a dependent's re-resolution no longer resolves
    // against the deleted file's declaration.

    #[test]
    fn deleting_a_file_surfaces_io_error_and_a_dependent_stops_resolving_against_it() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let f_path = dir.join("Base.tsx");
        std::fs::write(
            f_path.as_std_path(),
            "export interface BaseProps { x: string; }\nexport function Base(props: BaseProps) { return null; }\n",
        )
        .expect("write Base.tsx");
        let g_path = dir.join("Consumer.tsx");
        std::fs::write(
            g_path.as_std_path(),
            "import { BaseProps } from './Base';\nexport function Consumer(props: BaseProps) { return null; }\n",
        )
        .expect("write Consumer.tsx");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        let _ = session.initialize();

        // Sanity: Consumer resolves BaseProps' 'x' field before the deletion.
        let snap_before = session.snapshot();
        let consumer_before = snap_before.components.get("Consumer").expect("expected Consumer before deletion");
        assert!(
            consumer_before.props.contains_key("x"),
            "sanity check: Consumer should resolve 'x' via Base before deletion"
        );

        std::fs::remove_file(f_path.as_std_path()).expect("delete Base.tsx");
        let f_update = session.update_file(&f_path);
        assert!(
            f_update.diagnostics.iter().any(|d| d.code == crate::types::DiagnosticCode::IoError),
            "expected an IoError diagnostic for the deleted file, got {:?}",
            f_update.diagnostics
        );

        // Base's own stale component lingers in the snapshot — documented gap,
        // not pruned until a new WatchSession is constructed.
        let snap_after_delete = session.snapshot();
        assert!(
            snap_after_delete.components.contains_key("Base"),
            "Base's stale component should still be present (the documented staleness gap), got {:?}",
            snap_after_delete.components.keys().collect::<Vec<_>>()
        );

        // Consumer's re-resolution must no longer resolve as if Base still existed:
        // either a diagnostic naming the unresolvable reference, or an opaque/absent 'x' prop.
        let g_update = session.update_file(&g_path);
        let consumer_after = g_update
            .updated_components
            .iter()
            .find(|c| c.display_name == "Consumer")
            .expect("expected Consumer re-resolved");
        let x_still_resolved =
            matches!(consumer_after.props.get("x").map(|p| &p.prop_type), Some(crate::types::PropType::String));
        assert!(
            !x_still_resolved,
            "Consumer's 'x' prop must not still resolve to String as if Base's declaration still existed, got {:?}",
            consumer_after.props.get("x")
        );
    }

    // ── SPEC-PIPELINE-001 AC-024: update_file() on a file whose on-disk
    // content is not valid UTF-8 (distinct from AC-005's non-UTF8 *path*)
    // surfaces an IoError and does not re-resolve that file's own component.

    #[test]
    fn update_file_with_non_utf8_content_surfaces_io_error_and_skips_that_files_own_component() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let file_path = dir.join("Bad.tsx");
        std::fs::write(file_path.as_std_path(), "export function Bad(props: { x: string }) { return null; }\n")
            .expect("write Bad.tsx");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        let _ = session.initialize();

        // Overwrite with invalid UTF-8 bytes directly (std::fs::write accepts any
        // AsRef<[u8]>, unlike a &str write, which couldn't express this at all).
        std::fs::write(file_path.as_std_path(), [0x42, 0xFF, 0xFE, 0x00, 0xFF]).expect("write invalid utf8 bytes");

        let update = session.update_file(&file_path);
        assert!(
            update
                .diagnostics
                .iter()
                .any(|d| d.code == crate::types::DiagnosticCode::IoError && d.message.contains("Bad.tsx")),
            "expected an IoError diagnostic naming Bad.tsx, got {:?}",
            update.diagnostics
        );
        assert!(
            !update.updated_components.iter().any(|c| c.file_path == file_path),
            "the changed file's own component must not be re-resolved successfully after an unreadable-content \
             error, got {:?}",
            update.updated_components
        );
    }

    // ── component_cache staleness, manifestation 1: a component renamed
    // between edits (same file) left its OLD name's key behind forever,
    // since the old code only ever inserted under the new display_name,
    // never removing whatever key the file previously held.

    #[test]
    fn renaming_a_component_between_edits_evicts_its_old_cache_key() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let file_path = dir.join("Widget.tsx");
        std::fs::write(file_path.as_std_path(), "export function Card(props: { label: string }) { return null; }\n")
            .expect("write Widget.tsx");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        let _ = session.initialize();
        assert!(
            session.component_cache.get("Card").is_some(),
            "sanity check: Card should be cached after initialize()"
        );

        // Rename Card -> Panel within the same file.
        std::fs::write(file_path.as_std_path(), "export function Panel(props: { label: string }) { return null; }\n")
            .expect("rewrite Widget.tsx");
        let _ = session.update_file(&file_path);

        assert!(
            session.component_cache.get("Panel").is_some(),
            "expected the renamed component to be cached under its new name"
        );
        assert!(
            session.component_cache.get("Card").is_none(),
            "expected the old name's cache key to be evicted after the rename, but it's still present"
        );
    }

    // ── component_cache staleness, manifestation 2 (the more severe one): a
    // component that was disambiguated at cold initialize() time (because
    // another file already held its bare display_name) gets its content
    // silently corrupted on the next edit — the old code always wrote to the
    // BARE key regardless of which key the component actually held, so an
    // edited B.tsx's fresh content landed in A.tsx's bare-keyed cache entry,
    // while B's own disambiguated key kept A's — no, B's own — STALE
    // pre-edit content forever.

    #[test]
    fn editing_a_disambiguated_component_updates_its_own_key_not_the_bare_one() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = camino::Utf8Path::from_path(tmp.path()).expect("utf8 path").to_owned();
        let a_path = dir.join("A.tsx");
        let b_path = dir.join("B.tsx");
        // Both files declare a component named "Button" — A sorts first
        // lexically, so Phase 5 gives A the bare key and B the disambiguated one.
        std::fs::write(a_path.as_std_path(), "export function Button(props: { a: string }) { return null; }\n")
            .expect("write A.tsx");
        std::fs::write(b_path.as_std_path(), "export function Button(props: { b: string }) { return null; }\n")
            .expect("write B.tsx");
        // Canonicalize before comparing — on macOS the tempdir's own path
        // (/var/...) differs from its canonical form (/private/var/...),
        // same class of thing canonicalize_best_effort exists to handle, and
        // discover_files canonicalizes every path it reports at cold-extract
        // time, so the CACHED file_path is always the canonical form.
        let a_path = a_path.canonicalize_utf8().expect("canonicalize A.tsx");
        let b_path = b_path.canonicalize_utf8().expect("canonicalize B.tsx");

        let options = PipelineOptions { src_dirs: vec![dir], ..Default::default() };
        let session = WatchSession::new(options);
        let _ = session.initialize();

        let bare = session.component_cache.get("Button").expect("expected a bare 'Button' entry after initialize()");
        assert_eq!(bare.file_path, a_path, "expected A.tsx to hold the bare key (sorts first)");
        drop(bare);
        let disambiguated_key = format!("Button ({b_path})");
        assert!(
            session.component_cache.get(&disambiguated_key).is_some(),
            "expected B.tsx's Button to be disambiguated, got keys {:?}",
            session.component_cache.iter().map(|r| r.key().clone()).collect::<Vec<_>>()
        );

        // Edit B.tsx only — A.tsx is untouched.
        std::fs::write(
            b_path.as_std_path(),
            "export function Button(props: { b: string; c: string }) { return null; }\n",
        )
        .expect("rewrite B.tsx");
        let _ = session.update_file(&b_path);

        // A's bare entry must be untouched by B's edit.
        let bare_after = session.component_cache.get("Button").expect("expected the bare 'Button' entry to survive");
        assert_eq!(bare_after.file_path, a_path, "A's bare entry must not be overwritten by B's edit");
        assert!(bare_after.props.contains_key("a"), "A's own props must be unaffected by B's edit");

        // B's disambiguated entry must reflect the NEW content, not linger stale.
        let b_after = session
            .component_cache
            .get(&disambiguated_key)
            .expect("expected B's disambiguated entry to still exist under the same key");
        assert!(
            b_after.props.contains_key("c"),
            "expected B's disambiguated entry to reflect the edit, got props {:?}",
            b_after.props.keys().collect::<Vec<_>>()
        );
    }
}
