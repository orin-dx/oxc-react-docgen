//! DTS cache — persists parsed `SourceData` across runs to avoid re-parsing unchanged .d.ts files.
//!
//! Cache validity is based on file size + mtime (nanosecond resolution).
//! Schema version bumps automatically invalidate the entire cache.
//!
//! The cache never panics — all I/O errors are silently swallowed and the cache
//! degrades gracefully to an empty state.

use camino::{Utf8Path, Utf8PathBuf};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::types::SourceData;

/// Bump this whenever `SourceData`'s field list or field order changes.
/// Serialization is via plain `rmp_serde::to_vec`/`from_slice` — MessagePack's
/// *positional* (not named-map) struct encoding — so inserting a field
/// anywhere but the very end shifts every subsequent field's decode position
/// for any cache entry written before the change. A same-shaped field
/// addition (e.g. another `FxHashMap<String, Vec<X>>`) wouldn't even fail
/// loudly; it would decode as plausible-looking but wrong data. `const_arrays`
/// (added mid-struct, not appended) is exactly this case — this bump covers it.
const CACHE_SCHEMA_VERSION: u32 = 2;

// ─── Internal key type ────────────────────────────────────────────────────────

#[derive(Hash, Eq, PartialEq, Clone)]
struct CacheKey {
    path: Utf8PathBuf,
    size: u64,
    mtime_ns: u128,
}

// ─── Serializable surrogate (rmp-serde doesn't support u128) ─────────────────

#[derive(Serialize, Deserialize)]
struct SerializableCacheKey {
    path: String,
    size: u64,
    /// Truncated to u64 nanoseconds — sufficient for several decades.
    mtime_ns: u64,
}

impl From<&CacheKey> for SerializableCacheKey {
    fn from(k: &CacheKey) -> Self {
        Self { path: k.path.to_string(), size: k.size, mtime_ns: k.mtime_ns as u64 }
    }
}

impl From<SerializableCacheKey> for CacheKey {
    fn from(k: SerializableCacheKey) -> Self {
        Self { path: Utf8PathBuf::from(k.path), size: k.size, mtime_ns: k.mtime_ns as u128 }
    }
}

// ─── DtsCache ────────────────────────────────────────────────────────────────

/// Maximum number of cached `.d.ts` entries allowed in memory/on disk.
const MAX_CACHE_ENTRIES: usize = 5000;

/// Thread-safe DTS parse-result cache.
///
/// Keyed by `(path, size, mtime_ns)` — if the file hasn't changed, the cached
/// `SourceData` is returned directly without re-parsing.
pub struct DtsCache {
    store: DashMap<CacheKey, SourceData>,
    cache_dir: Utf8PathBuf,
    dirty: std::sync::atomic::AtomicBool,
}

impl DtsCache {
    /// Load cache from disk.
    ///
    /// On any error (missing dir, schema mismatch, corrupt data) the cache
    /// starts empty — subsequent `insert` calls will repopulate it.
    ///
    /// `cache_dir` defaults to `node_modules/.cache/oxc-react-docgen/` relative
    /// to the current working directory.
    pub fn load_from_disk(cache_dir: Option<&Utf8Path>) -> Self {
        let dir = cache_dir
            .map(|p| p.to_owned())
            .unwrap_or_else(|| Utf8PathBuf::from("node_modules/.cache/oxc-react-docgen"));

        let store = Self::try_load(&dir).unwrap_or_default();
        Self { store, cache_dir: dir, dirty: std::sync::atomic::AtomicBool::new(false) }
    }

    fn try_load(dir: &Utf8Path) -> Option<DashMap<CacheKey, SourceData>> {
        // Check schema version via manifest first.
        let manifest_path = dir.join("manifest.json");
        let manifest_bytes = std::fs::read_to_string(manifest_path.as_std_path()).ok()?;
        let manifest: serde_json::Value = serde_json::from_str(&manifest_bytes).ok()?;

        let schema = manifest["schema"].as_u64()?;
        if schema != CACHE_SCHEMA_VERSION as u64 {
            // Schema changed — discard entire cache.
            return None;
        }

        let data_path = dir.join("dts-v1.msgpack");
        let bytes = std::fs::read(data_path.as_std_path()).ok()?;

        let entries: Vec<(SerializableCacheKey, SourceData)> = rmp_serde::from_slice(&bytes).ok()?;

        let map = DashMap::with_capacity(entries.len());
        for (k, v) in entries {
            map.insert(CacheKey::from(k), v);
        }
        Some(map)
    }

