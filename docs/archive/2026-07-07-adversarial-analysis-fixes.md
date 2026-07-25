# Adversarial Analysis Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the critical/high/medium findings from the 2026-07-07 five-domain adversarial analysis (performance, Rust best-practices, security, UX/DX, output correctness) without introducing merge conflicts, by assigning each team a disjoint set of files.

**Architecture:** Eight independent worktree-isolated agent teams, each owning a distinct file cluster. Every team commits on its own branch; the controller (main session) cherry-picks/merges all eight into `main` sequentially, resolving any incidental overlap the same way the prior 8-team wave did (git 3-way auto-merge handled disjoint line ranges within shared files cleanly last time).

**Tech Stack:** Rust (crates/core, crates/cli), existing insta snapshot tests, cargo clippy/deny gates.

**Source of truth for every fix below:** the five-agent adversarial analysis conducted earlier this session (perf-auditor, rust-auditor, security-auditor, dx-auditor, output-auditor). File:line references are from that analysis; verify current line numbers on read since the file may have shifted slightly.

---

## Explicitly deferred (not in this pass)

To keep team file-ownership disjoint and the wave reviewable, the following confirmed-real findings are deferred to a follow-up pass:

- Scoped-key string allocation caching across `resolver/chain.rs` / `named.rs` / `template.rs` (Borrow-based type-map keys) — real but unbenchmarked, and touches the same files as Team D/H; do as a focused follow-up once this wave lands.
- `cache.rs` unconditional rewrite on every run (no dirty-flag) and no size cap on cache file read — Low severity, requires local write access attacker already has better options against.
- Migration guide / README updates, NAPI package npm publishing, exit-code documentation in `--help` — non-code / release-process items, not Rust fixes.

---

## Task A: Parser safety + extractor silent-drops (Critical)

**Files:**
- Modify: `crates/core/src/extractor/mod.rs`
- Modify: `crates/core/src/types/diagnostic.rs`

