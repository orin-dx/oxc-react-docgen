import * as path from 'node:path'
import { withCustomConfig } from 'react-docgen-typescript'
import { discoverFixtures, FIXTURES_ROOT } from './fixtures.ts'
import type { ToolResult } from './types.ts'

const tsconfigPath = path.resolve(FIXTURES_ROOT, '..', 'tsconfig.json')

// Falls back to a minimal option set if the extended options throw for this tsconfig.
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

// react-docgen-typescript represents union/literal prop types as
// `{ name: 'enum', value: [{ value: '"foo"' }, ...] }` (with
// shouldExtractLiteralValuesFromEnum) instead of putting the real type string
// in `.name` — reading `.name` alone for these props compares the literal
// string "enum" against our real union string, which isn't a type
// difference, it's reading the wrong field.
function rdtTypeToString(type: { name?: string; value?: Array<{ value: string }> } | undefined): string {
  if (!type) return 'unknown'
  if (type.name === 'enum' && Array.isArray(type.value)) {
    return type.value.map((v) => v.value).join(' | ')
  }
  return type.name ?? 'unknown'
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
                  type: rdtTypeToString(prop.type),
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
