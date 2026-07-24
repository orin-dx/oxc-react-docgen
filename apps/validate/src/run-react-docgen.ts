import { parse } from 'react-docgen'
import { readFileSync } from 'node:fs'
import { discoverFixtures } from './fixtures.ts'
import type { ToolResult, NormalizedOutput } from './types.ts'

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
            type: prop.type?.name ?? 'unknown',
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
    results.push({
      tool: 'react-docgen',
      fixture: fixture.name,
      durationMs,
      output: normalize(Array.isArray(raw) ? raw : [raw]),
    })
  } catch (e: any) {
    results.push({
      tool: 'react-docgen',
      fixture: fixture.name,
      durationMs: performance.now() - start,
      output: {},
      error: e.message,
    })
  }
}

console.log(JSON.stringify(results, null, 2))
