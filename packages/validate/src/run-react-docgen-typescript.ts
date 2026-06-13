import * as path from 'node:path'
import { withCustomConfig } from 'react-docgen-typescript'
import { discoverFixtures, FIXTURES_ROOT } from './fixtures.ts'
import type { ToolResult, NormalizedOutput } from './types.ts'

const tsconfigPath = path.resolve(FIXTURES_ROOT, '../tsconfig.json')

// Create parser — falls back gracefully if tsconfig not found
function makeParser() {
  try {
    return withCustomConfig(tsconfigPath, {
      shouldExtractLiteralValuesFromEnum: true,
      shouldRemoveUndefinedFromOptional: true,
      propFilter: { skipPropsWithoutDoc: false },
    })
  } catch {
    const { withDefaultConfig } = require('react-docgen-typescript')
    return withDefaultConfig({ shouldExtractLiteralValuesFromEnum: true })
  }
}

function normalize(rdtOutput: any[]): NormalizedOutput {
  const result: NormalizedOutput = {}
  for (const comp of rdtOutput) {
    result[comp.displayName] = {
      displayName: comp.displayName,
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
        ])
      ),
    }
  }
  return result
}

const parser = makeParser()
const fixtures = discoverFixtures().filter(f => !f.isDts) // RDT needs real TS files
const results: ToolResult[] = []

for (const fixture of fixtures) {
  const start = performance.now()
  try {
    const raw = parser.parse(fixture.path)
    results.push({
      tool: 'react-docgen-typescript',
      fixture: fixture.name,
      durationMs: performance.now() - start,
      output: normalize(raw),
    })
  } catch (e: any) {
    results.push({
      tool: 'react-docgen-typescript',
      fixture: fixture.name,
      durationMs: performance.now() - start,
      output: {},
      error: e.message,
    })
  }
}

console.log(JSON.stringify(results, null, 2))
