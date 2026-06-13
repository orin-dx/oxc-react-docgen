import { execSync } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'

mkdirSync('./baselines', { recursive: true })

console.log('Running react-docgen baseline...')
const rdg = execSync('pnpm run:rdg', { encoding: 'utf8' })
writeFileSync('./baselines/react-docgen.json', rdg)

console.log('Running react-docgen-typescript baseline...')
const rdt = execSync('pnpm run:rdt', { encoding: 'utf8' })
writeFileSync('./baselines/react-docgen-typescript.json', rdt)

console.log('✅ Baselines saved to ./baselines/')
