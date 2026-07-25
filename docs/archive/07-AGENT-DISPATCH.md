# Agent Dispatch Summary

# How to send these to Claude Code

## Overview

9 Claude Code sessions in total, across 6 phases. Each session gets: its briefing document + the master plan + types.rs (after Phase 1a).

## Phase 0 — Repository Setup

**Sessions: 1** **Model: claude-sonnet-4-6** **Blocking: YES — all other agents wait for this**

Send: 00-MASTER-PLAN.md + 01-PHASE0-REPO-SETUP.md

Prompt to Claude Code:

> "Set up the oxc-react-docgen repository according to the attached plan. Create all workspace files, stub source files, CI workflow, and fixture directory structure. Run `cargo build` and `cargo test` to verify everything compiles before finishing. Do not implement any logic — only structure, Cargo.toml files, and empty stubs."

---

## Phase 1a — Types

**Sessions: 1** **Model: claude-sonnet-4-6** **Blocking: YES — Phase 2 agents wait for types.rs**

Send: 00-MASTER-PLAN.md + 02-PHASE1A-TYPES.md

Prompt to Claude Code:

> "Implement crates/core/src/types.rs exactly as specified. This file is the contract every other agent depends on — correctness is paramount. Run `cargo build -p oxc-react-docgen-core` to verify it compiles. Run `cargo clippy -p oxc-react-docgen-core -- -D warnings` to verify no warnings. Do not add any logic — only type definitions, derives, and simple inherent methods."

---

## Phase 1b — Fixtures

**Sessions: 1 (parallel with Phase 1a)** **Model: claude-haiku-4-5** **Non-blocking — Phase 2 can start with or without fixtures**

Send: 00-MASTER-PLAN.md

Prompt to Claude Code:

> "Set up the fixtures directory for oxc-react-docgen. Run fixtures/update-fixtures.sh to pull real .d.ts files from npm. Additionally, create fixtures/shadcn/button.tsx that matches the real shadcn/ui Button component source code (copy it from the shadcn/ui GitHub repository). Create fixtures/panda/button.tsx that demonstrates PandaCSS recipe usage. These fixtures are used by all integration tests."

---

## Phase 2a — Extractor

**Sessions: 1 (after Phase 1a)** **Model: claude-sonnet-4-6**

Send: 00-MASTER-PLAN.md + 02-PHASE1A-TYPES.md + 03-PHASE2A-EXTRACTOR.md

Prompt to Claude Code:

> "Implement crates/core/src/extractor.rs and crates/core/src/react_types.rs as specified in the attached plan. The types.rs contract is already implemented — import from crate::types. Critical constraint: no AST references may escape the parse_file function. The allocator is local to that function. All returned data must be owned. Write tests against the fixture files in fixtures/. Run cargo test -p oxc-react-docgen-core and cargo clippy -- -D warnings."

---

## Phase 2b — Import Map

**Sessions: 1 (parallel with Phase 2a and 2c)** **Model: claude-sonnet-4-6**

Send: 00-MASTER-PLAN.md + 02-PHASE1A-TYPES.md + 04-PHASE2B2C-3A3B.md (ImportMap section only)

Prompt to Claude Code:

> "Implement crates/core/src/import_map.rs as specified. This module builds the ImportResolutionMap from GlobalSourceData. It handles re-export chain following (export { Foo as Bar } from './x'). Write unit tests for the three re-export patterns: named, wildcard, and namespace. Use FxHashMap for all internal maps. Run cargo test and cargo clippy."

---

## Phase 2c — Known Patterns

**Sessions: 1 (parallel with Phase 2a and 2b)** **Model: claude-sonnet-4-6**

Send: 00-MASTER-PLAN.md + 02-PHASE1A-TYPES.md + 04-PHASE2B2C-3A3B.md (Known section only)

Prompt to Claude Code:

