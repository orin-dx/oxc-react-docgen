import * as path from 'node:path'
import { withCustomConfig } from 'react-docgen-typescript'
import { discoverFixtures, FIXTURES_ROOT } from './fixtures.ts'
import type { ToolResult } from './types.ts'

const tsconfigPath = path.resolve(FIXTURES_ROOT, '..', 'tsconfig.json')

// Create parser — falls back gracefully if tsconfig not found
function makeParser() {
  try {
    return withCustomConfig(tsconfigPath, {
      shouldExtractLiteralValuesFromEnum: true,
      shouldRemoveUndefinedFromOptional: true,
      propFilter: { skipPropsWithoutDoc: false },
    })
  } catch {
    return withCustomConfig(tsconfigPath, { shouldExtractLiteralValuesFromEnum: true })
  }
}

const parser = makeParser()
const fixtures = discoverFixtures().filter((f) => !f.isDts) // RDT needs real TS files
const results: ToolResult[] = []

for (const fixture of fixtures) {
  const start = performance.now()
  try {
    const raw = parser.parse(fixture.path)
    const durationMs = performance.now() - start
    // One entry per component so keys match our tool's ${lib}/${basename}/${displayName} format.
    for (const comp of raw) {
      results.push({
        tool: 'react-docgen-typescript',
        fixture: `${fixture.name}/${comp.displayName}`,
        durationMs,
        output: {
          [comp.displayName]: {
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
              ]),
            ),
          },
        },
      })
    }
    if (raw.length === 0) {
      results.push({
        tool: 'react-docgen-typescript',
        fixture: fixture.name,
        durationMs,
        output: {},
        error: 'no components found',
      })
    }
  } catch (error: any) {
    results.push({
      tool: 'react-docgen-typescript',
      fixture: fixture.name,
      durationMs: performance.now() - start,
      output: {},
      error: error.message,
    })
  }
}

console.log(JSON.stringify(results, null, 2))
