import type { Plugin, ResolvedConfig, ViteDevServer, HotUpdateOptions } from 'vite'
import * as napi from '@oxc-react-docgen/napi'
import type { ExtractionOutput, IncrementalUpdate } from '@oxc-react-docgen/napi'

const VIRTUAL_ID = 'virtual:oxc-react-docgen'
const RESOLVED_VIRTUAL_ID = `\0${VIRTUAL_ID}`

export interface OxcDocgenOptions {
  srcDirs: string[]
  exclude?: string[]
  reactVersion?: 'react18' | 'react19'
  skipHtmlProps?: boolean
}

export function oxcReactDocgen(options: OxcDocgenOptions): Plugin {
  let sessionId: number
  let root: string
  let command: 'build' | 'serve' | undefined
  let resolvedSrcDirs: string[] = []
  let currentOutput: ExtractionOutput = {
    components: {},
    enums: {},
    diagnostics: [],
    stats: {
      componentsExtracted: 0,
      componentsSkipped: 0,
      filesParsed: 0,
      dtsFilesParsed: 0,
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
      htmlAttributes: options.skipHtmlProps ? ('none' as const) : undefined,
    }
  }

  // configureServer (dev-only) is where cold extraction normally runs and
  // populates `currentOutput` for the virtual module. Vite never calls
  // configureServer during `vite build`, so without this, a production
  // build's virtual module always resolved to the empty placeholder below.
  async function coldExtract(): Promise<boolean> {
    const json = await napi.initializeSession(sessionId, napiOptions())
    try {
      currentOutput = JSON.parse(json) as ExtractionOutput
      return true
    } catch (error) {
      console.error('[oxc-react-docgen] Failed to parse initializeSession output:', error)
      return false
    }
  }

  function isSrcFile(file: string): boolean {
    return (file.endsWith('.tsx') || file.endsWith('.ts')) && resolvedSrcDirs.some((dir) => file.startsWith(`${dir}/`))
  }

  return {
    name: 'oxc-react-docgen',

    configResolved(config: ResolvedConfig) {
      ;({ root, command } = config)
      resolvedSrcDirs = options.srcDirs.map((d) => (d.startsWith('/') ? d : `${root}/${d}`))
      sessionId = napi.createSession(napiOptions())
    },

    // `vite build` never calls configureServer — that hook only fires for
    // the dev server — so the build path needs its own cold extraction here.
    // Skipped for `serve`, where configureServer already handles it.
    async buildStart() {
      if (command !== 'build') return
      try {
        await coldExtract()
      } catch (error) {
        console.error('[oxc-react-docgen] initializeSession failed:', error)
      }
    },

    configureServer(server: ViteDevServer) {
      // Vite guarantees configResolved fires before configureServer;
      // sessionId and resolvedSrcDirs are safe to use here.
      //
      // This hook must return its teardown function synchronously — it can't
      // be `async` or `await` coldExtract() here without breaking that contract,
      // so the fire-and-forget .then()/.catch() chain below is intentional,
      // not an oversight.
      /* oxlint-disable promise/prefer-await-to-then, promise/always-return, promise/prefer-await-to-callbacks */
      initPromise = coldExtract()
        .then((ok) => {
          if (!ok) return
          // environments.client.hot is Vite 6+ API; cast avoids typing fight across patch versions
          ;(server as any).environments?.client?.hot?.send('oxc-react-docgen:ready', {
            components: currentOutput.components,
          })
        })
        .catch((error: unknown) => {
          console.error('[oxc-react-docgen] initializeSession failed:', error)
        })
      /* oxlint-enable promise/prefer-await-to-then, promise/always-return, promise/prefer-await-to-callbacks */

      // Return a teardown function — Vite calls this on dev-server close.
      // buildEnd is not called in dev mode, so this is the only reliable cleanup hook.
      return () => {
        napi.closeSession(sessionId)
      }
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
      } catch (error) {
        console.error('[oxc-react-docgen] Failed to parse extractFileIncremental output:', error)
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