> "Implement crates/core/src/known.rs — the known type pattern resolver. This is a single match function, not a trait. Add one arm per pattern. Patterns to implement: VariantProps/RecipeVariantProps, SxProps/SystemStyleObject, RenderProps, SlotProps, HTMLChakraProps/HTMLArkProps, PropsWithChildren, ComponentPropsWithoutRef/ComponentPropsWithRef, OverridableStringUnion. Each arm should be documented with which library uses it. Run cargo test and cargo clippy."

---

## Phase 3a — Resolver

**Sessions: 1 (after Phase 2 complete)** **Model: claude-sonnet-4-6**

Send: 00-MASTER-PLAN.md + 02-PHASE1A-TYPES.md + 04-PHASE2B2C-3A3B.md (Resolver section) + 06-SERIALIZERS-TESTS-GOTCHAS.md (Critical Gotchas section)

Prompt to Claude Code:

> "Implement crates/core/src/resolver.rs and crates/core/src/cache.rs. The resolver is the most complex module. It takes a ComponentMapping and produces a ComponentEntry by recursively resolving type references. Use oxc_resolver (NOT a custom resolver) for import specifier → file path resolution. Never fail silently — always emit a Diagnostic when a type can't be resolved. RDT propFilter compatibility is critical: parent.fileName must point to actual .d.ts paths. Run cargo test with fixture-based tests. Run cargo clippy."

---

## Phase 3b — Pipeline

**Sessions: 1 (parallel with Phase 3a)** **Model: claude-sonnet-4-6**

Send: 00-MASTER-PLAN.md + 02-PHASE1A-TYPES.md + 04-PHASE2B2C-3A3B.md (Pipeline section)

Prompt to Claude Code:

> "Implement crates/core/src/pipeline.rs. The pipeline orchestrates: file discovery (ignore crate), parallel parsing (rayon), GlobalSourceData merge (sequential), parallel resolution (rayon), output collection. Also implement WatchSession with DashMap + ArcSwap for incremental updates. The full_pipeline benchmark must complete in < 10ms for the shadcn fixtures. Run cargo bench to verify. Run cargo test. Run cargo clippy."

---

## Phase 4a — NAPI

**Sessions: 1 (after Phase 3 complete)** **Model: claude-sonnet-4-6**

Send: 00-MASTER-PLAN.md + 05-PHASE4-5-NAPI-CLI-PLUGINS.md (NAPI section) + 06-SERIALIZERS-TESTS-GOTCHAS.md (NAPI thread safety gotcha)

Prompt to Claude Code:

> "Implement crates/napi/src/lib.rs and packages/napi/index.d.ts. Expose three NAPI functions: extractAll(), extractFileIncremental(), createSession(), closeSession(). All return JSON strings — not complex NAPI types. Session state uses LazyLock<DashMap<u32, Arc<WatchSession>>> for thread safety. Generate TypeScript types via napi-derive. Run `cargo build -p oxc-react-docgen-napi`. Test that the compiled binary can be loaded from Node.js."

---

## Phase 4b — CLI

**Sessions: 1 (parallel with Phase 4a)** **Model: claude-sonnet-4-6**

Send: 00-MASTER-PLAN.md + 05-PHASE4-5-NAPI-CLI-PLUGINS.md (CLI section)

Prompt to Claude Code:

> "Implement crates/cli/src/main.rs with four subcommands: extract, watch, check, inspect. Use clap with derive macros. Use miette with fancy feature for error display. The inspect command is the key differentiator — it must show props in a clean table with prop name, type, required/optional, and parent interface. Add clap_complete for shell completions and clap_mangen for man page. Exit code 2 on any Error-severity diagnostic. Run cargo clippy."

---

## Phase 5a — Vite Plugin

**Sessions: 1 (after Phase 4a NAPI)** **Model: claude-sonnet-4-6**

Send: 00-MASTER-PLAN.md + 05-PHASE4-5-NAPI-CLI-PLUGINS.md (Vite plugin section) + 06-SERIALIZERS-TESTS-GOTCHAS.md (Vite enforce:pre gotcha)

Prompt to Claude Code:

