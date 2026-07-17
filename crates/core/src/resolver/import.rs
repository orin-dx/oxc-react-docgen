//! Import and canonical resolution.

use camino::{Utf8Path, Utf8PathBuf};

use crate::types::*;

use super::ResolutionContext;

/// Resolve `name` to its canonical `(file_path, name)` pair.
/// Returns `None` if `name` is a local declaration (not imported).
pub(super) fn resolve_to_canonical(
    name: &str,
    consuming_file: &Utf8Path,
    ctx: &ResolutionContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(Utf8PathBuf, String)> {
    if let Some(import_ref) = ctx.import_map.find_import(consuming_file, name) {
        let resolved_file = resolve_import_specifier(&import_ref.specifier, consuming_file, ctx, diagnostics)?;
        let canonical_name = import_ref.exported_name.to_string();
        return Some((resolved_file, canonical_name));
    }

    // Namespace-import member access: `import * as React from "react"` then a
    // reference to `React.DependencyList`. `find_import` only tracks the
    // namespace binding itself ("React", with `exported_name == "*"`) — a
    // literal dotted name like "React.DependencyList" is never itself a
    // binding, so the lookup above always misses it. Split off the namespace
    // prefix and route the member name through the namespace's own import
    // specifier instead.
    let (namespace, member) = name.split_once('.')?;
    let namespace_ref = ctx.import_map.find_import(consuming_file, namespace)?;
    if namespace_ref.exported_name != "*" {
        return None;
    }
    let resolved_file = resolve_import_specifier(&namespace_ref.specifier, consuming_file, ctx, diagnostics)?;
    Some((resolved_file, member.to_string()))
}

/// Look up an interface by canonical `(file, name)`, falling back to a
/// `React.`-namespace-qualified key. `@types/react` declares everything
/// inside `declare namespace React { ... }`, so real declarations are keyed
/// "React.HTMLAttributes" even though nothing imports them that way — a plain
/// `import type { HTMLAttributes } from "react"` binds the bare name.
pub(super) fn lookup_interface<'g>(
    global: &'g GlobalSourceData,
    canonical_file: &str,
    canonical_name: &str,
) -> Option<&'g CollectedInterface> {
    global
        .interfaces
        .get(&format!("{canonical_file}:{canonical_name}"))
        .or_else(|| global.interfaces.get(&format!("{canonical_file}:React.{canonical_name}")))
}

/// Same as `lookup_interface`, for type aliases. Returns the key that actually
/// matched (bare or `React.`-qualified) alongside the value, since callers
/// also need it to look up `type_alias_params` under the same key.
pub(super) fn lookup_type_alias<'g>(
    global: &'g GlobalSourceData,
    canonical_file: &str,
    canonical_name: &str,
) -> Option<(String, &'g CollectedTypeAlias)> {
    let bare_key = format!("{canonical_file}:{canonical_name}");
    if let Some(alias) = global.type_aliases.get(&bare_key) {
        return Some((bare_key, alias));
    }
    let qualified_key = format!("{canonical_file}:React.{canonical_name}");
    global.type_aliases.get(&qualified_key).map(|alias| (qualified_key, alias))
}

/// Look up a name directly on TypeScript's own ambient lib files
/// (`ctx.ambient_global_files` — `lib.es5.d.ts`/`lib.dom.d.ts`), by bare name
/// only (these are ambient globals, never namespace-qualified). Deliberately
/// separate from — and only ever consulted AFTER — every other resolution
/// path (imports, same-file declarations, the well-known-TS-utility-type
/// silent list): `Record`/`Partial`/etc. are themselves declared here too
/// (`Record<K, T> = { [P in K]: T }`, a mapped type this tool can't expand),
/// so checking this first would intercept those already-handled names and
/// degrade them to Opaque instead of leaving them to the existing shortcuts.
pub(super) fn lookup_ambient_global<'g>(ctx: &'g ResolutionContext, name: &str) -> Option<AmbientGlobalLookup<'g>> {
    for lib_file in &ctx.ambient_global_files {
        let key = format!("{lib_file}:{name}");
        if ctx.global.interfaces.contains_key(&key) {
            return Some(AmbientGlobalLookup::Interface);
        }
        if let Some(alias) = ctx.global.type_aliases.get(&key) {
            return Some(AmbientGlobalLookup::TypeAlias(alias));
        }
    }
    None
}

pub(super) enum AmbientGlobalLookup<'g> {
    Interface,
    TypeAlias(&'g CollectedTypeAlias),
}

/// Use `oxc_resolver` to turn an import specifier into an absolute file path.
pub(super) fn resolve_import_specifier(
    specifier: &str,
    from_file: &Utf8Path,
    ctx: &ResolutionContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Utf8PathBuf> {
    let from_dir = from_file.parent()?;

    // Bare package specifiers (not relative/absolute) need real TypeScript
    // declaration resolution: prefer a real .d.ts over whatever JS entry point
    // the package's own "main"/"exports" happens to point to, and fall back to
    // the separate `@types/<package>` package when the package ships no types
    // of its own at all — e.g. "react": its package.json has no "types" field
    // and no "types" export condition, so a plain `resolve()` below lands on
    // `index.js`, and a named type-only import like `HTMLAttributes` can never
    // be found there.
    if !specifier.starts_with('.') && !specifier.starts_with('/') {
        if let Some(path) = super::react::resolve_package_types_file(&ctx.oxc_resolver, from_dir, specifier) {
            return Utf8PathBuf::from_path_buf(std::path::PathBuf::from(path)).ok();
        }
    }

    match ctx.oxc_resolver.resolve(from_dir.as_std_path(), specifier) {
        Ok(resolved) => Utf8PathBuf::from_path_buf(resolved.path().to_owned()).ok(),
        Err(e) => {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                message: format!("Cannot resolve '{}' from '{}'", specifier, from_file),
                file: Some(from_file.to_string()),
                line: None,
                column: None,
                help: Some(format!("Resolution error: {}", e)),
                code: DiagnosticCode::UnresolvableImport,
            });
            None
        }
    }
}
