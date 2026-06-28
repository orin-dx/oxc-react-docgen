# oxc-react-docgen

React component prop extraction powered by [OXC](https://oxc.rs). Parses TypeScript natively in Rust — no TypeScript compiler program, no type-checking pass, no startup tax.

> **Status:** Core extraction, NAPI bindings, Vite plugin, and CLI are complete. Config file loading is the next milestone.

## Why

`react-docgen-typescript` spins up a full TypeScript `Program` to extract props. On a mid-size design system this takes seconds — enough to noticeably delay Storybook startup and make HMR feel sluggish. OXC parses each file in parallel with no type-checking pass, so cold extraction on a 15-component fixture set (shadcn, MUI, Chakra, Mantine, React Aria, Radix) takes **32ms**.

The output shape is RDT-compatible, so existing Storybook configurations work without changes.

## Install

```bash
npm install -D @oxc-react-docgen/vite-plugin @oxc-react-docgen/napi
```

## Vite plugin

```ts
// vite.config.ts
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { oxcReactDocgen } from '@oxc-react-docgen/vite-plugin'

export default defineConfig({
  plugins: [
    react(),
    oxcReactDocgen({ srcDirs: ['src'] }),
  ],
})
```

Metadata is available on the `virtual:oxc-react-docgen` module and pushed via HMR as components change:

```ts
import docgen from 'virtual:oxc-react-docgen'

docgen.components['Button'].props
// {
//   variant: {
//     type: { kind: 'literalUnion', members: ['default', 'destructive', 'outline'], hasDefault: true },
//     required: false,
//     defaultValue: { value: '"default"', computed: false },
//     description: '',
//   },
//   ...
// }
```

## CLI

```bash
# One-shot extraction
oxc-react-docgen extract --src src/ --out docgen.json

# Inspect a single file
oxc-react-docgen inspect src/components/Button.tsx

# Watch mode
oxc-react-docgen watch --src src/

# Shell completions
oxc-react-docgen completions zsh > ~/.zsh/completions/_oxc-react-docgen
```

## Output shape

```json
{
  "components": {
    "Button": {
      "displayName": "Button",
      "filePath": "src/components/Button.tsx",
      "props": {
        "variant": {
          "type": { "kind": "literalUnion", "members": ["default", "destructive", "outline"], "hasDefault": true },
          "required": false,
          "defaultValue": { "value": "\"default\"", "computed": false },
          "description": "Visual style variant.",
          "tags": {}
        }
      },
      "inheritance": [
        { "typeName": "ButtonHTMLAttributes", "htmlElement": "button", "omitted": [], "totalProps": 147 }
      ],
      "notableInherited": {
        "onClick": { "type": { "kind": "eventHandler", "eventType": "MouseEvent<HTMLButtonElement>" }, ... },
        "disabled": { "type": { "kind": "boolean" }, ... }
      },
      "discriminantProp": null
    }
  },
  "diagnostics": [],
  "stats": { "filesParsed": 3, "durationMs": 12, "dtsCacheHits": 2, ... }
}
```

## Supported patterns

| Pattern | Example |
|---------|---------|
| Function components | `function Button(props: ButtonProps)` |
| Arrow components | `const Button = (props: ButtonProps) => ...` |
| `React.forwardRef` | `forwardRef<HTMLButtonElement, ButtonProps>(...)` |
| HOC-wrapped | `styled(Base)<ButtonProps>` |
| Interface inheritance | `interface ButtonProps extends HTMLAttributes<HTMLButtonElement>` |
| Intersection aliases | `type ButtonProps = BaseProps & { variant?: string }` |
| `Omit` / `Pick` | `type ButtonProps = Omit<InputProps, 'value'>` |
| `ComponentPropsWithoutRef` | `ComponentPropsWithoutRef<'button'>` |
| `VariantProps` | `VariantProps<typeof buttonVariants>` (CVA / TV / PandaCSS) |
| Discriminated unions | MUI-style `variant: 'filled' \| 'outlined' \| 'standard'` |
| JSDoc tags | `@description`, `@default`, `@deprecated`, `@internal` |

## Repository

```
crates/core/          pure extraction logic (no I/O, no async, Send + Sync)
crates/binding/       thin NAPI wrapper over core
crates/cli/           clap + miette CLI
packages/napi/        @oxc-react-docgen/napi (dev binary loader + TS types)
packages/vite-plugin/ @oxc-react-docgen/vite-plugin
apps/validate/        accuracy comparison harness (react-docgen, rdt, ours)
fixtures/             real library .d.ts/.tsx files (shadcn, MUI, Chakra, Mantine, etc.)
```

## Contributing

[CONTRIBUTING.md](CONTRIBUTING.md) — setup, code style, commit conventions.  
[ARCHITECTURE.md](ARCHITECTURE.md) — pipeline, modules, key design decisions.

## License

MIT
