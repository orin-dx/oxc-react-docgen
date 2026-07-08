use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use camino::Utf8Path;
use dashmap::DashMap;
use napi_derive::napi;

use oxc_react_docgen_core::pipeline::{
    extraction_output_to_json, incremental_update_to_json, PipelineOptions, WatchSession,
};

// ─── Session store ────────────────────────────────────────────────────────────

static SESSIONS: LazyLock<DashMap<u32, Arc<WatchSession>>> = LazyLock::new(DashMap::new);
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(0);

fn next_session_id() -> u32 {
    let pid = std::process::id() as u64;
    let counter = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    // Combine pid (upper 16 bits) with counter (lower 16 bits) into u32.
    // This avoids collisions across concurrent Vite dev server instances.
    (((pid & 0xFFFF) << 16) | (counter & 0xFFFF)) as u32
}

// ─── JsExtractOptions ─────────────────────────────────────────────────────────

/// Options passed from TypeScript — flat struct for easy NAPI marshalling.
#[napi(object)]
pub struct JsExtractOptions {
    pub src_dirs: Vec<String>,
    pub exclude: Option<Vec<String>>,
    #[napi(ts_type = "'react18' | 'react19'")]
    pub react_version: Option<String>,
    pub cross_package: Option<bool>,
    pub pandacss_outdir: Option<String>,
    pub variant_functions: Option<Vec<String>>,
    pub skip_html_props: Option<bool>,
    // Fields from architectural review:
    pub tsconfig_path: Option<String>,
    pub extra_builtins: Option<Vec<String>>,
    pub vanilla_extract: Option<bool>,
    pub cache_dir: Option<String>,
    pub resolve_complex_types: Option<bool>,
    // Complex fields accepted as JSON strings to avoid napi object complexity:
    /// JSON: Record<string, string[]>
    pub extra_paths_json: Option<String>,
    /// JSON: Record<string, {kind: ...}>
    pub known_type_overrides_json: Option<String>,
}

// ─── From<JsExtractOptions> for PipelineOptions ───────────────────────────────

impl From<JsExtractOptions> for PipelineOptions {
    fn from(js: JsExtractOptions) -> Self {
        use compact_str::CompactString;

        let extra_builtins: rustc_hash::FxHashSet<CompactString> =
            js.extra_builtins.unwrap_or_default().into_iter().map(CompactString::from).collect();

        let extra_paths: rustc_hash::FxHashMap<String, Vec<camino::Utf8PathBuf>> = js
            .extra_paths_json
            .and_then(|s| serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&s).ok())
            .map(|map| {
                map.into_iter()
                    .filter_map(|(k, v)| {
                        let paths: Vec<camino::Utf8PathBuf> =
                            v.as_array()?.iter().filter_map(|p| p.as_str()).map(camino::Utf8PathBuf::from).collect();
                        Some((k, paths))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let known_type_overrides: rustc_hash::FxHashMap<String, oxc_react_docgen_core::pipeline::KnownTypeOverride> =
            js.known_type_overrides_json.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();

        PipelineOptions {
            src_dirs: js.src_dirs.into_iter().map(Into::into).collect(),
            exclude_patterns: js.exclude.unwrap_or_default(),
            react_version: match js.react_version.as_deref() {
                Some("react18") => oxc_react_docgen_core::react_types::REACT_18,
                _ => oxc_react_docgen_core::react_types::REACT_19,
            },
            cross_package: js.cross_package.unwrap_or(true),
            pandacss_outdir: js.pandacss_outdir.map(Into::into),
            variant_functions: js
                .variant_functions
                .unwrap_or_else(|| vec!["cva".into(), "tv".into(), "defineRecipe".into(), "recipe".into()]),
            skip_html_props: js.skip_html_props.unwrap_or(false),
            tsconfig_path: js.tsconfig_path.map(Into::into),
            extra_paths,
            known_type_overrides,
            extra_builtins,
            vanilla_extract: js.vanilla_extract.unwrap_or(false),
            cache_dir: js.cache_dir.map(Into::into),
            resolve_complex_types: js.resolve_complex_types.unwrap_or(false),
            exclude_prefixes: vec![],
        }
    }
}

// ─── Public NAPI functions ────────────────────────────────────────────────────

/// Cold extraction — returns JSON string of ExtractionOutput.
/// Use for: build-time extraction, CLI backing, one-off runs.
#[napi]
pub async fn extract_all(options: JsExtractOptions) -> napi::Result<String> {
    let pipeline_options = PipelineOptions::from(options);
    tokio::task::spawn_blocking(move || {
        let output = oxc_react_docgen_core::pipeline::extract(&pipeline_options);
        extraction_output_to_json(&output).map_err(|e| napi::Error::from_reason(e.to_string()))
    })
    .await
    .map_err(|e| napi::Error::from_reason(e.to_string()))?
}

/// Create a persistent watch session. Returns session ID.
/// Call this in Vite's configResolved hook.
#[napi]
pub fn create_session(options: JsExtractOptions) -> u32 {
    let id = next_session_id();
    let session = Arc::new(WatchSession::new(PipelineOptions::from(options)));
    SESSIONS.insert(id, session);
    id
}

/// Incremental extraction for a single changed file.
/// Returns JSON string of IncrementalUpdate.
/// Use for: Vite HMR hotUpdate.
#[napi]
pub async fn extract_file_incremental(
    file_path: String,
    session_id: u32,
    options: JsExtractOptions,
) -> napi::Result<String> {
    let session = SESSIONS
        .entry(session_id)
        .or_insert_with(|| Arc::new(WatchSession::new(PipelineOptions::from(options))))
        .clone();

    tokio::task::spawn_blocking(move || {
        let path = Utf8Path::new(&file_path);
        let update = session.update_file(path);
        incremental_update_to_json(&update).map_err(|e| napi::Error::from_reason(e.to_string()))
    })
    .await
    .map_err(|e| napi::Error::from_reason(e.to_string()))?
}

/// Initialize a watch session with a full cold extraction.
/// Returns JSON string of ExtractionOutput.
/// Call this after create_session(), in configureServer.
#[napi]
pub async fn initialize_session(session_id: u32, options: JsExtractOptions) -> napi::Result<String> {
    let session = match SESSIONS.get(&session_id) {
        Some(s) => s.clone(),
        None => {
            let s = Arc::new(WatchSession::new(PipelineOptions::from(options)));
            SESSIONS.insert(session_id, s.clone());
            s
        }
    };
    tokio::task::spawn_blocking(move || {
        let output = session.initialize();
        oxc_react_docgen_core::pipeline::extraction_output_to_json(&output)
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    })
    .await
    .map_err(|e| napi::Error::from_reason(e.to_string()))?
}

/// Release session state. Call in Vite's buildEnd hook.
#[napi]
pub fn close_session(session_id: u32) {
    SESSIONS.remove(&session_id);
}
