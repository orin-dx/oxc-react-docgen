import { spawnSync } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

mkdirSync('./baselines', { recursive: true })

function runScript(script: string): string {
  const result = spawnSync('tsx', [resolve(import.meta.dirname, script)], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'], // stdout captured, stderr forwarded
  })
  if (result.status !== 0) throw new Error(`${script} failed`)
  return result.stdout
}

console.log('Running react-docgen baseline...')
writeFileSync('./baselines/react-docgen.json', runScript('run-react-docgen.ts'))
console.log('✅ react-docgen baseline saved')

console.log('Running react-docgen-typescript baseline...')
writeFileSync('./baselines/react-docgen-typescript.json', runScript('run-react-docgen-typescript.ts'))
console.log('✅ react-docgen-typescript baseline saved')

console.log('\nBaselines saved to ./baselines/')
