# Edge Cases & Failure Modes

A comprehensive audit of non-happy-path behavior across the pipeline: extractor, resolver, pipeline/cache/import_map, CLI/config/NAPI binding, and types/serialization/plugin. Produced by five parallel read-only audits against the codebase as of 2026-08-04, cross-checked against existing test coverage. Nothing here has been fixed yet — this is the map, not the patch.

**How to read this doc:** findings are grouped by severity tier first (for prioritization), then by subsystem (for context when doing the actual fix). "Tested?" means an existing test exercises exactly this behavior, not just the surrounding happy path.

---

## Severity tiers

- **Crash/hang** — can panic, OOM, or hang the process on realistic or adversarial input.
- **Silent data loss** — violates CLAUDE.md non-negotiable #6 ("always emit a Diagnostic when degrading — never fail silently"): output is missing or incomplete with no trace.
- **Silent correctness bug** — output is present but *wrong*, not just missing, with no trace.
- **Contract inconsistency** — behavior diverges from a documented or implied contract (exit codes, schema, precedence rules already established elsewhere in the same codebase).
- **Documented/by-design limitation** — already tracked in `rdt-coverage.md` or `type-checker-integration.md`; listed here only for completeness.

---

## Priority 0 — silent correctness bugs (wrong output, not just missing)

