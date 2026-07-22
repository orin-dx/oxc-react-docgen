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
    #[napi(ts_type = "'curated' | 'full' | 'none'")]
    pub html_attributes: Option<String>,
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

// ─── TryFrom<JsExtractOptions> for PipelineOptions ────────────────────────────

impl TryFrom<JsExtractOptions> for PipelineOptions {
    /// Names the bad `reactVersion` value — a typo (or a caller bypassing the
    /// `'react18' | 'react19'` TS type, e.g. via `as any`) must not silently
    /// fall back to react19 (crates/core/CLAUDE.md non-negotiable #6).
    type Error = String;

    fn try_from(js: JsExtractOptions) -> Result<Self, Self::Error> {
        use compact_str::CompactString;

        let mut opts =
            PipelineOptions { src_dirs: js.src_dirs.into_iter().map(Into::into).collect(), ..Default::default() };

        if let Some(exclude) = js.exclude {
            opts.exclude_patterns = exclude;
        }
        if let Some(v) = js.react_version.as_deref() {
            opts.react_version = oxc_react_docgen_core::react_types::parse_react_version(v)
                .map_err(|bad| format!("reactVersion is '{bad}', expected \"react18\" or \"react19\""))?;
        }
        if let Some(cross_package) = js.cross_package {
            opts.cross_package = cross_package;
        }
        if let Some(dir) = js.pandacss_outdir {
            opts.pandacss_outdir = Some(dir.into());
        }
        if let Some(fns) = js.variant_functions {
            opts.variant_functions = fns;
        }
        if let Some(mode) = js.html_attributes.as_deref() {
            opts.html_attributes = match mode {
                "full" => oxc_react_docgen_core::pipeline::HtmlAttributeMode::Full,
                "none" => oxc_react_docgen_core::pipeline::HtmlAttributeMode::None,
                _ => oxc_react_docgen_core::pipeline::HtmlAttributeMode::Curated,
            };
        }
        if let Some(path) = js.tsconfig_path {
            opts.tsconfig_path = Some(path.into());
        }
        if let Some(names) = js.extra_builtins {
            opts.extra_builtins = names.into_iter().map(CompactString::from).collect();
        }
        if let Some(json) = js.extra_paths_json {
            opts.extra_paths = serde_json::from_str(&json).map_err(|e| format!("extraPaths is not valid JSON: {e}"))?;
        }
        if let Some(json) = js.known_type_overrides_json {
            opts.known_type_overrides =
                serde_json::from_str(&json).map_err(|e| format!("knownTypeOverrides is not valid JSON: {e}"))?;
        }
        if let Some(v) = js.vanilla_extract {
            opts.vanilla_extract = v;
        }
        if let Some(dir) = js.cache_dir {
            opts.cache_dir = Some(dir.into());
        }
        if let Some(v) = js.resolve_complex_types {
            opts.resolve_complex_types = v;
        }

        Ok(opts)
    }
}

// ─── Public NAPI functions ────────────────────────────────────────────────────

/// Cold extraction — returns JSON string of ExtractionOutput.
/// Use for: build-time extraction, CLI backing, one-off runs.
#[napi]
pub async fn extract_all(options: JsExtractOptions) -> napi::Result<String> {
    let pipeline_options = PipelineOptions::try_from(options).map_err(napi::Error::from_reason)?;
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
pub fn create_session(options: JsExtractOptions) -> napi::Result<u32> {
    let pipeline_options = PipelineOptions::try_from(options).map_err(napi::Error::from_reason)?;
    let id = next_session_id();
    let session = Arc::new(WatchSession::new(pipeline_options));
    SESSIONS.insert(id, session);
    Ok(id)
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
    let session = match SESSIONS.get(&session_id) {
        Some(s) => s.clone(),
        None => {
            let pipeline_options = PipelineOptions::try_from(options).map_err(napi::Error::from_reason)?;
            let s = Arc::new(WatchSession::new(pipeline_options));
            SESSIONS.insert(session_id, s.clone());
            s
        }
    };

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
            let pipeline_options = PipelineOptions::try_from(options).map_err(napi::Error::from_reason)?;
            let s = Arc::new(WatchSession::new(pipeline_options));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base_options() -> JsExtractOptions {
        JsExtractOptions {
            src_dirs: vec!["./src".into()],
            exclude: None,
            react_version: None,
            cross_package: None,
            pandacss_outdir: None,
            variant_functions: None,
            html_attributes: None,
            tsconfig_path: None,
            extra_builtins: None,
            vanilla_extract: None,
            cache_dir: None,
            resolve_complex_types: None,
            extra_paths_json: None,
            known_type_overrides_json: None,
        }
    }

    #[test]
    fn unset_fields_match_pipeline_options_defaults() {
        let opts = PipelineOptions::try_from(base_options()).expect("should convert");
        let defaults = PipelineOptions::default();

        assert_eq!(opts.cross_package, defaults.cross_package);
        assert_eq!(opts.variant_functions, defaults.variant_functions);
        assert_eq!(opts.html_attributes, defaults.html_attributes);
        assert_eq!(opts.vanilla_extract, defaults.vanilla_extract);
        assert_eq!(opts.resolve_complex_types, defaults.resolve_complex_types);
    }

    #[test]
    fn malformed_extra_paths_json_is_a_hard_error_not_a_silent_empty_map() {
        let mut js = base_options();
        js.extra_paths_json = Some("not valid json".into());

        let err = PipelineOptions::try_from(js).expect_err("malformed JSON should not silently default");
        assert!(err.contains("extraPaths"), "error should name the bad field, got: {err}");
    }

    #[test]
    fn malformed_known_type_overrides_json_is_a_hard_error_not_a_silent_empty_map() {
        let mut js = base_options();
        js.known_type_overrides_json = Some("not valid json".into());

        let err = PipelineOptions::try_from(js).expect_err("malformed JSON should not silently default");
        assert!(err.contains("knownTypeOverrides"), "error should name the bad field, got: {err}");
    }

    #[test]
    fn valid_extra_paths_json_is_parsed_correctly() {
        let mut js = base_options();
        js.extra_paths_json = Some(r#"{"@myorg/ui": ["../packages/ui/src"]}"#.into());

        let opts = PipelineOptions::try_from(js).expect("should parse valid JSON");
        assert_eq!(opts.extra_paths.get("@myorg/ui").map(|v| v.len()), Some(1));
    }
}