    /// Look up cached `SourceData` for `path`.
    ///
    /// Returns `None` if the file is not cached or its mtime/size has changed.
    pub fn get(&self, path: &Utf8Path) -> Option<SourceData> {
        let key = self.key_for(path)?;
        self.store.get(&key).map(|v| v.clone())
    }

    /// Insert `data` into the cache, keyed by `path`'s current mtime + size.
    ///
    /// Silently does nothing if the file metadata cannot be read.
    pub fn insert(&self, path: &Utf8Path, data: SourceData) {
        let Some(key) = self.key_for(path) else { return };
        // Evict extra entries if cache size exceeds MAX_CACHE_ENTRIES
        if self.store.len() >= MAX_CACHE_ENTRIES {
            let keys_to_evict: Vec<CacheKey> = self.store.iter().take(100).map(|r| r.key().clone()).collect();
            for k in keys_to_evict {
                self.store.remove(&k);
            }
        }
        self.store.insert(key, data);
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Persist the cache to disk atomically (temp file + rename).
    ///
    /// Skips writing if the cache is clean (no inserts since load).
    /// Never panics — all errors are silently ignored.
    pub fn save_to_disk(&self) {
        if !self.dirty.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let _ = self.try_save();
    }

    fn try_save(&self) -> Option<()> {
        std::fs::create_dir_all(self.cache_dir.as_std_path()).ok()?;

        // Collect into a serializable form.
        let entries: Vec<(SerializableCacheKey, SourceData)> =
            self.store.iter().map(|r| (SerializableCacheKey::from(r.key()), r.value().clone())).collect();

        let bytes = rmp_serde::to_vec(&entries).ok()?;

        // Atomic write: write to tmp then rename (avoids half-written files on crash).
        let tmp_path = self.cache_dir.join("dts-v1.msgpack.tmp");
        let final_path = self.cache_dir.join("dts-v1.msgpack");
        std::fs::write(tmp_path.as_std_path(), &bytes).ok()?;
        std::fs::rename(tmp_path.as_std_path(), final_path.as_std_path()).ok()?;

        // Write manifest (schema version + entry count for quick inspection).
        let manifest = serde_json::json!({
            "schema": CACHE_SCHEMA_VERSION,
            "entry_count": entries.len(),
        });
        let manifest_json = serde_json::to_string_pretty(&manifest).ok()?;
        std::fs::write(self.cache_dir.join("manifest.json").as_std_path(), manifest_json.as_bytes()).ok()?;

        self.dirty.store(false, std::sync::atomic::Ordering::Relaxed);
        Some(())
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn key_for(&self, path: &Utf8Path) -> Option<CacheKey> {
        let meta = std::fs::metadata(path.as_std_path()).ok()?;
        let mtime_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        Some(CacheKey { path: path.to_owned(), size: meta.len(), mtime_ns })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SourceData;
    use camino::Utf8Path;
    use std::io::Write;

    /// Create a unique temp directory under std::env::temp_dir().
    fn temp_dir(suffix: &str) -> Utf8PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let base = std::env::temp_dir();
        let dir = base.join(format!("oxc-docgen-cache-test-{}-{}", suffix, ts));
        std::fs::create_dir_all(&dir).unwrap();
        Utf8PathBuf::from_path_buf(dir).unwrap()
    }

    fn make_temp_file(dir: &Utf8PathBuf, name: &str, content: &[u8]) -> Utf8PathBuf {
        let path = dir.as_std_path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
        Utf8PathBuf::from_path_buf(path).unwrap()
    }

    #[test]
    fn test_get_miss_on_empty_cache() {
        let tmp = temp_dir("miss");
        let cache_dir = tmp.join("cache");
        let cache = DtsCache::load_from_disk(Some(&cache_dir));

        let file_path = make_temp_file(&tmp, "foo.d.ts", b"export {}");
        assert!(cache.get(Utf8Path::new(file_path.as_str())).is_none());
    }

    #[test]
    fn test_insert_and_get() {
        let tmp = temp_dir("insert");
        let cache_dir = tmp.join("cache");
        let cache = DtsCache::load_from_disk(Some(&cache_dir));

        let file_path = make_temp_file(&tmp, "bar.d.ts", b"export type Foo = string;");
        let data = SourceData::default();

        cache.insert(&file_path, data.clone());
        let retrieved = cache.get(&file_path);
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_save_and_reload() {
        let tmp = temp_dir("reload");
        let cache_dir = tmp.join("cache");

        let file_path = make_temp_file(&tmp, "baz.d.ts", b"export type X = number;");

        {
            let cache = DtsCache::load_from_disk(Some(&cache_dir));
            cache.insert(&file_path, SourceData::default());
            cache.save_to_disk();
        }

        // Check manifest was written.
        let manifest_path = cache_dir.join("manifest.json");
        let manifest_content = std::fs::read_to_string(manifest_path.as_std_path()).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
        assert_eq!(manifest["schema"].as_u64().unwrap(), CACHE_SCHEMA_VERSION as u64);
        assert_eq!(manifest["entry_count"].as_u64().unwrap(), 1);

        // Reload and verify entry is present.
        let cache2 = DtsCache::load_from_disk(Some(&cache_dir));
        assert!(cache2.get(&file_path).is_some());
    }

    #[test]
    fn test_schema_version_mismatch_returns_empty() {
        let tmp = temp_dir("schema");
        let cache_dir = tmp.join("cache");
        std::fs::create_dir_all(cache_dir.as_std_path()).unwrap();

        // Write a manifest with a different schema version.
        let bad_manifest = serde_json::json!({ "schema": 999u64, "entry_count": 0u64 });
        std::fs::write(
            cache_dir.join("manifest.json").as_std_path(),
            serde_json::to_string(&bad_manifest).unwrap().as_bytes(),
        )
        .unwrap();

        let cache = DtsCache::load_from_disk(Some(&cache_dir));
        assert!(cache.store.is_empty());
    }

    #[test]
    fn test_stale_key_after_content_change() {
        let tmp = temp_dir("stale");
        let cache_dir = tmp.join("cache");
        let cache = DtsCache::load_from_disk(Some(&cache_dir));

        let file_path = make_temp_file(&tmp, "changing.d.ts", b"v1");
        cache.insert(&file_path, SourceData::default());

        // Overwrite file with different content — size changes.
        std::fs::write(file_path.as_std_path(), b"v2 with more content here").unwrap();

        // The key has changed (different size), so cache miss.
        assert!(cache.get(&file_path).is_none());
    }

    #[test]
    fn test_load_from_disk_no_dir_returns_empty() {
        // A nonexistent cache dir should just give an empty cache without panicking.
        let cache = DtsCache::load_from_disk(Some(Utf8Path::new("/nonexistent/cache/dir")));
        assert!(cache.store.is_empty());
    }

    #[test]
    fn test_save_to_disk_never_panics_on_bad_dir() {
        // Writing to a path that can't be created should not panic.
        let cache = DtsCache {
            store: DashMap::new(),
            cache_dir: Utf8PathBuf::from("/dev/null/impossible/cache"),
            dirty: std::sync::atomic::AtomicBool::new(true),
        };
        cache.save_to_disk(); // must not panic
    }

    #[test]
    fn test_dirty_flag_prevents_unnecessary_write() {
        let tmp = temp_dir("dirty");
        let cache_dir = tmp.join("cache");
        let cache = DtsCache::load_from_disk(Some(&cache_dir));

        assert!(!cache.dirty.load(std::sync::atomic::Ordering::Relaxed));
        cache.save_to_disk();
        // manifest should NOT exist because cache wasn't dirty
        assert!(!cache_dir.join("manifest.json").exists());

        let file_path = make_temp_file(&tmp, "dirty.d.ts", b"export type D = boolean;");
        cache.insert(&file_path, SourceData::default());
        assert!(cache.dirty.load(std::sync::atomic::Ordering::Relaxed));

        cache.save_to_disk();
        assert!(!cache.dirty.load(std::sync::atomic::Ordering::Relaxed));
        assert!(cache_dir.join("manifest.json").exists());
    }

    #[test]
    fn test_eviction_when_exceeding_max_entries() {
        let tmp = temp_dir("evict");
        let cache_dir = tmp.join("cache");
        let cache = DtsCache::load_from_disk(Some(&cache_dir));

        for i in 0..(MAX_CACHE_ENTRIES + 10) {
            let key = CacheKey { path: Utf8PathBuf::from(format!("file_{i}.d.ts")), size: 10, mtime_ns: 100 };
            cache.store.insert(key, SourceData::default());
        }

        let file_path = make_temp_file(&tmp, "overflow.d.ts", b"export type O = string;");
        cache.insert(&file_path, SourceData::default());
        assert!(cache.store.len() <= MAX_CACHE_ENTRIES);
    }
}
