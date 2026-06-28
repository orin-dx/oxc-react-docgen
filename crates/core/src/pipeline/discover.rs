use camino::Utf8PathBuf;

/// Walk `src_dirs` and collect every `.ts` / `.tsx` file, excluding:
/// - Built-in patterns: `.stories.`, `.test.`, `.spec.`, `__snapshots__`, `node_modules`
/// - User-supplied extra exclude patterns
pub(super) fn discover_files(src_dirs: &[Utf8PathBuf], extra_excludes: &[String]) -> Vec<Utf8PathBuf> {
    let mut files = Vec::new();

    for dir in src_dirs {
        // If the user explicitly points at a node_modules path, respect it.
        let dir_str = dir.as_str();
        let dir_is_in_node_modules = dir_str.contains("node_modules");

        let walker =
            ignore::WalkBuilder::new(dir.as_std_path()).hidden(false).git_ignore(!dir_is_in_node_modules).build();

        for entry in walker.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "ts" | "tsx") {
                continue;
            }

            let path_str = path.to_str().unwrap_or("");

            // Built-in excludes — skip node_modules sub-dirs only when not intentionally targeting them.
            if path_str.contains(".stories.")
                || path_str.contains(".test.")
                || path_str.contains(".spec.")
                || path_str.contains("__snapshots__")
                || (!dir_is_in_node_modules && path_str.contains("node_modules"))
            {
                continue;
            }

            // User-supplied excludes.
            if extra_excludes.iter().any(|p| path_str.contains(p.as_str())) {
                continue;
            }

            if let Ok(utf8) = Utf8PathBuf::from_path_buf(path.to_owned()) {
                files.push(utf8);
            }
        }
    }

    files.sort(); // deterministic ordering across OS / FS
    files
}

pub(super) fn should_skip(name: &str, exclude_prefixes: &[String]) -> bool {
    exclude_prefixes.iter().any(|p| name.starts_with(p.as_str()))
}
