# Migrating from react-docgen-typescript

`oxc-react-docgen` targets output-shape compatibility with `react-docgen-typescript` (RDT) via the CLI's `--format rdt` flag. The NAPI binding and Vite plugin currently only expose the tool's own canonical format (`kind`-tagged `PropType`, shown in the main README) — there's no RDT-shape option at that layer yet. This document covers what's compatible today, what's intentionally different, and what's a known gap rather than a design choice.

**Should you switch today?** If your components use plain interfaces/type aliases, `forwardRef` (including wrapped in an `as` cast), `FC`, HOC wrapping, `Omit`/`Pick`, `VariantProps` (CVA/TV/PandaCSS), or discriminated unions (including ones wrapped in an intersection) — yes, output should match closely. If you scrape the full 250+ inherited HTML attributes per component, read the [HTML attribute inheritance](#html-attribute-inheritance) section below — it's supported, but opt-in.

## Field-by-field comparison

### Component level (`ComponentDoc`)

| RDT field | `--format rdt` | Notes |
| --- | --- | --- |
| `displayName` | ✅ |  |
| `description` | ✅ | From the component's own leading JSDoc, if any |
| `props` | ✅ | See prop-level table below |
| `methods` | ✅ (always `[]`) | RDT only populates this for class components; we don't support class components, so it's always empty — present for shape compatibility, not because we detect methods |
| `tags` | ✅ | JSDoc `@tag` values on the component declaration (e.g. `@deprecated`, `@since`) — not part of RDT's original spec but included since some tooling reads it |

### Prop level (`PropItem`)

| RDT field | `--format rdt` | Notes |
| --- | --- | --- |
| `name` | ✅ |  |
| `required` | ✅ |  |
| `type.name` | ✅ | Literal unions emit `{name: "enum", value: [...]}` matching RDT's convention (so Storybook-style `<select>` controls activate) instead of inlining the literal text |
| `description` | ✅ |  |
| `defaultValue` | ✅ | `{value, computed}` — captured from destructured parameter defaults (`{ variant = 'primary' }`) or JSDoc `@default`, code value wins on conflict |
| `parent.name` / `parent.fileName` | ✅ | `fileName` is always canonicalized to an absolute path regardless of how `--src` was invoked |
| `declarations` | ❌ not emitted in `--format rdt` | Only present in `--format canonical` (the raw internal shape); omitted from both `--format rdt` and `--format storybook` to match RDT's single-parent shape. File an issue if you need it |

### Intentionally omitted

| Prop | Why |
| --- | --- |
| `ref` | Not a user-facing prop — a React internal. RDT includes it because it walks the full inherited type; we treat it as noise |
| `key` | Same reasoning as `ref` |

### HTML attribute inheritance

RDT inlines every inherited HTML attribute (e.g. all ~250-300 members of `ButtonHTMLAttributes`) directly into `props`. By default, we don't — but you can turn this on:

- **Default (curated):** `inheritance` (canonical format only) records the layer itself (`ButtonHTMLAttributes`, the element it maps to, what was `Omit`-ted), and `notableInherited` surfaces ~15-20 curated, commonly-documented HTML attributes per element (`onClick`, `disabled`, `type`, ARIA attributes, etc.) rather than the full set. `--format rdt` does **not** include `notableInherited` at all in this mode — RDT consumers that filter by `parent.fileName.includes('node_modules')` to drop inherited noise will simply see fewer inherited props than RDT provides, not extra/wrong ones.
- **Opt-in full expansion:** pass `--html-attributes full` (CLI), `htmlAttributes: 'full'` (NAPI), or `htmlAttributes: "full"` (`docgen.config.ts`) to actually resolve `@types/react`'s real `HTMLAttributes`/`AriaAttributes`/`DOMAttributes`/`<Element>HTMLAttributes` interface chain and merge the real fields directly into `props` — this _does_ reach `--format rdt`, matching RDT's flat behavior. Verified against a real button: 238 of ~250 real attributes resolve; the remainder is a narrow, separate gap (a handful of fields inside `@types/react`'s own interface chain reference a same-namespace sibling type without an explicit qualifier — degrades gracefully to an `UNRESOLVABLE_IMPORT` diagnostic on that one field, not a crash or missing component). See `docs/rdt-coverage.md`'s "HtmlAttributeMode" section for details.
- **`--html-attributes none`:** no inherited HTML attributes synthesized at all, own props only.

If you need the full attribute set and aren't ready to opt into `full` mode, this remains the main compatibility gap to check for your specific components.

## Getting the exact diff for your codebase

`apps/validate/` in this repo is the harness used to compare this tool's output against a real `react-docgen-typescript` run, field by field, across real component libraries — currently 15: shadcn, MUI, Chakra, Mantine, React Aria, Radix, PandaCSS, Fluent UI, Base UI, Ant Design, Ark UI, Zendesk Garden, React Day Picker, Headless UI, and Blueprint. Run `pnpm --filter @oxc-react-docgen/validate compare:all` against your own components if you need a concrete list of diffs before switching, rather than trusting this document alone. `docs/rdt-coverage.md` tracks which specific patterns each fixture exercises and the bugs found (almost all now fixed) along the way.
