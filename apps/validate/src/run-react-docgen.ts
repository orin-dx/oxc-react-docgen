import { parse } from 'react-docgen'
import { readFileSync } from 'node:fs'
import { discoverFixtures } from './fixtures.ts'
import type { ToolResult, NormalizedOutput } from './types.ts'

// react-docgen puts real TypeScript type info under `tsType`, not `type` —
// `type` is the legacy PropTypes-oriented field (`PropTypes.string` etc.) and
// is simply absent for interface/type-annotated TS props, so reading it alone
// silently produced "unknown" for every TS-typed prop. `tsType.raw` carries
// the exact original source text (`'primary' | 'secondary'`, `string[]`,
// `(id: string) => void`) when TS can print it, which is the fairest
// comparison against our own type-to-string rendering — falls back to
// `tsType.name` for the cases where `raw` isn't present (plain primitives).
function rdgTypeToString(prop: any): string {
  const t = prop.tsType ?? prop.type
  if (!t) return 'unknown'
  return t.raw ?? t.name ?? 'unknown'
}

function normalize(rdgOutput: any[]): NormalizedOutput {
  const result: NormalizedOutput = {}
  for (const comp of rdgOutput) {
    const name = comp.displayName ?? 'Unknown'
    result[name] = {
      displayName: name,
      description: comp.description ?? '',
      props: Object.fromEntries(
        Object.entries(comp.props ?? {}).map(([propName, prop]: [string, any]) => [
          propName,
          {
            name: propName,
            required: prop.required ?? false,
            type: rdgTypeToString(prop),
            description: prop.description ?? '',
            defaultValue: prop.defaultValue?.value,
          },
        ]),
      ),
    }
  }
  return result
}

const fixtures = discoverFixtures().filter((f) => f.isTsx)
const results: ToolResult[] = []

for (const fixture of fixtures) {
  const source = readFileSync(fixture.path, 'utf8')
  const start = performance.now()
  try {
    const raw = parse(source, { filename: fixture.path })
    const durationMs = performance.now() - start
    const comps = Array.isArray(raw) ? raw : [raw]
    // One entry per component, keyed `${fixture.name}/${displayName}` — matches
    // rdt's and ours' keying. Previously this pushed one ToolResult per FILE
    // (keyed by fixture.name alone, with every component packed into its
    // `output`), which never joined against rdt/ours' per-component keys in
    // compare.ts — a multi-component file silently broke the join for every
    // component in it, and even single-component files only "matched" by
    // accident of an exact string comparison that never actually held.
    for (const comp of comps) {
      const name = comp.displayName ?? 'Unknown'
      results.push({
        tool: 'react-docgen',
        fixture: `${fixture.name}/${name}`,
        durationMs,
        output: normalize([comp]),
      })
    }
    if (comps.length === 0) {
      results.push({ tool: 'react-docgen', fixture: fixture.name, durationMs, output: {} })
    }
  } catch (error: any) {
    results.push({
      tool: 'react-docgen',
      fixture: fixture.name,
      durationMs: performance.now() - start,
      output: {},
      error: error.message,
    })
  }
}

console.log(JSON.stringify(results, null, 2))
