# oxc-react-docgen — Project Status

**Last updated:** 2026-06-15  
**Branch:** master  
**Tests:** 97 passing, 0 failing (90 unit + 7 snapshots)  
**Build:** clean (cargo clippy -D warnings passes)

---

## Phase Completion

| Phase | Status | Notes |
|-------|--------|-------|
| 0 — Repo Setup | ✅ Complete | Cargo workspace, moon, proto, just, CI |
| 1a — types.rs | ✅ Complete | CollectedType, InheritedLayer, RawDefault, PropType (manual serde) |
| 1b — Fixtures | ✅ Complete | shadcn, radix, mui, chakra, mantine, react-aria, panda |
| 2a — Extractor | ✅ Complete | OXC 0.135 AST visitor, all 4 component patterns, cva() extraction |
| 2b — ImportMap | ✅ Complete | 9 tests |
| 2c — KnownPatterns | ✅ Complete | 33 tests, RecipeVariants added |
| 3a — Resolver | ✅ Complete | CollectedType dispatch, inheritance chain, discriminated unions |
| 3b — Pipeline | ✅ Complete | rayon parallel extract, WatchSession, DtsCache (now wired in) |
| 4a — NAPI | ✅ Complete | All JsExtractOptions fields, session management, initializeSession |
| 4b — CLI | ✅ Complete | 5 subcommands, comfy-table inspect, watchexec 8.x, crossterm |
| Refactor (R1-R8, R11) | ✅ Complete | Module splits, ResolveState, IO errors, visibility, .clippy.toml |
| Quality (R12-R17) | ✅ Complete | deny(unsafe_code), Display+Error on Diagnostic, non_exhaustive, must_use, ScopedKey deleted, pub use at crate root, ResolveState pub(crate) |
| Claude Code setup | ✅ Complete | CLAUDE.md, skills (rust-style/types/rustdoc), /check, /snapshot |
| Pre-commit hooks | ✅ Complete | moon native vcs hooks: fmt, clippy --all-targets, typos, cargo-deny |
| 5a — Vite plugin | ❌ Not started | Single Plugin, Vite 7+, hotUpdate + environment.hot.send(), virtual module |
| 5b — Rolldown plugin | 🚫 Dropped | No external native Rust plugin mechanism in Rolldown 1.x; JS/NAPI bridge adds no value over Vite plugin |
| 6 — Integration tests | ❌ Not started | Needs validate/run-ours.ts once NAPI binary compiled |

## Quality Improvement Queue (R12–R17) — ✅ Complete

All items completed 2026-06-15.

| Item | Status | Notes |
|------|--------|-------|
| R12 | ✅ | `#![deny(unsafe_code)]`; `cargo-deny` in pre-commit hook; clippy `--all-targets -D warnings` |
| R13 | ✅ | Verified: all `unwrap()`/`expect()` are inside `#[cfg(test)]` — no production panics |
| R14 | ✅ | `Display`+`Error` on `Diagnostic`; `#[non_exhaustive]` on `DiagnosticSeverity`+`DiagnosticCode`; `#[must_use]` on `ExtractionOutput` |
| R15 | ✅ | Deleted `ScopedKey` — was defined but never wired into `GlobalSourceData` maps |
| R16 | ✅ | `import_map.rs` pre-wired scaffolding suppressed with `#[allow(dead_code)]` (tests cover it) |
| R17 | ✅ | `pub use` at crate root for primary consumer API; `ResolveState` → `pub(crate)` |

---

## Current Known Bugs

### ✅ Fixed: Radix Button `ComponentPropsWithoutRef` (2026-06-14)

Root cause: The Radix pattern uses an **intersection type alias** (`type PrimitiveButtonProps = React.ComponentPropsWithoutRef<"button"> & { asChild?: boolean }`), not a direct `extends`. The code path was `resolve_type_alias_chain(Intersection)` → `resolve_base_as_chain(Named)`, which was **discarding type args** (`args: _`) when calling `resolve_props_chain`. This meant `ComponentPropsWithoutRef` was called with empty args, `resolve_known` returned `None`, and `is_react_builtin` fired → `empty_with_compose`.

**Fixes applied:**
1. `resolve_base_as_chain`: Pass type args through when resolving `CollectedType::Named` (was `args: _` → `args`, converting to `raw_args`)
2. `resolve_base_as_chain`: Handle `CollectedType::Object(fields)` — expand inline object fields as own props (was `_ => default()`)
3. `resolve_props_chain` Step 2: `KnownPatternResult::Type(PropType::HtmlAttributes)` now creates `InheritedLayer` instead of `empty_with_compose`
4. Regression test added: `test_component_props_without_ref_in_intersection_alias`

### ✅ Fixed: WatchSession::update_file race condition

`ArcSwap::rcu` is used in `update_file` — retries if a concurrent call swaps the pointer, preventing lost-update races.

### 🟡 Medium: Mantine/React Aria Button collision in fixture dedup

