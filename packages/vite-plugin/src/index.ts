import type { Plugin, ResolvedConfig, ViteDevServer, HotUpdateOptions } from 'vite'
import * as napi from '@oxc-react-docgen/napi'
import type { ExtractionOutput, IncrementalUpdate } from '@oxc-react-docgen/napi'

const VIRTUAL_ID = 'virtual:oxc-react-docgen'
const RESOLVED_VIRTUAL_ID = '\0' + VIRTUAL_ID

export interface OxcDocgenOptions {
  srcDirs: string[]
  exclude?: string[]
  reactVersion?: 'react18' | 'react19'
  skipHtmlProps?: boolean
}

export function oxcReactDocgen(options: OxcDocgenOptions): Plugin {
  let sessionId: number
  let root: string
  let resolvedSrcDirs: string[] = []
  let currentOutput: ExtractionOutput = {
    components: {},
    enums: {},
    diagnostics: [],
    stats: {
      componentsExtracted: 0,
      componentsSkipped: 0,
      filesParsed: 0,
      dtsCacheHits: 0,
      durationMs: 0,
      tier1Count: 0,
      tier3Count: 0,
      opaqueCount: 0,
    },
  }
  // Holds the in-flight init promise so hotUpdate can await it before first incremental call.
  let initPromise: Promise<void> | null = null

  function napiOptions() {
    return {
      srcDirs: resolvedSrcDirs,
      exclude: options.exclude,
      reactVersion: options.reactVersion,
      skipHtmlProps: options.skipHtmlProps,
    }
  }

  function isSrcFile(file: string): boolean {
    return (
      (file.endsWith('.tsx') || file.endsWith('.ts')) &&
      resolvedSrcDirs.some(dir => file.startsWith(dir))
    )
  }

  return {
    name: 'oxc-react-docgen',

    configResolved(config: ResolvedConfig) {
      root = config.root
      resolvedSrcDirs = options.srcDirs.map(d => (d.startsWith('/') ? d : `${root}/${d}`))
      sessionId = napi.createSession(napiOptions())
    },

    configureServer(server: ViteDevServer) {
      // Vite guarantees configResolved fires before configureServer;
      // sessionId and resolvedSrcDirs are safe to use here.
      initPromise = napi
        .initializeSession(sessionId, napiOptions())
        .then(json => {
          try {
            currentOutput = JSON.parse(json) as ExtractionOutput
          } catch (err) {
            console.error('[oxc-react-docgen] Failed to parse initializeSession output:', err)
            return // leave currentOutput as the zero-value
          }
          // Notify client that initial extraction is ready
          // environments.client.hot is Vite 6+ API; cast avoids typing fight across patch versions
          ;(server as any).environments?.client?.hot?.send('oxc-react-docgen:ready', {
            components: currentOutput.components,
          })
        })
        .catch((err: unknown) => {
          console.error('[oxc-react-docgen] initializeSession failed:', err)
          // Do NOT re-throw — keep initPromise fulfilled so hotUpdate doesn't reject silently
        })
    },

    async hotUpdate(opts: HotUpdateOptions) {
      // Vite 6+ supplies this.environment in the hotUpdate context.
      // We cast rather than annotating `this:` to avoid fighting the Plugin interface's
      // declared this-type across different Vite patch versions.
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const env = (this as any).environment as
        | { name: string; hot: { send(event: string, data?: unknown): void } }
        | undefined

      if (!env || env.name !== 'client') return
      if (!isSrcFile(opts.file)) return

      // Wait for cold extraction to finish before the first incremental call.
      if (initPromise) await initPromise

      const json = await napi.extractFileIncremental(opts.file, sessionId, napiOptions())
      let update: IncrementalUpdate
      try {
        update = JSON.parse(json) as IncrementalUpdate
      } catch (err) {
        console.error('[oxc-react-docgen] Failed to parse extractFileIncremental output:', err)
        env.hot.send('oxc-react-docgen:update', { file: opts.file, updatedComponents: [], diagnostics: [] })
        return
      }

      for (const comp of update.updatedComponents) {
        currentOutput.components[comp.displayName] = comp
      }

      env.hot.send('oxc-react-docgen:update', {
        file: opts.file,
        updatedComponents: update.updatedComponents,
        diagnostics: update.diagnostics,
      })

      // Return undefined (NOT []) — returning [] suppresses React Fast Refresh
      // for the changed file. We want both: updated types AND component re-render.
    },

    resolveId(id: string) {
      if (id === VIRTUAL_ID) return RESOLVED_VIRTUAL_ID
    },

    load(id: string) {
      if (id !== RESOLVED_VIRTUAL_ID) return
      return `export default ${JSON.stringify(currentOutput)}`
    },

    buildEnd() {
      napi.closeSession(sessionId)
    },
  }
}
