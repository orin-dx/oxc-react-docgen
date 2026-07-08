# Migrating from react-docgen-typescript

`oxc-react-docgen` targets output-shape compatibility with `react-docgen-typescript` (RDT) via the CLI's `--format rdt` flag. The NAPI binding and Vite plugin currently only expose the tool's own canonical format (`kind`-tagged `PropType`, shown in the main README) — there's no RDT-shape option at that layer yet. This document covers what's compatible today, what's intentionally different, and what's a known gap rather than a design choice.

**Should you switch today?** If your components use plain interfaces/type aliases, `forwardRef`, `FC`, HOC wrapping, `Omit`/`Pick`, or `VariantProps` (CVA/TV/PandaCSS) — yes, output should match closely. If you scrape the full 250+ inherited HTML attributes per component, or rely on deeply nested discriminated unions, read the [HTML attribute inheritance](#html-attribute-inheritance) and [Known bugs](#known-bugs-not-design-choices) sections below first.

## Field-by-field comparison

### Component level (`ComponentDoc`)

| RDT field | `--format rdt` | Notes |
|---|---|---|
| `displayName` | ✅ | |
| `description` | ✅ | From the component's own leading JSDoc, if any |
| `props` | ✅ | See prop-level table below |
| `methods` | ✅ (always `[]`) | RDT only populates this for class components; we don't support class components, so it's always empty — present for shape compatibility, not because we detect methods |
| `tags` | ✅ | JSDoc `@tag` values on the component declaration (e.g. `@deprecated`, `@since`) — not part of RDT's original spec but included since some tooling reads it |

### Prop level (`PropItem`)

| RDT field | `--format rdt` | Notes |
|---|---|---|
| `name` | ✅ | |
| `required` | ✅ | |
| `type.name` | ✅ | Literal unions emit `{name: "enum", value: [...]}` matching RDT's convention (so Storybook-style `<select>` controls activate) instead of inlining the literal text |
| `description` | ✅ | |
| `defaultValue` | ✅ | `{value, computed}` — captured from destructured parameter defaults (`{ variant = 'primary' }`) or JSDoc `@default`, code value wins on conflict |
| `parent.name` / `parent.fileName` | ✅ | `fileName` is always canonicalized to an absolute path regardless of how `--src` was invoked |
| `declarations` | ❌ not emitted in `--format rdt` | Only present in `--format canonical` (the raw internal shape); omitted from both `--format rdt` and `--format storybook` to match RDT's single-parent shape. File an issue if you need it |

### Intentionally omitted

| Prop | Why |
|---|---|
| `ref` | Not a user-facing prop — a React internal. RDT includes it because it walks the full inherited type; we treat it as noise |
| `key` | Same reasoning as `ref` |

### HTML attribute inheritance

RDT inlines every inherited HTML attribute (e.g. all ~250-300 members of `ButtonHTMLAttributes`) directly into `props`. We don't — instead:

- `inheritance` (canonical format only) records the layer itself (`ButtonHTMLAttributes`, the element it maps to, what was `Omit`-ted).
- `notableInherited` surfaces ~15-20 curated, commonly-documented HTML attributes per element (`onClick`, `disabled`, `type`, ARIA attributes, etc.) rather than the full set.
- `--format rdt` does **not** include `notableInherited` at all — RDT consumers that filter by `parent.fileName.includes('node_modules')` to drop inherited noise will simply see fewer inherited props than RDT provides, not extra/wrong ones.

If your tooling depends on the full inherited-attribute list being present, this is the biggest real compatibility gap — not a bug, but a deliberate scope decision to keep output focused on what a component actually documents.

## Known bugs, not design choices

These are tracked in [docs/rdt-coverage.md](docs/rdt-coverage.md) and get fixed over time — check that file for current status before assuming a mismatch is permanent:

- Discriminated unions (e.g. `AccordionSingleProps | AccordionMultipleProps`) union each conflicting prop's type across all members that declare it — matches RDT's behavior for the fixtures tested so far, but hasn't been exercised against every union shape RDT handles.
- `Pick<T, K>` / `Omit<T, K>` resolution depends on `K` being a literal string-union at the reference site; a `K` that's itself an aliased type may not resolve.

## Getting the exact diff for your codebase

`apps/validate/` in this repo is the harness used to compare this tool's output against a real `react-docgen-typescript` run, field by field, across several real component libraries (shadcn, MUI, Chakra, Mantine, React Aria, Radix). Run `pnpm --filter @oxc-react-docgen/validate compare:all` against your own components if you need a concrete list of diffs before switching, rather than trusting this document alone.