When running `--src fixtures/` (all libraries), multiple `Button` components from different libraries with the same file stem (`Button.d`) collide in the dedup logic, causing one to be dropped. Not a real-world issue (users only scan their own codebase).

### 🟢 Minor: Watch mode `--out` writes placeholder `{}`

`cmd_watch` writes a literal `{}` to the output file on each change. The full extraction output should be written.

### 🟢 Minor: CLI `--config docgen.config.ts` is stubbed

Config file is read and evaluated via node+tsx but the result is discarded (returns `None`). The JSON → PipelineOptions mapping needs to be implemented.

### 🟢 Minor: `run-ours.ts` missing in packages/validate

The validation harness has `run-react-docgen.ts` and `run-react-docgen-typescript.ts` but no `run-ours.ts` to compare against. Can only be written after the NAPI binary is compiled.

---

## Adversarial Analysis Findings (2026-06-14)

Conducted full adversarial review. Findings by severity:

### Fixed ✅
- React.* namespace prefix not recognized in `resolve_props_chain` (→ fixed, now strips prefix before builtin check)
- `notable_inherited` always empty (→ fixed, synthesized directly from `notable_html_attrs` table)
- cva() variant extraction missing (→ fixed, extractor now detects cva/tv/defineRecipe calls)
- `VariantProps<typeof buttonVariants>` not resolving (→ fixed, `"typeof X"` raw strings now → `Named{name:X}`)
- TypeScript built-in utility types emitting false warnings (→ fixed, short-circuit before Step 6)
- Wrong TypeScript field names in `index.d.ts` (→ fixed: `event_type` → `eventType`, `has_default` → `hasDefault`, `function_name` → `functionName`)
- `LiteralUnion.raw_string()` missing quotes (→ fixed: `default` → `"default"`)
- DTS cache dead code (→ fixed, now wired into parallel parse loop with Arc)
- NAPI `initializeSession` missing (→ fixed, added new NAPI function)
- Command injection in `--config` path loading (→ fixed, path passed via env var not embedded in JS)
- `resolve_known` priority over source types (→ fixed, import resolution now before known patterns in `resolve_named`)
- `PropType` serde 19-minute compile time (→ fixed, manual `to_tagged_value`/`from_tagged_value` impl)

### Still Open 🔴
- None — all adversarial findings resolved

---

## Architecture Decisions (canonical reference)

### Data flow
```
OXC parse → SourceData (extractor) → GlobalSourceData (merge) → ComponentEntry (resolver) → ExtractionOutput (pipeline)
```

### Key type choices
- `CollectedType` — structured AST type from extractor (not raw string)
- `PropType` — semantic resolved type in output
- `InheritedLayer` — one step in inheritance chain (replaces old `html_element`/`omitted_html_props`)
- `notable_inherited` — curated subset of inherited HTML attrs (onClick, disabled, etc.)
- `discriminant_prop` — name of the discriminating prop in a union type (MUI TextField `variant`)
- `param_defaults` — default values extracted from destructuring parameters

### Serde recursion workaround
Both `CollectedType` and `PropType` are deeply recursive enums. Using serde derive on them caused 19-minute compile times (recursion limit overflows). Both now have manual `Serialize`/`Deserialize` implementations via `to_json_value()`/`from_json_value()` that use concrete `serde_json::Value` as intermediary, eliminating the compile-time monomorphization chain.

`#![recursion_limit = "2048"]` still lives in `lib.rs` for other serde traits, but no longer needed for PropType/CollectedType.

### Cache location
`node_modules/.cache/oxc-react-docgen/` (CI-friendly, cleaned with node_modules). NOT `~/.cache/`.

### Plugin architecture
- `packages/napi/` — compiled native binary + TypeScript types (`@oxc-react-docgen/napi`)
- `packages/vite-plugin/` — single `Plugin` (not `Plugin[]`), Vite `^7.0.0 || ^8.0.0`, `hotUpdate` hook + `environment.hot.send()`, `\0virtual:oxc-react-docgen` module (`@oxc-react-docgen/vite-plugin`)
- Phase 5b (Rolldown plugin) — dropped: Rolldown 1.x has no external native Rust plugin mechanism
- Config file — `docgen.config.ts`, discovered by walking up to workspace root

### OXC 0.135 API notes (vs spec written for 0.60)
- `Visit` trait is in `oxc_ast_visit` crate (separate from `oxc_ast`)
- `TSMappedType.constraint` is a direct field
- `TSTupleElement` is an enum variant (no `.element_type()` method)
- `BindingPattern` is an enum directly
- `type_arguments` not `type_parameters` on TSTypeReference / TSInterfaceHeritage
- `ModuleExportName.name()` returns `Str` not `&str`

---

## Immediate Next Steps (priority order)

**Refactor phase COMPLETE (R1-R8, R11). Ready for Phase 5+.**

Completed refactor sequence:

