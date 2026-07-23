# Status

**Updated:** 2026-07-22

Core extraction, resolver, CLI, NAPI binding, and Vite plugin all work and are
tested. Config file loading, cross-package `.d.ts` resolution, and watch mode
are fully implemented — not stubs, despite what older docs in this repo used
to say. If you find another doc contradicting this file, this file wins.

## Numbers

- 191 Rust tests (21 cli + 158 core unit + 8 snapshot + 4 napi binding), 18
  vitest — all green
- `cargo clippy --workspace --all-targets -D warnings` clean
- 20 real-world fixture libraries validated against `react-docgen-typescript`
  (shadcn, Radix, MUI, Chakra, Mantine, React Aria, antd, ariakit, ark-ui,
  base-ui, blueprint, day-picker, fluentui, headlessui, panda,
  react-final-form, react-resizable-panels, storybook-emotion,
  tanstack-table, zendesk-garden — see `rdt-coverage.md`)

## What's not built yet

- **Preset system** (`@oxc-react-docgen/presets`) — named `PipelineOptions`
  bundles. Config-side only, no Rust changes needed.
- **LSP server** — `lsp-types` is a dependency; nothing built on it.
- **Barrel/re-export scoped-key allocation caching** — `resolver/chain.rs` /
  `named.rs` / `template.rs` build a `"{file}:{name}"` scoped-key string on
  every lookup. A `Borrow`-based type-map key would let lookups happen
  without allocating. Real but unbenchmarked — do as a focused perf pass if
  profiling shows it matters.
- **DTS cache has no dirty-flag or size cap** (`cache.rs`) — rewrites the
  whole cache file on every run regardless of whether anything changed, and
  has no upper bound on how large the on-disk cache can grow. Low severity
  (requires local write access, and an attacker with that already has better
  options) — worth fixing for large monorepos before it becomes a real cost.

## Known gaps that won't get fixed without a type checker

See the "Known gaps summary" table in `rdt-coverage.md` for the full,
maintained list. The short version: conditional types, mapped types over an
unbound generic, and a couple of `@emotion/styled`-specific call shapes all
need real type inference OXC deliberately doesn't do. Not bugs, not on the
roadmap unless typescript-go's Corsa API stabilizes (see
`type-checker-integration.md`).

## Where to look next

- **What broke and why, historically** → `rdt-coverage.md` — every bug found
  during real-library validation, root cause, and the fix. Keep this updated;
  it's the project's memory.
- **Why a hard-to-reverse decision was made** → `docs/adr/` — OXC over the
  TypeScript compiler, manual serde for `PropType`, positional msgpack
  encoding, deferring type-checker integration. Write a new one when you make
  a decision like these (see `docs/adr/README.md`).
- **How the pipeline fits together** → `ARCHITECTURE.md`
- **Setup, testing, code style, commit conventions** → `CONTRIBUTING.md`
- **Migrating from react-docgen-typescript** → `MIGRATING.md`
- **Old build-out specs, resolved questions, point-in-time analyses** →
  `docs/archive/` — historical, not current, kept for reference
