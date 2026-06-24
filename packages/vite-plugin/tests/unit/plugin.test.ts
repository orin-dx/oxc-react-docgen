import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock BEFORE importing plugin — vi.mock is hoisted by Vitest
vi.mock('@oxc-react-docgen/napi', () => ({
  createSession: vi.fn().mockReturnValue(42),
  initializeSession: vi.fn().mockResolvedValue(
    JSON.stringify({ components: {}, enums: {}, diagnostics: [], stats: {} })
  ),
  extractFileIncremental: vi.fn().mockResolvedValue(
    JSON.stringify({
      updatedComponents: [{ displayName: 'Button', filePath: '/project/src/Button.tsx', props: {}, inheritance: [], notableInherited: {}, description: '', discriminantProp: null, composes: [], tags: {}, methods: [] }],
      affectedFiles: ['/project/src/Button.tsx'],
      diagnostics: [],
      durationMs: 5,
    })
  ),
  closeSession: vi.fn(),
}))

import { oxcReactDocgen } from '../../src/index.js'
import * as napi from '@oxc-react-docgen/napi'

describe('oxcReactDocgen', () => {
  let plugin: ReturnType<typeof oxcReactDocgen>

  beforeEach(() => {
    vi.clearAllMocks()
    plugin = oxcReactDocgen({ srcDirs: ['src'] })
    // Simulate Vite calling configResolved
    ;(plugin.configResolved as Function)({ root: '/project' })
  })

  // ── resolveId ──────────────────────────────────────────────────────────────

  describe('resolveId', () => {
    it('resolves the virtual module id', () => {
      const result = (plugin.resolveId as Function)('virtual:oxc-react-docgen')
      expect(result).toBe('\0virtual:oxc-react-docgen')
    })

    it('returns undefined for unrelated ids', () => {
      expect((plugin.resolveId as Function)('react')).toBeUndefined()
      expect((plugin.resolveId as Function)('./Button.tsx')).toBeUndefined()
    })
  })

  // ── load ───────────────────────────────────────────────────────────────────

  describe('load', () => {
    it('returns a JS module for the resolved virtual id', () => {
      const code = (plugin.load as Function)('\0virtual:oxc-react-docgen') as string
      expect(code).toMatch(/^export default \{/)
      const data = JSON.parse(code.replace('export default ', ''))
      expect(data).toHaveProperty('components')
    })

    it('returns undefined for other ids', () => {
      expect((plugin.load as Function)('\0other')).toBeUndefined()
      expect((plugin.load as Function)('virtual:oxc-react-docgen')).toBeUndefined()
    })
  })

  // ── configResolved ─────────────────────────────────────────────────────────

  describe('configResolved', () => {
    it('creates a NAPI session with resolved absolute src dirs', () => {
      expect(napi.createSession).toHaveBeenCalledWith(
        expect.objectContaining({ srcDirs: ['/project/src'] })
      )
    })
  })

  // ── configureServer ────────────────────────────────────────────────────────

  describe('configureServer', () => {
    it('calls initializeSession with the session id and resolved options', async () => {
      const mockServer = {
        environments: { client: { hot: { send: vi.fn() } } },
      } as any
      // configureServer returns a teardown fn; initializeSession is async internally.
      // Flush the microtask queue so the .then() callback runs before asserting.
      ;(plugin.configureServer as Function)(mockServer)
      await vi.waitUntil(() => (napi.initializeSession as any).mock.calls.length > 0)
      await (napi.initializeSession as any).mock.results[0].value
      expect(napi.initializeSession).toHaveBeenCalledWith(
        42,
        expect.objectContaining({ srcDirs: ['/project/src'] })
      )
    })
  })

  // ── hotUpdate ──────────────────────────────────────────────────────────────

  describe('hotUpdate', () => {
    const clientEnv = () => ({
      environment: { name: 'client', hot: { send: vi.fn() } },
    })

    const makeOpts = (file: string) => ({
      file,
      type: 'update' as const,
      modules: [],
      timestamp: 1000,
      read: vi.fn().mockResolvedValue(''),
      server: {} as any,
    })

    it('calls extractFileIncremental for tsx files in srcDirs', async () => {
      const ctx = clientEnv()
      await (plugin.hotUpdate as Function).call(ctx, makeOpts('/project/src/Button.tsx'))
      expect(napi.extractFileIncremental).toHaveBeenCalledWith(
        '/project/src/Button.tsx',
        42,
        expect.objectContaining({ srcDirs: ['/project/src'] })
      )
    })

    it('sends oxc-react-docgen:update over HMR', async () => {
      const ctx = clientEnv()
      await (plugin.hotUpdate as Function).call(ctx, makeOpts('/project/src/Button.tsx'))
      expect(ctx.environment.hot.send).toHaveBeenCalledWith(
        'oxc-react-docgen:update',
        expect.objectContaining({ file: '/project/src/Button.tsx' })
      )
    })

    it('returns undefined so React Fast Refresh still processes the file', async () => {
      const ctx = clientEnv()
      const result = await (plugin.hotUpdate as Function).call(ctx, makeOpts('/project/src/Button.tsx'))
      expect(result).toBeUndefined()
    })

    it('skips SSR and other non-client environments', async () => {
      const ctx = { environment: { name: 'ssr', hot: { send: vi.fn() } } }
      await (plugin.hotUpdate as Function).call(ctx, makeOpts('/project/src/Button.tsx'))
      expect(napi.extractFileIncremental).not.toHaveBeenCalled()
    })

    it('skips files outside srcDirs', async () => {
      const ctx = clientEnv()
      await (plugin.hotUpdate as Function).call(ctx, makeOpts('/project/node_modules/react/index.js'))
      expect(napi.extractFileIncremental).not.toHaveBeenCalled()
    })

    it('skips non-ts/tsx files inside srcDirs', async () => {
      const ctx = clientEnv()
      await (plugin.hotUpdate as Function).call(ctx, makeOpts('/project/src/styles.css'))
      expect(napi.extractFileIncremental).not.toHaveBeenCalled()
    })

    it('skips files in a sibling directory that shares the srcDir prefix', async () => {
      const ctx = clientEnv()
      await (plugin.hotUpdate as Function).call(ctx, makeOpts('/project/src-generated/Button.tsx'))
      expect(napi.extractFileIncremental).not.toHaveBeenCalled()
    })
  })

  // ── buildEnd ───────────────────────────────────────────────────────────────

  describe('buildEnd', () => {
    it('closes the NAPI session', () => {
      ;(plugin.buildEnd as Function)()
      expect(napi.closeSession).toHaveBeenCalledWith(42)
    })
  })
})