> "Implement packages/vite-plugin/src/index.ts. The plugin wraps the NAPI binary — zero re-implementation of extraction logic. Critical: enforce: 'pre' to see original TypeScript source. Use Vite's createFilter for include/exclude patterns. Handle the timing: extraction is async, use a promise gate before transform runs. Support propFilter option for RDT compatibility. Write TypeScript tests using vitest against a toy Vite config."

---

## Phase 5b — Rolldown Plugin

**Sessions: 1 (after Phase 3 complete, parallel with 5a)** **Model: claude-sonnet-4-6**

Send: 00-MASTER-PLAN.md + 05-PHASE4-5-NAPI-CLI-PLUGINS.md (Rolldown section)

Prompt to Claude Code:

> "Implement packages/rolldown-plugin as a native Rust rolldown plugin. It uses the core crate directly — no NAPI boundary. The Rolldown native plugin API may still be stabilizing — check current docs. If the native API is not stable, implement as a TypeScript wrapper over NAPI (same pattern as the Vite plugin) and leave a clear TODO for native migration. Document in README which approach was taken and why."

---

## Phase 6 — Integration Tests

**Sessions: 1 (after Phase 5 complete)** **Model: claude-sonnet-4-6**

Send: 00-MASTER-PLAN.md + 06-SERIALIZERS-TESTS-GOTCHAS.md

Prompt to Claude Code:

> "Write integration tests in tests/integration/. The critical test: RDT propFilter compatibility for shadcn, Radix, MUI, React Aria. Use insta for snapshot testing — run `cargo insta review` to approve initial snapshots. Write divan benchmarks for single file parse and full pipeline. Verify the SLOs:
>
> - parse_single_file: < 10µs
> - full_pipeline (50 components): < 10ms Run cargo test --workspace and cargo bench."

---

## Model Recommendations Summary

| Phase | Agent           | Model             | Why                                          |
| ----- | --------------- | ----------------- | -------------------------------------------- |
| 0     | Repo Setup      | claude-sonnet-4-6 | Needs to understand full workspace structure |
| 1a    | Types           | claude-sonnet-4-6 | Most critical file — needs careful design    |
| 1b    | Fixtures        | claude-haiku-4-5  | Simple file operations, npm commands         |
| 2a    | Extractor       | claude-sonnet-4-6 | Complex OXC AST traversal                    |
| 2b    | Import Map      | claude-sonnet-4-6 | Re-export chain logic                        |
| 2c    | Known Patterns  | claude-sonnet-4-6 | Needs deep library knowledge                 |
| 3a    | Resolver        | claude-sonnet-4-6 | Most complex logic, many edge cases          |
| 3b    | Pipeline        | claude-sonnet-4-6 | Rayon + async coordination                   |
| 4a    | NAPI            | claude-sonnet-4-6 | NAPI-rs patterns, thread safety              |
| 4b    | CLI             | claude-sonnet-4-6 | miette + clap integration                    |
| 5a    | Vite Plugin     | claude-sonnet-4-6 | Vite plugin API, timing concerns             |
| 5b    | Rolldown Plugin | claude-sonnet-4-6 | May need to adapt to current API             |
| 6     | Integration     | claude-sonnet-4-6 | Needs to understand full system              |

## Critical Shared Context

Give EVERY agent this at the start of their prompt:

> "This is part of a parallel multi-agent implementation of oxc-react-docgen. Key rules that apply to ALL agents:
>
> 1. Use FxHashMap (rustc-hash) for internal maps, BTreeMap for JSON-facing output
> 2. Use CompactString (compact_str) for type/prop names, not String
> 3. Never unwrap() in library code — use ? and thiserror
> 4. No AST references escape parse_file — allocator is local per call
> 5. No terminal/display code in crates/core — zero dependency on indicatif/owo-colors
> 6. Tier 4 / Corsa / tsgo is out of scope — add TODO comments only
> 7. Always emit Diagnostic when degrading — never fail silently"
