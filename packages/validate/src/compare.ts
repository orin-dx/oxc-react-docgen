import { readFileSync, existsSync } from 'node:fs'
import equal from 'fast-deep-equal'
import type { ToolResult, ComparisonResult } from './types.ts'

function loadResults(tool: string): ToolResult[] {
  const p = `./baselines/${tool}.json`
  if (!existsSync(p)) {
    console.error(`No baseline for ${tool} — run pnpm run:${tool === 'react-docgen' ? 'rdg' : 'rdt'} first`)
    process.exit(1)
  }
  return JSON.parse(readFileSync(p, 'utf8'))
}

const rdgResults = Object.fromEntries(loadResults('react-docgen').map(r => [r.fixture, r]))
const rdtResults = Object.fromEntries(loadResults('react-docgen-typescript').map(r => [r.fixture, r]))

const allFixtures = new Set([...Object.keys(rdgResults), ...Object.keys(rdtResults)])

for (const fixture of [...allFixtures].sort()) {
  const rdg = rdgResults[fixture]
  const rdt = rdtResults[fixture]

  console.log(`\n## ${fixture}`)

  if (!rdg) { console.log('  ⚠️  react-docgen: no result'); continue }
  if (!rdt) { console.log('  ⚠️  react-docgen-typescript: no result'); continue }
  if (rdg.error) console.log(`  ❌ rdg error: ${rdg.error}`)
  if (rdt.error) console.log(`  ❌ rdt error: ${rdt.error}`)

  console.log(`  ⏱  rdg: ${rdg.durationMs.toFixed(1)}ms  |  rdt: ${rdt.durationMs.toFixed(1)}ms`)

  const rdgComps = Object.keys(rdg.output)
  const rdtComps = Object.keys(rdt.output)
  const rdtOnlyComps = rdtComps.filter(c => !rdgComps.includes(c))
  const rdgOnlyComps = rdgComps.filter(c => !rdtComps.includes(c))
  if (rdtOnlyComps.length) console.log(`  rdt-only components: ${rdtOnlyComps.join(', ')}`)
  if (rdgOnlyComps.length) console.log(`  rdg-only components: ${rdgOnlyComps.join(', ')}`)

  for (const comp of rdtComps.filter(c => rdgComps.includes(c))) {
    const rdgProps = Object.keys(rdg.output[comp]?.props ?? {})
    const rdtProps = Object.keys(rdt.output[comp]?.props ?? {})
    const rdtOnly = rdtProps.filter(p => !rdgProps.includes(p))
    const rdgOnly = rdgProps.filter(p => !rdtProps.includes(p))
    const typeDiffs = rdtProps
      .filter(p => rdgProps.includes(p))
      .filter(p => rdt.output[comp].props[p].type !== rdg.output[comp].props[p].type)
      .map(p => ({ prop: p, rdt: rdt.output[comp].props[p].type, rdg: rdg.output[comp].props[p].type }))

    if (rdtOnly.length) console.log(`  ${comp}: rdt-only props (${rdtOnly.length}): ${rdtOnly.slice(0,5).join(', ')}${rdtOnly.length > 5 ? '...' : ''}`)
    if (rdgOnly.length) console.log(`  ${comp}: rdg-only props (${rdgOnly.length}): ${rdgOnly.slice(0,5).join(', ')}${rdgOnly.length > 5 ? '...' : ''}`)
    if (typeDiffs.length) console.log(`  ${comp}: type diffs (${typeDiffs.length}): ${typeDiffs.slice(0,3).map(d => `${d.prop}(rdt:"${d.rdt}" rdg:"${d.rdg}")`).join(', ')}`)
    if (!rdtOnly.length && !rdgOnly.length && !typeDiffs.length) console.log(`  ${comp}: ✅ identical`)
  }
}
