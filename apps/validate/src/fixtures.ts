import { resolve } from 'node:path'
import { readdirSync, statSync } from 'node:fs'

export const FIXTURES_ROOT = resolve(import.meta.dirname, '../../../fixtures')

export interface Fixture {
  name: string // e.g. "shadcn/button"
  path: string // absolute path
  ext: string // ".tsx" | ".ts" | ".d.ts"
  isTsx: boolean
  isDts: boolean
}

export function discoverFixtures(): Fixture[] {
  const fixtures: Fixture[] = []
  for (const dir of readdirSync(FIXTURES_ROOT)) {
    const dirPath = resolve(FIXTURES_ROOT, dir)
    if (!statSync(dirPath).isDirectory()) continue
    if (dir === 'panda') continue // skip styled-system subdir complexity for now
    for (const file of readdirSync(dirPath)) {
      if (!file.endsWith('.ts') && !file.endsWith('.tsx')) continue
      if (file === 'index.ts') continue // skip barrel files for now
      const ext = file.endsWith('.d.ts') ? '.d.ts' : file.endsWith('.tsx') ? '.tsx' : '.ts'
      fixtures.push({
        name: `${dir}/${file.replace(/\.tsx?$/, '').replace(/\.d$/, '')}`,
        path: resolve(dirPath, file),
        ext,
        isTsx: file.endsWith('.tsx'),
        isDts: file.endsWith('.d.ts'),
      })
    }
  }
  return fixtures.sort((a, b) => a.name.localeCompare(b.name))
}