1. **R1** ✅ — `insta` snapshot tests across all 7 fixtures (safety net)
2. **R2** ✅ — Split `types.rs` → `types/{collected,output,diagnostic,global}.rs`; added `ScopedKey`, `ResolveState` types
3. **R3** ✅ — `ResolveState` wired into all 17 resolver functions; `visited/diagnostics` threaded as one struct
4. **R4** ✅ — Split `resolver.rs` (2,482 lines) → `resolver/` (12 sub-modules: chain, extends, alias, collected, named, primitives, template, func, react, import, html)
5. **R5** ✅ — Split `extractor.rs` (1,867 lines) → `extractor/` (7 sub-modules: component, interface, alias, jsdoc, visit, defaults)
6. **R6** ✅ — Split `pipeline.rs` → `pipeline/{discover,watch}.rs`; fixed 3 silent IO failures to emit `Diagnostic::IoError`
7. **R7** ✅ — Split `cli/main.rs` (760 lines) → `commands/{extract,watch,inspect,check,completions}.rs` + `config.rs` + `output.rs`
8. **R8** ✅ — Visibility: `cache`, `extractor`, `import_map`, `known`, `resolver` → `pub(crate)`; `pipeline`, `types`, `react_types` remain `pub`
9. **R9** — Skipped: PropType derive requires breaking JSON format (tuple variants can't use `#[serde(tag)]` cleanly)
10. **R10** — Deferred: cross-package resolution is a new feature, not refactor
11. **R11** ✅ — `.clippy.toml` with `too-many-arguments-threshold = 6`, `too-many-lines-threshold = 100`

All 97 tests (90 unit + 7 snapshot) green throughout.

After refactor:
- Phase 5a — Vite plugin (spec: `docs/10-PLUGIN-SPEC.md`)
- Phase 6 — Integration tests + benchmarks
- Wire config file loading
- Add preset system

---

## Validation Results (2026-06-14, running against fixtures/)

```
=== 15 components, 32ms ===
  Badge                   1 own  inh=[div]  notable=[aria-label, className, id]       ✅
  Button (Chakra)        13 own  inh=[]     notable=[]                                ✅
  Button (React Aria)    35 own  inh=[]     notable=[]                                ✅
  Button (shadcn)         3 own  inh=[btn]  notable=[aria-*, disabled, onClick]        ✅  variant+size+asChild
  Button (Radix)          1 own  inh=[btn]  notable=[aria-*, disabled, onClick, ...]   ✅  asChild prop
  Input (Chakra)         24 own  inh=[]     notable=[]                                ✅
  Input (shadcn)          0 own  inh=[inp]  notable=[autoComplete, checked, ...]      ⚠️  missing own props?
  TextField (MUI)        33 own  disc=variant                                         ✅
  TextInput (Mantine)    40 own                                                       ✅
  ButtonGroup             0 own                                                       -
```

**Key accuracy wins:**
- shadcn Button: `variant: "default"|"destructive"|...`, `size: "default"|"sm"|...`, `asChild: boolean` ✅
- MUI TextField: discriminated union detected, `variant` as discriminant ✅
- Mantine TextInput: 40 props from layered interface composition ✅
- Notable HTML attrs: `onClick`, `disabled`, `type`, `aria-*` correctly curated per element ✅

**Key accuracy gaps:**
- Radix Button: `ComponentPropsWithoutRef<'button'>` wrapper not expanding (bug above)
- shadcn Input: extends `React.InputHTMLAttributes<HTMLInputElement>` should produce own props (InputProps is empty interface — this is actually correct, 0 own props is right)
- No MUI Button in output — name collision with Chakra Button (dedup issue with fixture mixing)

---

## Files to know

| File | Purpose |
|------|---------|
| `crates/core/src/types.rs` | All shared data types — the contract |
| `crates/core/src/extractor.rs` | OXC AST → SourceData |
| `crates/core/src/resolver.rs` | SourceData → ComponentEntry (prop type resolution) |
| `crates/core/src/pipeline.rs` | Orchestration: discover → parse → merge → resolve → output |
| `crates/core/src/known.rs` | Library-specific type shortcuts (SxProps, VariantProps, etc.) |
| `crates/core/src/react_types.rs` | React builtin recognition + notable HTML attrs table |
| `crates/core/src/import_map.rs` | Import resolution map |
| `crates/core/src/cache.rs` | DTS parse cache (mtime+size invalidation, msgpack+atomic write) |
| `crates/napi/src/lib.rs` | NAPI bindings: extractAll, createSession, initializeSession, closeSession |
| `crates/cli/src/main.rs` | CLI: extract, inspect, watch, check, completions |
| `packages/napi/index.d.ts` | TypeScript types for the NAPI package |
| `packages/validate/` | Comparison harness: react-docgen + react-docgen-typescript baselines |
| `fixtures/` | Real-world .d.ts and .tsx fixtures from shadcn, MUI, Chakra, Mantine, etc. |
| `docs/08-OPEN-QUESTIONS.md` | Architecture decisions log |
| `docs/09-STATUS.md` | This file |
