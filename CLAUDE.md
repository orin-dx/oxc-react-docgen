# oxc-react-docgen

10-100x faster drop-in replacement for react-docgen-typescript. OXC parses TypeScript/TSX; we extract React component props and emit RDT-compatible JSON. See `docs/STATUS.md` for current state and known gaps.

## Commands

```bash
cargo test -p oxc-react-docgen-core          # unit + snapshot tests
cargo clippy -p oxc-react-docgen-core -- -D warnings
cargo build --release                         # full build including CLI
```

Or use `/check` and `/snapshot` for the common workflows.

## Non-negotiables

1. No `unwrap()` outside `#[cfg(test)]` — use `?`
2. `FxHashMap` for internal maps; `BTreeMap` for JSON-output maps
3. `CompactString` for type/prop names in hot paths
4. No AST refs escape `parse_file()` — allocator is local per call
5. No terminal/display code in `crates/core`
6. Always emit `Diagnostic` when degrading — never fail silently

## Architecture

```
OXC parse → SourceData (extractor/) → GlobalSourceData (pipeline/) → ComponentEntry (resolver/) → ExtractionOutput
```

No reverse dependencies. `pub(crate)` for implementation modules; `pub` only for consumer-facing API (`pipeline/`, `types/`, `react_types`).

## Testing discipline

Two habits that repeatedly caught real bugs during the 2026-08-12 spec-validation pass (a stale-by-one-edit watch mode, always-zero watch stats — both had zero test coverage despite extensive existing suites):

1. **Sibling-path parity.** When a function parallels an already-tested one (e.g. `WatchSession::update_file` mirrors `extract()`'s resolve/panic-containment logic; `create_session`'s guard boundary mirrors `initialize_session`/`extract_file_incremental`'s), the new path needs a test asserting the *same* contract, not just "it runs without error." A sibling path is exactly where coverage silently lapses.
2. **Assert content, not presence.** Prefer `assert_eq!` against an exact expected value over `contains(...)`/`is_some()`/liveness checks wherever the cost is comparable. A test asserting "component X exists" survives X's props resolving to stale or wrong data; a test asserting X's exact prop set does not.

## Crate guide

- `crates/core` — pure extraction logic; @crates/core/CLAUDE.md
- `crates/cli` — clap CLI, no logic; @crates/cli/CLAUDE.md
- `crates/binding` — thin NAPI wrapper, delegates to core
