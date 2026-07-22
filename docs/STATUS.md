# Status

**Updated:** 2026-07-21

Core extraction, resolver, CLI, NAPI binding, and Vite plugin all work and are
tested. Config file loading, cross-package `.d.ts` resolution, and watch mode
are fully implemented — not stubs, despite what older docs in this repo used
to say. If you find another doc contradicting this file, this file wins.

## Numbers

- 159 Rust tests (16 cli + 135 core unit + 8 snapshot), 14 vitest — all green
- `cargo clippy --workspace --all-targets -D warnings` clean
- 15 real-world fixture libraries validated against `react-docgen-typescript`
  (shadcn, Radix, MUI, Chakra, Mantine, React Aria, antd, ark-ui, base-ui,
  blueprint, day-picker, fluentui, headlessui, panda, react-resizable-panels,
  storybook-emotion, tanstack-table, zendesk-garden — see `rdt-coverage.md`)

## What's not built yet

- **Preset system** (`@oxc-react-docgen/presets`) — named `PipelineOptions`
  bundles. Config-side only, no Rust changes needed.
- **LSP server** — `lsp-types` is a dependency; nothing built on it.
- **React 19 `ref`-as-prop** — `ReactVersion::ref_as_prop` exists as a field
  but nothing reads it yet. Components using React 19's `function Button({
  ref, ...props })` pattern (no `forwardRef`) won't pick up `ref`'s type.
- **Compound components** — `Dialog.Trigger`, `Select.Item`. Not detected as
  separate components at all right now.
- **Static `defaultProps`** — `Button.defaultProps = { size: 'md' }` isn't
  read. Destructured defaults (`function Button({ size = 'md' })`) are.

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
- **How the pipeline fits together** → `ARCHITECTURE.md`
- **Setup, testing, code style, commit conventions** → `CONTRIBUTING.md`
- **Migrating from react-docgen-typescript** → `MIGRATING.md`
- **Old build-out specs, resolved questions, point-in-time analyses** →
  `docs/archive/` — historical, not current, kept for reference