**Context:** Two independent Critical findings live in the same function. `extract_type_name_from_type` (mod.rs, ~line 433) only handles `TSTypeReference`, `TSParenthesizedType`, `TSUnionType`, `TSIntersectionType` (the last two added this session) — a bare `TSTypeLiteral` (e.g. `FC<{x: string}>`, `forwardRef<E, {x: string}>`) falls into `_ => None`, and the calling component-detection functions (`try_fc_annotation`, `try_forward_ref`, `try_hoc_wrapped`) silently produce no `ComponentMapping` at all. `stats.componentsSkipped` stays 0; `check` (the CI-oriented subcommand) reports a clean pass. Separately, `parse_file` (mod.rs, ~line 35) calls `oxc_parser::Parser::parse()` and only reads `ret.program` — `ret.errors: Vec<OxcDiagnostic>` (real TS syntax errors, with spans) and `ret.panicked` are never inspected anywhere in the crate. And critically: `oxc_parser` itself has no recursion-depth guard for several recursive-descent grammar productions (nested parens/object literals/conditionals) — a ~13KB adversarial file with ~6,000 nesting levels deterministically stack-overflows and aborts the process (SIGABRT, uncatchable — this happens inside the parser, confirmed via a standalone parser-only harness, before any of this project's code runs).

- [ ] **Step 1: Add a pre-parse nesting-depth guard**

  In `parse_file` (mod.rs), before calling `Parser::new(&allocator, source, source_type).parse()`, do a cheap linear scan of the raw source text counting maximum nesting depth of `(`, `{`, and `[` (a single running counter, incremented on any open bracket, decremented on any close bracket of matching class — track the max value seen, not full stack balance, since we only need to bound depth not verify balance). If the observed depth exceeds a constant `MAX_SOURCE_NESTING_DEPTH: usize = 2000` (comfortably under the ~6,000 crash threshold with margin for future grammar changes), skip parsing this file entirely and return a `SourceData` with a single `Diagnostic` (new code, see Step 2) instead of calling `Parser::parse()`.

  ```rust
  const MAX_SOURCE_NESTING_DEPTH: usize = 2000;

  fn max_bracket_nesting_depth(source: &str) -> usize {
      let mut depth: i64 = 0;
      let mut max_depth: usize = 0;
      for b in source.bytes() {
          match b {
              b'(' | b'{' | b'[' => {
                  depth += 1;
                  if depth as usize > max_depth {
                      max_depth = depth as usize;
                  }
              }
              b')' | b'}' | b']' => depth -= 1,
              _ => {}
          }
      }
      max_depth
  }
  ```

  Wire this into `parse_file` before the `Parser::new(...)` call. On exceeding the threshold, construct a minimal `SourceData` (via `SourceDataCollector::new` + `finish()`, or a direct `SourceData::default()` if that's cleaner) with one `Diagnostic { severity: Error, code: DiagnosticCode::ExcessiveNesting, message: format!("File exceeds maximum type nesting depth ({} > {}), skipped to avoid parser stack overflow", observed, MAX_SOURCE_NESTING_DEPTH), file: path.clone(), .. }` pushed into `data.diagnostics`, and return early without calling the real parser.

- [ ] **Step 2: Add `DiagnosticCode::ExcessiveNesting` and `DiagnosticCode::ParseError`**

  In `crates/core/src/types/diagnostic.rs`, add two new arms to the `DiagnosticCode` enum (near the existing `IoError`, `UnresolvableImport` etc. — follow the exact existing pattern for whatever derives/serde attributes are on that enum):

  ```rust
  ExcessiveNesting,
  ParseError,
  ```

  Run through the `rust-types` skill checklist for these new enum variants (this touches a public type per `crates/core/CLAUDE.md`).

- [ ] **Step 3: Surface OXC's own parse errors as diagnostics**

  In `parse_file`, after `let ret = Parser::new(&allocator, source, source_type).parse();`, check `ret.errors` — for each `OxcDiagnostic` in it, push a `Diagnostic { severity: Error, code: DiagnosticCode::ParseError, message: <the OxcDiagnostic's message>, file: path.clone(), line: <derive from span if available, else None>, column: None, help: None }` onto the collector's diagnostics before proceeding to `collector.visit_program(&ret.program)`. Do this even if `ret.errors` is non-empty — OXC's parser is error-recovering, so `ret.program` is still usable (this is existing OXC behavior; just also report the errors instead of silently ignoring them). Check `OxcDiagnostic`'s public API (`oxc_diagnostics` crate) for how to extract a message string and span — use whatever's idiomatic, don't over-engineer span-to-line/column conversion if it's not already available cheaply elsewhere in this codebase; `line: None, column: None` is acceptable for this fix if a byte-span-to-line-number utility doesn't already exist (this is documented elsewhere as a separate, lower-priority gap, not a blocker for this task).

- [ ] **Step 4: Add a `TSTypeLiteral` arm to `extract_type_name_from_type`**

  Mirror the existing `TSUnionType`/`TSIntersectionType` arms added this session (same function, mod.rs ~line 448 onward). `ts_type_to_collected` (mod.rs, ~line 218) already converts `TSType::TSTypeLiteral` into `CollectedType::Object(fields)` — reuse that:

  ```rust
  TSType::TSTypeLiteral(_) => {
      let collected = self.ts_type_to_collected(ty);
      let bare = format!("__anon_{}", self.data.type_aliases.len());
      let scoped = self.scoped_key(&bare);
      self.data.type_aliases.insert(
          scoped,
          CollectedTypeAlias::Passthrough { target: collected, file_path: self.file_path.clone() },
      );
      Some((bare.into(), vec![]))
  }
  ```

  Confirm `CollectedTypeAlias::Passthrough`'s resolver-side handling (`resolver/alias.rs` or wherever `Passthrough` is matched) already knows how to resolve a `CollectedType::Object(fields)` target into props directly — if it currently assumes `Passthrough.target` is always `CollectedType::Named`, you may need to add an `Object` arm there too (check `resolve_base_as_chain` in resolver/alias.rs, which already has a `CollectedType::Union` arm added this session — add a parallel `CollectedType::Object(fields)` arm that builds a `ResolvedChain` directly from those fields if it doesn't already handle this).

- [ ] **Step 5: Test**

  Write a fixture (or extend `fixtures/rdt-compat/` with a new file, e.g. `fixtures/rdt-compat/inline-object-props.tsx`) with:
  ```tsx
  import * as React from 'react';

  export const Toast = React.forwardRef<HTMLDivElement, { message: string; duration?: number }>(
    ({ message, duration }, ref) => <div ref={ref}>{message}</div>
  );
  Toast.displayName = 'Toast';

  export const Badge: React.FC<{ label: string; variant?: 'info' | 'warning' }> = ({ label, variant }) => (
    <span>{label}</span>
  );
  ```
  Run `cargo test -p oxc-react-docgen-core snapshot_rdt_compat` — expect a new snapshot diff showing `Toast` and `Badge` now extracted with their inline-object props. Accept the new snapshot with `INSTA_UPDATE=always cargo test -p oxc-react-docgen-core`.

  Also write a standalone unit test (in `extractor/mod.rs`'s own `#[cfg(test)] mod tests` or wherever similar extractor unit tests live) asserting `parse_file` on a source string with >2000 nested parens produces exactly one `ExcessiveNesting` diagnostic and zero components, and does NOT crash. Also add a `.tsx` with a deliberate syntax error (e.g. unclosed brace) and assert the resulting `SourceData.diagnostics` contains a `ParseError` entry.

- [ ] **Step 6: Verify and commit**

  ```bash
  cargo clippy -p oxc-react-docgen-core -- -D warnings
  cargo test -p oxc-react-docgen-core
  ```
  Commit as: `fix(extractor): guard against unbounded parser recursion; surface parse errors; extract inline object props`

---

## Task B: JSDoc correctness + performance (High)

**Files:**
- Modify: `crates/core/src/extractor/jsdoc.rs`
- Modify: `crates/core/src/extractor/visit.rs`

**Context:** `find_jsdoc` (jsdoc.rs ~line 15) does `self.comments.iter().rev().find(|c| ...)` on every single call — since callers visit the AST in ascending span order, each call re-scans from the end past every already-processed comment, making comment-lookup O(n²) in comment count for the file. Empirically measured: 517ms for a 32k-prop synthetic file vs. ~16ms if it scaled linearly. Separately, `visit_ts_interface_declaration` (visit.rs ~line 113) collects `props` (each prop's `find_jsdoc` call marks a comment consumed) BEFORE calling `find_jsdoc(node.span.start)` for the interface's own description — so for a short interface where the leading `/** doc */` comment is within the first prop's 120-byte proximity threshold, the first prop's lookup claims and consumes the interface's own comment, leaving the interface `description` empty.

- [ ] **Step 1: Replace the O(n²) scan with a monotonic cursor**

  Add a field to `SourceDataCollector` (mod.rs) tracking the next unexamined comment index, e.g. `comment_cursor: usize` initialized to 0 in `new()`. Since `self.comments` is sorted by span (confirmed: extracted from `ret.program.comments` in original source order), and all callers visit the AST in increasing `span_start` order, `find_jsdoc(span_start)` can advance `comment_cursor` forward past any comment whose `span_end <= span_start - 120` (too far back to ever match again) instead of re-scanning from the end every time, then search forward from the cursor for the nearest unconsumed block comment within the 120-byte window. This changes total work across a file from O(n²) to O(n) amortized.

  Read the current `find_jsdoc` implementation carefully before changing it — preserve exact matching semantics (block comment, span_end <= span_start, `span_start - span_end <= 120`, not already consumed) while changing only the traversal strategy.

- [ ] **Step 2: Fix interface-description ordering**

  In `visit_ts_interface_declaration` (visit.rs ~line 113-139), reorder so `find_jsdoc(node.span.start)` (and `extract_jsdoc_tags`) for the interface's own description runs BEFORE the `node.body.body.iter().filter_map(|sig| self.collect_property_signature(sig))` loop that processes individual props. This ensures the interface's own leading comment is consumed by the interface first; each prop's own `find_jsdoc` call will then correctly fail to find (or find its own, different) comment.

- [ ] **Step 3: Test**

  Add a fixture or unit test: an interface with a leading doc comment and a first property with NO comment of its own, both within 120 bytes of each other:
  ```ts
  /** Props for Button. */
  interface ButtonProps {
    variant: string;
  }
  ```
  Assert the resulting `CollectedInterface.description` is `"Props for Button."` and the `variant` prop's description is empty — currently (before this fix) it's the reverse.

  For the performance fix, no snapshot changes are expected (behavior is unchanged, only traversal strategy) — running the full existing snapshot suite should show zero diffs. If you want to verify the complexity fix empirically, generate a large synthetic `.d.ts` (in `/private/tmp` or a scratch dir, NOT committed) with thousands of JSDoc'd props and confirm `stats.durationMs` scales roughly linearly, not quadratically, but this is optional verification, not a committed test.

- [ ] **Step 4: Verify and commit**

  ```bash
  cargo clippy -p oxc-react-docgen-core -- -D warnings
  cargo test -p oxc-react-docgen-core
  ```
  Commit as: `perf(extractor): replace O(n²) JSDoc comment scan with monotonic cursor; fix interface description ordering`

---

## Task C: Component prop defaults for forwardRef/FC patterns (High)

**Files:**
- Modify: `crates/core/src/extractor/component.rs`

**Context:** `try_forward_ref` (~line 76) and `try_fc_annotation` (~line 30) both hardcode `param_defaults: Default::default()` and never call `extract_param_defaults` — only `try_hoc_wrapped`'s plain-function branch does. This means destructured defaults (`({ variant = 'primary' }, ref) => ...`) are never captured for the two most common real-world authoring patterns (shadcn/Radix-style `forwardRef` components, and plain `FC`-typed arrow functions with destructured defaults) — confirmed on the real `fixtures/shadcn/button.tsx` fixture, where `asChild`'s `defaultValue` is `null` despite `asChild = false` in the source.

- [ ] **Step 1: Wire defaults into `try_forward_ref`**

  `try_forward_ref` already has `call: &CallExpression` (the `forwardRef(...)` call) in scope. Find the render-function argument (likely `call.arguments.last()` or a specific index — check how `try_hoc_wrapped` locates its function argument for the pattern to follow) — it's typically an arrow function or function expression: `({ variant, size }, ref) => ...`. If the first parameter is a destructured object pattern with defaults, call the existing `extract_param_defaults` (used already by `try_hoc_wrapped`) on it and populate `param_defaults` in the returned `ComponentMapping` instead of `Default::default()`.

- [ ] **Step 2: Wire defaults into `try_fc_annotation`**

  `try_fc_annotation` has `decl: &VariableDeclarator` in scope. Its `decl.init` (the assigned value, e.g. `({ variant = 'primary' }) => ...`) — if it's an `Expression::ArrowFunctionExpression` or `Expression::FunctionExpression`, extract its first parameter the same way and call `extract_param_defaults`.

- [ ] **Step 3: Test**

  Extend `fixtures/rdt-compat/` (or add a new fixture) with:
  ```tsx
  export const Toggle = React.forwardRef<HTMLButtonElement, ToggleProps>(
    ({ pressed = false, size = 'md' }, ref) => <button ref={ref} />
  );

  export const Chip: React.FC<ChipProps> = ({ label, closable = true }) => <span>{label}</span>;
  ```
  Run the snapshot suite, confirm `pressed`, `size`, `closable` now show real `defaultValue` entries (RDT-shaped: `{"value": "false", "computed": false}` etc. — match whatever format `extract_param_defaults` already produces for the working `try_hoc_wrapped` path, confirmed via a synthetic `React.memo(function Widget({size='md'}: Props){...})` test in the adversarial analysis).

- [ ] **Step 4: Verify and commit**

  ```bash
  cargo clippy -p oxc-react-docgen-core -- -D warnings
  cargo test -p oxc-react-docgen-core
  ```
  Commit as: `fix(extractor): capture destructured param defaults in forwardRef and FC-typed components`

---

## Task D: Resolver correctness + cva/variant-props performance (High/Medium)

**Files:**
- Modify: `crates/core/src/resolver/alias.rs`
- Modify: `crates/core/src/resolver/func.rs`
- Modify: `crates/core/src/known.rs`

**Context:** Two independent issues. (1) `resolve_union_alias` (resolver/alias.rs ~line 193-221) merges props from union members with `merged_props.entry(prop.name.clone()).or_insert_with(...)` — pure first-member-wins for every prop except the discriminant (which is correctly unioned). Confirmed via the project's own compare-vs-real-RDT harness as an actual regression, not a design tradeoff: real RDT produces `string | string[]` for Accordion's `value`; this tool produces `string` only. (2) `resolve_typeof` (resolver/func.rs ~line 67) and `resolve_cva_variant_props` (known.rs ~line 123) both do `ctx.global.enums.iter().find(|(key, _)| key.ends_with(&format!(":{}", name)) || ...)` — a full linear scan of every enum/cva/tv/recipe entry in the whole project, with a fresh `format!()` allocation checked against every element, for every `typeof X` / `VariantProps<...>` reference. Empirically confirmed superlinear in practice (11x slowdown for 4x growth in declaration count, likely from cache pressure at scale).

- [ ] **Step 1: Fix union-member prop merging to union conflicting types**

  In `resolve_union_alias`, when merging a prop that already exists in `merged_props` with a different type than the incoming member's version of that prop, combine them into a `PropType::Union` (or whatever the existing union-representation convention is — check how the discriminant prop's own union-of-literals is already built a few lines below, at ~line 212-221, and reuse the same mechanism) rather than keeping only the first. Two members contributing the *same* type for a prop should NOT wrap in a redundant single-element union — only actually-differing types get unioned. Also make non-discriminant required/optional flags correct: if a prop is required in one member and optional in another, the merged result should be optional (since the union type doesn't always require it).

  Be careful: this needs to correctly handle N-way unions (not just 2), and needs to distinguish "prop present in this member with type X" from "prop absent from this member" (RDT's real behavior when a prop is entirely absent from one union branch is presumably to make it optional in the merged result — verify against the compare harness's real-RDT output for Accordion, which is the canonical test case here).

- [ ] **Step 2: Precompute a bare-name lookup for enums/cva/variant declarations**

  Wherever `ResolutionContext` (or equivalent) is constructed once per resolution pass (check `resolver/mod.rs` or wherever `ctx.global` is set up), build an additional `FxHashMap<CompactString, &Vec<EnumEntry>>` (or owned `Vec<EnumEntry>` clone if lifetime/borrow issues make a reference awkward — prefer a reference if `GlobalSourceData` is already `Arc`-wrapped and outlives resolution, per the existing pattern) keyed by the bare (non-scoped) name, built once by iterating `global.enums` a single time and stripping the `file:` prefix from each key. Then `resolve_typeof` and `resolve_cva_variant_props` become simple `O(1)` hashmap lookups instead of linear scans with per-element allocation. Follow the same pattern `ImportResolutionMap::build` already uses elsewhere in this codebase for a precomputed lookup structure.

  Note: if two different files declare an enum/cva/tv with the same bare name, the current linear-scan code has the same ambiguity (first `.find()` match wins) — the precomputed map should preserve identical tie-breaking behavior (e.g. by using `.entry(...).or_insert(...)` so the first-inserted-wins, matching iteration order) rather than silently changing which one wins. If `global.enums`' iteration order isn't deterministic (it's an `FxHashMap`), note this as a pre-existing ambiguity, not a regression you're introducing — don't try to fix the ambiguity itself in this task.

- [ ] **Step 3: Test**

  For Step 1: run `cargo test -p oxc-react-docgen-core snapshot_rdt_compat`, expect the Accordion snapshot to change — `value`, `defaultValue`, `onValueChange` should now show unioned types (`string | string[]` etc.) matching real RDT's output (cross-check against the adversarial analysis's quoted real-RDT diff: `defaultValue: rdt="string | string[]"`, `onValueChange: rdt="((value: string) => void) | ((value: string[]) => void)"`, `value: rdt="string | string[]"`).

  For Step 2: no behavior change expected (pure perf refactor) — full snapshot suite should show zero diffs. Optionally verify with a synthetic large-cva-count stress test in a scratch dir, not committed.

- [ ] **Step 4: Verify and commit**

  ```bash
  cargo clippy -p oxc-react-docgen-core -- -D warnings
  cargo test -p oxc-react-docgen-core
  ```
  Commit as: `fix(resolver): union conflicting prop types across discriminated-union members; precompute enum lookup for typeof/variant-props`

---

## Task E: CLI validation & config honesty (Critical)

**Files:**
- Modify: `crates/cli/src/config.rs`
- Modify: `crates/cli/src/commands/check.rs`
- Modify: `crates/cli/src/commands/extract.rs`
- Modify: `crates/cli/src/output.rs`
- Modify: `crates/core/src/pipeline/mod.rs` (or wherever `--src` directory resolution/validation currently happens — read `pipeline/discover.rs` first to find the right insertion point)

**Context:** Three independent Critical DX findings, all sharing the "silent success instead of loud failure" pattern. (1) `try_load_config` (config.rs ~line 21-55) fully parses and executes the user's `docgen.config.ts` via node+tsx, JSON-parses the result to validate it, then unconditionally returns `None` (explicit TODO stub) — so a real config with e.g. a different `srcDirs` is silently ignored and the tool falls back to defaults with zero signal. (2) Neither `extract` nor `check` validates that `--src` resolves to an existing directory — a typo'd path exits 0 with an empty-but-valid result, meaning `check` (documented "for CI") can't detect its own misconfiguration. (3) `extract`'s default (no `--out`, no `--json`, no `--quiet`) stdout interleaves a human-readable summary (`print_summary`/`print_diagnostics` in output.rs, called via bare `println!`) with the JSON blob, breaking `| jq .`.

- [ ] **Step 1: Hard-error on stubbed config**

  In `config.rs`, at the point where `try_load_config` currently returns `None` after successfully parsing (the stub), instead return an error (check what error type this function should propagate — likely needs to change its return type from `Option<T>` to `Result<Option<T>, ConfigError>` or similar, or the caller needs to distinguish "no config found" from "config found but unusable"). Distinguish two cases clearly:
  - No `docgen.config.ts` found at all → current default behavior, fine, no error.
  - A `docgen.config.ts` IS found (and successfully executes/parses) → since schema mapping to `PipelineOptions` isn't implemented yet, this must be a hard error surfaced to the CLI caller (non-zero exit, a clear miette-rendered message like "docgen.config.ts was found but config file support is not yet implemented — remove the file or file an issue", not a silent fallback to defaults).

  Trace the call site in `crates/cli/src/main.rs` or wherever `load_config_file`/`try_load_config` is invoked from the `extract`/`check`/`watch` command handlers, and propagate the new error path through to a nonzero exit with a miette-rendered message (follow existing miette usage patterns in this codebase, e.g. in `extract.rs`).

- [ ] **Step 2: Validate `--src` exists**

  Find where `--src` argument(s) get turned into directories the pipeline scans (likely in `pipeline/mod.rs`'s top-level `extract()` entry point, or in `discover.rs`). Before running discovery, check each resolved `--src` path exists on disk (`Utf8Path::exists()` or similar) — if none of the provided/default src paths exist, push an Error-severity `Diagnostic` (reuse `DiagnosticCode::IoError` or add a new variant if more specific is warranted — check the existing enum first) into the output AND make sure the CLI-level exit-code logic (already present in `extract.rs` — it already exits 2 on Error-severity diagnostics per the DX audit's positive findings) picks this up naturally. Confirm `check`'s exit-code logic (check.rs ~line 26-31) also correctly treats this as an Error diagnostic causing a nonzero exit, not just `extract`.

- [ ] **Step 3: Default `extract` stdout must be pure JSON**

  In `output.rs`, the human-readable summary/diagnostics printers (`print_summary`, `print_diagnostics`) currently write to stdout via bare `println!` unconditionally. Change them to always write to stderr (not just conditionally under `--json`), so stdout is reserved exclusively for the actual JSON payload in every mode. Check `extract.rs`'s call sequence (~line 38-51) — the summary/diagnostics print calls should use `eprintln!` (or whatever this codebase's stderr-writing convention is — check if there's a tracing/logging abstraction already used elsewhere per `crates/cli/CLAUDE.md`'s "All user-facing output goes through output.rs or the tracing subscriber" rule) instead of `println!`, unconditionally, regardless of `--json`/`--quiet` flags. The JSON result itself remains the only thing ever written to stdout by default.

  Verify this doesn't break the `--quiet` flag's existing meaning (check what `--quiet` currently suppresses — if it already suppressed the stdout summary, its behavior post-fix might become a no-op for that specific suppression, which is fine, but check it doesn't now also suppress something it shouldn't).

- [ ] **Step 4: Test**

  - Run `oxc-react-docgen extract --src fixtures/rdt-compat` with no other flags, capture stdout separately from stderr (`... 1>stdout.txt 2>stderr.txt`), assert `stdout.txt` is valid JSON via `jq . stdout.txt` (or equivalent) and `stderr.txt` contains the human summary.
  - Run `oxc-react-docgen check --src /does/not/exist`, assert nonzero exit code.
  - Write a `docgen.config.ts` in a scratch test fixture dir, run `extract --config <path>`, assert nonzero exit with a clear message (not a silent fallback).
  - Add or extend integration tests in `crates/cli/tests/` (check what test harness already exists there, e.g. `assert_cmd`-style tests if present) covering these three cases; if no CLI integration test harness exists yet, add minimal ones following whatever pattern the existing test setup suggests, or a shell-based smoke test if that's more consistent with this project's conventions — check first.

- [ ] **Step 5: Verify and commit**

  ```bash
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
  Commit as: `fix(cli): hard-error on unsupported config files; validate --src exists; keep stdout pure JSON by default`

---

## Task F: Watch-mode diagnostics + type-alias silent-drop (Medium/High)

**Files:**
- Modify: `crates/core/src/pipeline/watch.rs`
- Modify: `crates/cli/src/commands/watch.rs`
- Modify: `crates/core/src/extractor/alias.rs`

**Context:** (1) `WatchSession::update_file` (pipeline/watch.rs) correctly computes diagnostics per update, but `WatchSession::snapshot()` (~line 104-111) hardcodes `diagnostics: vec![]` — and `cmd_watch` (cli/commands/watch.rs ~line 88-102) never reads `update.diagnostics` either. A test, `snapshot_after_initialize_has_no_diagnostics` (pipeline/watch.rs ~220-226), currently locks in the empty-diagnostics behavior as expected — this test's name and assertion need to change once the fix lands (it's asserting the bug, not a feature). This same test is also the source of the current `cargo clippy --all-targets -- -D warnings` failure (an ignored `#[must_use]` return from `session.initialize()`). (2) `classify_type_alias` (extractor/alias.rs ~line 12-107) has the same silent-`None` shape as Task A's `extract_type_name_from_type`: a type alias RHS that isn't `TSTypeReference`/`TSUnionType`/`TSIntersectionType`/`TSParenthesizedType` (e.g. `type Foo = {a: string}`) vanishes from `data.type_aliases` with zero diagnostic.

- [ ] **Step 1: Accumulate diagnostics in `WatchSession`**

  Add a field to `WatchSession` (or wherever its state lives) to accumulate diagnostics across `update_file` calls — e.g. `diagnostics: Vec<Diagnostic>` behind whatever synchronization this struct already uses (check if `WatchSession` is already behind a `Mutex`/`RwLock` for other state, given it's driven by a file-watcher callback — follow the existing concurrency pattern rather than introducing a new one). Each `update_file` call should append its `IncrementalUpdate.diagnostics` to this accumulator (decide: does this ever need to clear/expire old diagnostics for files that were fixed since? Check `IncrementalUpdate`'s semantics — likely the right behavior is to replace, not append, this file's prior diagnostics on each re-update, keyed by file path, rather than growing unbounded. Look at how `GlobalSourceData::remove_file` handles the analogous "replace this file's contributions" pattern for interfaces/type_aliases/etc., and mirror that for diagnostics).

- [ ] **Step 2: Surface diagnostics from `snapshot()` and `cmd_watch`**

  Change `WatchSession::snapshot()` to include the accumulated diagnostics instead of `vec![]`. In `cmd_watch` (cli/commands/watch.rs), read `update.diagnostics` (or the session's accumulated set, whichever is more correct per Step 1's design) and print them via the CLI's existing diagnostic-printing path (reuse `print_diagnostics` from `output.rs` — the same one Task E modified to write to stderr).

- [ ] **Step 3: Fix the test that locks in the bug, and the clippy failure**

  Update `snapshot_after_initialize_has_no_diagnostics` (pipeline/watch.rs ~220-226) — rename and rewrite it to assert the correct new behavior (diagnostics ARE surfaced when they exist; a clean file after `initialize()` legitimately has none, so verify the test's actual fixture content to determine what its assertion should now say). While here, fix the immediate clippy failure: `session.initialize();` on its own discards a `#[must_use]` `ExtractionOutput` — either use the return value meaningfully in the test (likely: assert something about it, which may already be the intent) or explicitly `let _ = session.initialize();` if truly unused after this fix, with a comment explaining why.

- [ ] **Step 4: Add `TSTypeLiteral` handling to `classify_type_alias`**

  Mirror Task A's Step 4 fix, in `extractor/alias.rs`'s `classify_type_alias`: add a `TSType::TSTypeLiteral` arm producing `CollectedTypeAlias::Passthrough { target: self.ts_type_to_collected(ty), file_path: fp }` (reusing the same `Object(fields)` conversion), instead of falling to `_ => None`.

- [ ] **Step 5: Test**

  For watch diagnostics: write a test that creates a `WatchSession`, calls `update_file` on a file with an unresolvable import (or another diagnostic-producing condition), calls `snapshot()`, and asserts the resulting `ExtractionOutput.diagnostics` is non-empty.

  For the type-alias fix: add a fixture with `type ToastVariant = { message: string; kind?: 'info' | 'error' };` referenced by a component, confirm it now resolves instead of silently failing.

- [ ] **Step 6: Verify and commit**

  ```bash
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
  Commit as: `fix(pipeline): surface watch-mode diagnostics; extract inline object type aliases`

---

## Task G: Output format fidelity (Medium)

**Files:**
- Modify: `crates/cli/src/commands/extract.rs` (the `serialize_rdt` function)
- Modify: `crates/core/src/types/output.rs`

**Context:** Three independent RDT-compat gaps. (1) `serialize_rdt()` (extract.rs ~line 62-91) omits `methods` and `tags` — real RDT `ComponentDoc` always includes `methods: Method[]` (even when empty) — ironic since `output.rs`'s own doc comment on the `methods` field says it's "present for RDT compat." (2) `is_literal_union()` (output.rs ~line 206-217) exists with a doc comment saying it should let serializers choose between RDT's `"enum"` and `"union"` type-name convention, but has zero callers anywhere — literal-union props (`variant`, `size`, `type`, etc. — the most common curated props in any design system) never get RDT's `{name: "enum", value: [...]}` shape, so Storybook-style `<select>` controls that pattern-match on `type.name === "enum"` never activate for them. (3) The hand-rolled `Serialize` impl for `PropType` (output.rs ~line 264-330) uses a bare positional `"0"` JSON key for tuple-style enum variants (`{"kind":"union","0":[...]}`, `{"kind":"stringLiteral","0":"primary"}`) while struct-style variants get real field names — inconsistent and awkward for consumers (this propagates into the hand-written `packages/napi/index.d.ts` types as a literal `0` field).

- [ ] **Step 1: Add `methods`/`tags` to `serialize_rdt`**

  In `serialize_rdt()`, add `"methods": []` (matching what `--format storybook`'s serializer already does for the same field — check that serializer for the exact JSON shape to match) and a `tags` field sourced from the component's existing `tags` data (check `ComponentEntry`'s field name for JSDoc tags) to the RDT-format output.

- [ ] **Step 2: Wire `is_literal_union()` into the RDT serializer**

  In `serialize_rdt()` (or wherever `PropType`→RDT-type-string conversion happens for this format), call the existing `is_literal_union()` helper — when true, emit RDT's convention: `{"name": "enum", "value": [{"value": "\"single\""}, {"value": "\"multiple\""}]}` (verify exact RDT shape against `docs/rdt-coverage.md` or the `apps/validate` compare harness's real-RDT baseline output for a literal-union prop) instead of inlining the literal text into `type.name` as a plain string.

- [ ] **Step 3: Give tuple-style `PropType` JSON variants real field names**

  In the hand-rolled `Serialize` impl (output.rs ~line 264-330), replace the positional `"0"` key with descriptive names per variant — e.g. `union`/`intersection` → `"members"`, `tuple` → `"elements"`, `stringLiteral`/`numberLiteral`/`boolLiteral` → `"value"`. Update `packages/napi/index.d.ts`'s hand-written types to match (grep for the literal `0` field pattern there and rename alongside). This changes the wire format — check whether any snapshot tests assert the old `"0"` shape and update them accordingly; this is an intentional, documented breaking change to the JSON schema, not a bug regression.

- [ ] **Step 4: Test**

  Run the full snapshot suite — expect diffs across every fixture with a literal-union or tuple/union `PropType` (should be most of them). Regenerate with `INSTA_UPDATE=always cargo test -p oxc-react-docgen-core` and spot-check 2-3 of the new snapshots by eye to confirm the new field names read correctly and the `"enum"` convention shows up for literal unions in `--format rdt` output specifically (add a small `--format rdt` CLI-level test/fixture check if one doesn't already exist).

- [ ] **Step 5: Verify and commit**

  ```bash
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
  Commit as: `fix(output): match RDT's methods/tags/enum conventions; name tuple PropType JSON fields`

---

## Task H: Path normalization + low-hanging safety/perf (Medium/Low)

**Files:**
- Modify: `crates/core/src/pipeline/discover.rs` (or wherever `Utf8PathBuf` is first constructed from a `--src` argument — read `pipeline/mod.rs` first to confirm the right insertion point)
- Modify: `crates/core/src/resolver/chain.rs`
- Modify: `crates/core/src/resolver/template.rs`
- Modify: `crates/core/src/import_map.rs`
- Modify: `crates/cli/src/commands/extract.rs`
- Modify: `crates/cli/src/commands/watch.rs`

**Context:** Several independent Low/Medium items that don't warrant their own team. (1) `parent.fileName`/`filePath` in output is a verbatim echo of whatever string was passed via `--src` — confirmed to vary (relative, absolute, `./`-prefixed) purely based on invocation context, breaking any consumer that filters/dedupes by file path across CI vs local runs. (2) Three `unwrap()` calls exist in non-test code, violating the project's own "no unwrap() outside #[cfg(test)]" rule: `resolver/chain.rs:252` (`parent_ref.clone().unwrap()`, provably safe today since `parent_ref` is unconditionally `Some` two lines earlier, but a landmine if that ever changes without the compiler noticing), `resolver/template.rs:24` (`values.into_iter().next().unwrap()`, guarded by a length check above it), and `cli/commands/extract.rs:19` + `watch.rs:26` (`ProgressStyle::default_spinner().template(...).unwrap()` on a static string). (3) `import_map.rs`'s `find_import` (~line 91-94) clones a whole `Utf8PathBuf` to build a lookup key on every call.

- [ ] **Step 1: Canonicalize file paths at discovery time**

  Find where discovered file paths first become `Utf8PathBuf`s that flow into `SourceData.file_path` / `ComponentMapping.file_path` (likely in `discover.rs`'s walk callback, or immediately after). Canonicalize each path to an absolute path at that point (e.g. via `camino::Utf8Path::canonicalize_utf8()` or equivalent) rather than leaving it as whatever relative/absolute form the user's `--src` argument happened to take. This should make `parent.fileName` in output stable regardless of whether the user ran with `-s fixtures/x`, `-s $(pwd)/fixtures/x`, or `cd fixtures/x && -s .`.

  Check whether any existing snapshot tests currently assert relative paths in their expected output (the `[ROOT]` redaction in `snapshots.rs` suggests paths are already normalized for test purposes — this fix should be compatible with that redaction, potentially making it simpler since paths will now consistently be absolute).

- [ ] **Step 2: Remove the three non-test `unwrap()` calls**

  - `resolver/chain.rs:252`: restructure so the `PropParent` is built once as a plain (non-`Option`) value where it's first constructed (a few lines above, per the audit), and only wrapped in `Some(...)` at the point where `Option` is actually needed — eliminating the later `.unwrap()` entirely rather than guarding it.
  - `resolver/template.rs:24`: replace `values.into_iter().next().unwrap()` (guarded by `if values.len() == 1`) with `if let [only] = values.as_slice() { return PropType::StringLiteral(only.clone()); }` — removes the unwrap by construction.
  - `cli/commands/extract.rs:19` and `watch.rs:26`: these build a `ProgressStyle` from a static, always-valid template string. If a genuinely infallible construction path exists (check `indicatif`'s API for a non-`Result`-returning constructor, or whether the template can be validated at compile time some other way), use it. If not, this is one of the rare cases where the underlying operation truly cannot fail — leave a `.expect("static progress template is always valid")` with exactly that justification as a comment, since `.expect()` with a clear invariant-explaining message is the documented escape hatch for provably-infallible operations in most Rust style guides, even under a strict "no unwrap" rule (confirm this reading is consistent with the `rust-style` skill's intent before proceeding — if the skill's intent is stricter than this, keep `.unwrap()` but flag it in the commit message as a known, deliberately-accepted exception).

- [ ] **Step 3: Avoid cloning `Utf8PathBuf` per lookup in `find_import`**

  In `import_map.rs`'s `find_import` (~line 91-94), avoid `(file.to_owned(), CompactString::from(local_name))` as a per-call lookup key. If the map is keyed by an owned `(Utf8PathBuf, CompactString)` tuple, change the lookup to use a borrowed-key-compatible approach (e.g. implement `Borrow<(&Utf8Path, &str)>` for the owned key type, or restructure the map to be nested — `FxHashMap<Utf8PathBuf, FxHashMap<CompactString, ImportBinding>>` — so the outer lookup borrows `&Utf8Path` directly without cloning). Pick whichever approach requires the smaller diff against the existing map structure.

- [ ] **Step 4: Test**

For Step 1: run the full snapshot suite, update snapshots if the `[ROOT]` redaction needs adjusting for now-consistently-absolute paths (check `redact_paths` in `snapshots.rs` still does its job). For Steps 2-3: `cargo clippy --workspace --all-targets -- -D warnings` should show no new warnings; no behavior change expected, so the full test suite should be green with zero snapshot diffs from these two steps specifically.

- [ ] **Step 5: Verify and commit**

  ```bash
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
  Commit as: `fix(core): canonicalize discovered file paths; remove non-test unwrap() calls; avoid per-lookup path clone in import_map`

---

## Integration (controller, after all 8 tasks land on their branches)

- [ ] Cherry-pick or merge each team's branch into `main` one at a time, in an order that minimizes conflict risk: A, B, C, D, F, G, H, E last (E touches `output.rs` which F and G's stderr/print changes may also touch — resolve by hand if needed, same as the prior wave's `mod.rs`/`alias.rs` overlaps auto-merged cleanly).
- [ ] After each merge, run `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` before proceeding to the next.
- [ ] Once all 8 are integrated: `cargo deny check advisories` (fix any new advisory the same way `anyhow`/`crossbeam-epoch` were handled this session), then a final full `cargo test --workspace` + `cargo build --release`.
- [ ] Re-run `apps/validate`'s compare harness (`npx tsx src/compare.ts` from `apps/validate/`, regenerating the `oxc-react-docgen` baseline first per the process caveat noted in this session's output-correctness audit) to confirm the discriminated-union and inline-object-props fixes show up as real improvements against RDT.
- [ ] Update `docs/rdt-coverage.md` to mark the fixed bugs resolved (Pick/Omit-in-extends already updated last session; this pass additionally resolves inline-object-props silent drop, discriminated-union type narrowing, and destructured-defaults-in-forwardRef).
