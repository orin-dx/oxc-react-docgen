import { execSync } from 'node:child_process'
import { existsSync, mkdirSync, readdirSync, statSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import type { ToolResult, NormalizedOutput } from './types.ts'

const __dirname = dirname(fileURLToPath(import.meta.url))
const FIXTURES_ROOT = resolve(__dirname, '../../../fixtures')
// Prefer release build (faster, has all fixes); fall back to debug for dev convenience.
const CLI = existsSync(resolve(__dirname, '../../../target/release/oxc-react-docgen'))
  ? resolve(__dirname, '../../../target/release/oxc-react-docgen')
  : resolve(__dirname, '../../../target/debug/oxc-react-docgen')

// Find library directories under fixtures/
function discoverLibraries(): string[] {
  return readdirSync(FIXTURES_ROOT)
    .filter(d => statSync(resolve(FIXTURES_ROOT, d)).isDirectory())
    .sort()
}

// Map our PropType to a human-readable string for comparison
function propTypeToString(pt: any): string {
  if (!pt || typeof pt !== 'object') return 'unknown'
  switch (pt.kind) {
    case 'string': return 'string'
    case 'number': return 'number'
    case 'boolean': return 'boolean'
    case 'null': return 'null'
    case 'undefined': return 'undefined'
    case 'any': return 'any'
    case 'never': return 'never'
    case 'unknown': return 'unknown'
    case 'void': return 'void'
    case 'reactNode': return 'ReactNode'
    case 'cssProperties': return 'CSSProperties'
    case 'elementType': return 'ElementType'
    case 'sxProps': return 'SxProps'
    case 'stringLiteral': return JSON.stringify(pt.value)
    case 'numberLiteral': return String(pt.value)
    case 'boolLiteral': return String(pt.value)
    case 'union': return (pt.members as any[]).map(propTypeToString).join(' | ')
    case 'intersection': return (pt.members as any[]).map(propTypeToString).join(' & ')
    case 'array': return `${propTypeToString(pt.element)}[]`
    case 'tuple': return `[${(pt.elements as any[]).map(propTypeToString).join(', ')}]`
    case 'object': return '{ ... }'
    case 'named': return pt.args?.length ? `${pt.name}<${pt.args.map(propTypeToString).join(', ')}>` : pt.name
    case 'eventHandler': return `(${pt.paramName ?? 'e'}: ${pt.eventType}) => void`
    case 'ref': return pt.element ? `Ref<${pt.element}>` : 'Ref'
    case 'htmlAttributes': return `${pt.element}HTMLAttributes`
    case 'literalUnion': return pt.members.map((m: string) => JSON.stringify(m)).join(' | ')
    case 'opaque': return pt.raw
    default: return JSON.stringify(pt)
  }
}

function normalize(output: any): NormalizedOutput {
  const result: NormalizedOutput = {}
  for (const [_key, comp] of Object.entries(output.components ?? {})) {
    const c = comp as any
    result[c.displayName] = {
      displayName: c.displayName,
      description: c.description ?? '',
      props: Object.fromEntries(
        Object.entries(c.props ?? {}).map(([propName, prop]: [string, any]) => [
          propName,
          {
            name: propName,
            required: prop.required ?? false,
            type: propTypeToString(prop.type),
            description: prop.description ?? '',
            defaultValue: prop.defaultValue?.value,
          },
        ])
      ),
    }
  }
  return result
}

const libraries = discoverLibraries()
const results: ToolResult[] = []

for (const lib of libraries) {
  const libPath = resolve(FIXTURES_ROOT, lib)
  const start = performance.now()
  try {
    const raw = execSync(
      `${CLI} extract --src ${libPath} --format canonical`,
      { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }
    )
    // CLI prints a spinner line then JSON — find the JSON start
    const jsonStart = raw.indexOf('{')
    const json = jsonStart >= 0 ? raw.slice(jsonStart) : raw
    const parsed = JSON.parse(json)
    const durationMs = performance.now() - start

    for (const [_key, comp] of Object.entries(parsed.components ?? {})) {
      const c = comp as any
      const inheritedElements = (c.inheritance ?? [])
        .map((l: any) => l.htmlElement)
        .filter(Boolean)
      const notableInheritedNames = Object.keys(c.notableInherited ?? {})
      // Key by displayName (not file path) to avoid collision when multiple components share a file.
      const basename = c.filePath.split('/').pop()?.replace(/\.tsx?$/, '').replace(/\.d$/, '') ?? c.displayName
      results.push({
        tool: 'oxc-react-docgen',
        fixture: `${lib}/${basename}/${c.displayName}`,
        durationMs,
        output: normalize({ components: { [c.displayName]: c } }),
        inheritedElements,
        notableInheritedNames,
      })
    }
  } catch (e: any) {
    results.push({
      tool: 'oxc-react-docgen',
      fixture: lib,
      durationMs: performance.now() - start,
      output: {},
      error: e.stderr ?? e.message,
    })
  }
}

console.log(JSON.stringify(results, null, 2))