| # | Finding | Location | Tested? |
| --- | --- | --- | --- |
| P0-1 | `known.rs` shortcut precedence is inconsistent: `named.rs` checks source-defined interfaces/type-aliases *before* falling back to hardcoded library shortcuts (correct, and explicitly commented as intentional to avoid this exact collision) — but `chain.rs`'s `extends`-clause path (reached from both `ExtendsRef::SameFile` and `ExtendsRef::Imported`) checks the hardcoded shortcuts *first*. A project with its own `interface SxProps {...}` or `interface SlotProps {...}` extended via `extends` gets silently replaced by the hardcoded MUI/React-Aria opaque shape. | `resolver/chain.rs:90-106` vs `resolver/named.rs:41-72` | No |
| P0-2 | Cache staleness on same-mtime-tick, same-size edits: `key_for` keys on `(path, size, mtime_ns)`. On filesystems/environments with coarse mtime resolution (network FS, container overlay FS), an edit completing within the same tick and producing identical byte length is indistinguishable from the original — a stale `SourceData` is served. Scope is narrow (only `.d.ts` files go through this cache) but real. | `cache.rs:181-191` | Partial — existing test only covers a size-changing edit |
| P0-3 | `ParsedProp.required == true` with `default_value: Some(_)` is representable and unvalidated — a contradictory state (RDT's own convention treats these as mutually exclusive) that nothing in the type or its constructors prevents. | `types/output.rs:79-99` | No |
| P0-4 | TOON format truncates `Union`/`Intersection` prop types to 4 members with **no truncation indicator**, while the sibling `LiteralUnion` path in the same function correctly appends `"...(+N)"`. A 10-member union silently renders as if it had exactly 4 — misleading, not just incomplete. | `toon.rs:125-132` (vs. the correct pattern at `toon.rs:110-116`) | No |
| P0-5 | `schema.rs`'s exported JSON Schema is hand-written, not derived from the real structs, and has already drifted: `ComponentEntry.methods`/`.tags`, five of nine `ExtractionStats` fields, and `Diagnostic.line`/`.column`/`.help` are all undocumented in the schema despite being real serialized fields today. | `cli/commands/schema.rs:17-67` vs `core/types/output.rs:47-68,609-619`, `core/types/diagnostic.rs:8-16` | No — the one test only checks the schema itself is valid JSON, never diffs against real output |

## Priority 1 — crash/hang risks

| # | Finding | Location | Tested? |
| --- | --- | --- | --- |
| P1-1 | Unbounded Cartesian product when expanding template-literal types: N parts × M-member unions per part produces up to M^N strings with no cap. Large design-system token unions (a realistic, non-adversarial case) can hit this. | `resolver/template.rs:94-104` | No — zero tests exercise multi-part template literals with non-trivial union sizes |
| P1-2 | LSP `Content-Length` header is parsed and used directly in `vec![0u8; len]` with no upper bound. A malformed or buggy local LSP client sending a huge value triggers a giant allocation (likely an allocator abort, not a catchable panic). | `cli/commands/lsp.rs:36` | No |
| P1-3 | A plugin hook panic unwinds straight out of `pipeline::extract()` with zero `catch_unwind` anywhere in the repo — one bad plugin on one file kills output for every file, no diagnostic, no attribution. | `plugin.rs` (hooks called from `pipeline/mod.rs:293,385`) | No — existing plugin tests only cover the happy path |
| P1-4 | Parse/resolve phases (`src_files.par_iter().map(...).collect()` and the resolve equivalent) have no per-file `catch_unwind` — a panic in one file's extraction or resolution aborts the entire batch rather than degrading just that file. Blast radius currently bounded only by the extractor/resolver having no panic paths of their own (true today per audits) and plugins having none (false — see P1-3). | `pipeline/mod.rs:260,357` | No |
| P1-5 | `watch.rs`'s `initialized.lock().expect("init lock poisoned")` — if any prior panic occurred while holding this lock (e.g. via P1-3/P1-4 surfacing inside a watch session), the mutex poisons permanently and every subsequent operation on that session panics. No recovery path short of restarting the session. | `pipeline/watch.rs:93` | No — unpoisonable without deliberate fault injection |
| P1-6 | `lsp.rs` advertises `hoverProvider: true` in its `initialize` response, but no hover handling exists anywhere — unhandled methods fall into a silent `_ => {}` catch-all. A real client sending `textDocument/hover` (which it will, based on the advertised capability) gets **no response at all**, not even an error — for a request with an `id`, this hangs the client indefinitely. | `cli/commands/lsp.rs:56,78` | No |
| P1-7 | Malformed/headerless LSP framing: `content_length: None` after the header loop just `continue`s without consuming a body, which can desync the stream and misinterpret subsequent bytes as headers indefinitely. | `cli/commands/lsp.rs:32-34` | No |
| P1-8 | Deep conditional-type chains in source may exceed the extractor's nesting-depth guard without tripping it: the guard counts *brackets*, not *type nesting*, and chained conditional types (`A extends B ? C extends D ? ... : ... : ...`) achieve deep nesting with proportionally fewer brackets per level than paren/object nesting. The only test for this guard uses pure paren nesting, which isn't representative. Untested stack-overflow risk on adversarial or just type-heavy real-world code. | `extractor/mod.rs:38-40,105-120` (guard), `mod.rs:311-471` (recursion) | Guard tested with a non-representative case only |
| P1-9 | *(Needs verification, not confirmed)* `create_session`/`close_session` in the NAPI binding run synchronously on the JS-calling thread rather than inside `spawn_blocking` (unlike the other three entry points, which are protected). Whether a panic here crashes the Node process depends on whether napi-rs's codegen auto-wraps in `catch_unwind` — not verified against the pinned napi-rs version. | `crates/binding/src/lib.rs:132-138,193-195` | No |

## Priority 2 — silent data loss (missing output, no diagnostic)

Grouped by subsystem since these are numerous; each violates non-negotiable #6.

**Extractor:**
- Anonymous default-exported function components (`export default function(props: Props) {}`) are silently skipped — `visit_function` requires `func.id` to be `Some`. Likely a real-world miss (common in Next.js page files). `visit.rs:265`
- Class components (`class Button extends React.Component<Props>`) are never detected as components at all — no handler exists for this shape. Undocumented gap.
- `Object.assign(Component, { Sub: ... })` compound-component pattern isn't recognized; only `X.Y = ...` static-member assignment is. `interface.rs:193-201`
- Class-expression components (`const Button = class extends React.Component<Props> {}`) fall through every detector silently.
- `satisfies`-wrapped expressions (a cva/tv config, an as-const object) are never unwrapped — no `TSSatisfiesExpression` handling exists anywhere in the extractor. Silently dropped.
- `classify_type_alias`'s `Omit`/`Pick`/`Partial`/`Required`/`Readonly` arms all `return None` via `?` on malformed/missing type arguments — the *entire* type alias vanishes from `data.type_aliases`, not just the malformed part. `alias.rs:18-70`
- Any `TSType` variant not explicitly matched in `extract_props_arg`/`extract_type_name_from_type` (exotic generic expressions inside `FC<...>`) silently produces no `ComponentMapping`. `mod.rs:602-651`
- When every component detector (`try_fc_annotation`, `try_forward_ref`, `try_hoc_wrapped`, `try_forward_ref_exotic_decl`) fails, there's no signal that a PascalCase binding *looked* like a component but wasn't recognized. `visit.rs:238-247`
- Computed/numeric/symbol property keys vanish from an interface with no trace — only `StaticIdentifier`/`StringLiteral` keys are handled. `mod.rs:514-566,674-729`

**Resolver:**
- The cycle-detected return path is the one give-up site in `chain.rs` that doesn't follow the established `empty_with_compose` pattern used at every other give-up site in the same file (lines 31, 89, 126, 139, 177) and in `alias.rs:252` — a genuinely self-referential/mutually-recursive interface pair silently loses all props with zero trace. `chain.rs:38-41`
- Multi-parameter function types degrade to `PropType::Opaque{MultiParamFunction}` without ever pushing a diagnostic, unlike every other Opaque-producing path in the resolver. `func.rs:54-61`
- A `LiteralUnion` used directly as a props base is silently rejected — malformed usage, but bypasses the shared "cannot be used as props base" diagnostic path that handles this exact scenario for other non-object-like types. `alias.rs:116` vs `alias.rs:234-253`
- Generic aliases with more declared type parameters than the call site supplies (partial application, or unsupported default type params like `Foo<T, U = T>`) silently leave trailing parameters unsubstituted — `build_substitution`'s `.zip()` just stops. `substitute.rs:33-43`

**Pipeline/discovery:**
- Permission-denied files/directories are silently dropped during discovery — `ignore::Walk`'s `Err` results are discarded via `.flatten()`. `discover.rs:17`
- Non-UTF8 filenames are silently dropped during discovery (path resolves to an empty string, passes exclusion checks trivially, then vanishes at the `Utf8PathBuf` conversion). `discover.rs:24,41`
- An explicitly empty `src_dirs` (`[]`, whether via CLI or an explicit `srcDirs: []` in `docgen.config.ts`) short-circuits past the "all src dirs missing" diagnostic entirely — a zero-file, zero-diagnostic run indistinguishable from "correctly configured but genuinely empty." `pipeline/mod.rs:220`
- Two component mappings with identical display name *and* identical file path still collide — the dedup key is `"{name} ({file_path})"` for both; the second silently overwrites the first via `components.insert`. `pipeline/mod.rs:378-388`
- Overlapping `src_dirs` (e.g. `["./src", "./src/components"]`, a realistic monorepo misconfiguration) cause the same file to be discovered and parsed twice, duplicating every `ComponentMapping` for that file and flowing directly into the collision above. `pipeline/mod.rs` + `types/global.rs:75-97`

**CLI:**
- `watch --out` write failures are completely silent: the result of `std::fs::write` is discarded via `let _ = ...`. No error printed, no diagnostic, nothing. `cli/commands/watch.rs:107-112`
- `--out`/`watch --out` writes are non-atomic (`fs::write` truncates then writes) — a mid-write failure (disk full, permission revoked) can leave a truncated, corrupt file at the target path instead of either the old content or nothing.

**Types/serialization:**
- `NumberLiteral(f64)` values that are NaN or Infinity serialize to JSON `null` (serde_json's behavior for non-finite floats), and on deserialization `as_f64().unwrap_or(0.0)` silently turns that into `0.0`. Likely unreachable via normal TS literal syntax, but present. `types/output.rs:314,423`
- Zero-member `LiteralUnion`s are treated as a valid enum by `is_literal_union()`, producing `{"name":"enum","value":[]}` in RDT output with nothing explaining the empty select.

## Priority 3 — contract inconsistencies

| # | Finding | Location |
| --- | --- | --- |
| P3-1 | Exit-code contract (0 = success, 2 = extraction error) is documented and enforced only for `extract`/`check`. `watch` always exits 0 regardless of extraction errors — fine for an interactive dev loop, but a footgun if ever run non-interactively (CI, pre-commit). `inspect` never surfaces Error-severity diagnostics found elsewhere in the tree at all (only fails on a missing-component lookup). | `cli/main.rs:181-198` |
| P3-2 | `DiagnosticCode::Unknown` is defined but never constructed anywhere in the repo — dead code, though the enum is `#[non_exhaustive]` so may be intentional headroom for external consumers. | `types/diagnostic.rs:61` |

## Documented/by-design limitations (listed for completeness, not new findings)

- Ariakit's `Props<T, O> = O & Omit<ComponentPropsWithoutRef<T>, keyof O>` generic pattern is unmatched at the chain level — `composes` stays empty for 6 of the 10 outlier components tracked in `docs/benchmarks.md`. Matches the already-documented gap.
- Mapped types, conditional types, and a couple of `@emotion/styled`-specific call shapes need real type inference OXC deliberately doesn't do — tracked in `docs/type-checker-integration.md` and `rdt-coverage.md`'s known-gaps table.
- Multi-root/monorepo ambient-global (`lib.dom.d.ts` etc.) resolution only inspects `options.src_dirs.first()` — if the first `src_dir` can't resolve `typescript` but a later one could, ambient lookups silently return empty for the whole run. Currently treated as legitimate silent degradation by design; flagged here only because it's untested against an actual multi-root config.
- TOON's format is deliberately lossy relative to canonical JSON (truncated unions, no length cap on descriptions) but this isn't stated in the module's own doc comment — a documentation gap, not a behavior bug.

## Notable non-findings (checked, confirmed sound)

- No `unwrap()`/`expect()`/`panic!()` outside `#[cfg(test)]` anywhere in the extractor, resolver, CLI, or binding crates — non-negotiable #1 holds everywhere it was checked.
- Import re-export cycles are correctly bounded (`MAX_REEXPORT_DEPTH = 8` plus a visited set) with dedicated tests.
- Parallel parse/merge is deterministic: discovery sorts, rayon preserves input order through `collect()`, and the sequential merge phase iterates that same order — diagnostics ordering and dedup winner-selection are reproducible.
- Editor atomic-save patterns (temp file + rename) are explicitly handled in watch mode via `canonicalize_best_effort`.
- Shell injection in config loading is not possible — the config path is passed via `env()`, never interpolated into a shell string, verified across all code paths.

---

## Root-cause analysis and implementation plan

See `root-cause-analysis.md` — the ~30 findings above collapse into 11 mechanism-level root causes plus 7 standalone tickets, each with a concrete fix proposal and a Phase 1 task breakdown grouped by shared-file conflicts. Two clusters (panic containment, schema derivation) are new architectural decisions with a draft ADR each; the rest are enforcement of an existing-but-unenforced pattern. Update the tables above to mark items fixed as Phase 2 implementation lands.
