# Phase 5a — Vite Plugin Spec

**Status:** Not started  
**Package:** `@oxc-react-docgen/vite-plugin`  
**Depends on:** `@oxc-react-docgen/napi`

---

## 1. Monorepo structure

```
crates/
  core/        — pure extraction logic (unchanged)
  binding/     — NAPI shims (renamed from crates/napi/)
  cli/         — CLI (unchanged)
packages/
  napi/        — @oxc-react-docgen/napi  (npm package, wraps crates/binding)
  vite-plugin/ — @oxc-react-docgen/vite-plugin
apps/
  validate/    — comparison harness (moved from packages/validate/)
```

**Migration steps:**

- Rename `crates/napi/` → `crates/binding/` (update `Cargo.toml` workspace members + `packages/napi/` build script)
- Move `packages/validate/` → `apps/validate/`
- Delete `packages/rolldown-plugin/`
- Add `packages/napi/package.json` with `napi` block (platform targets, binary name)
- Add `packages/vite-plugin/package.json`

---

## 2. Plugin API

### Shape

Returns a single `Plugin` object — not `Plugin[]`. Composing multiple plugins is the caller's responsibility.

```typescript
import type { Plugin } from 'vite'

export interface OxcDocgenOptions {
  srcDirs: string[]
  exclude?: string[]
  reactVersion?: 'react18' | 'react19'
  // ...mirrors ExtractOptions from @oxc-react-docgen/napi
}

export function oxcReactDocgen(options: OxcDocgenOptions): Plugin
```

### Peer dependency

```json
"peerDependencies": {
  "vite": "^7.0.0 || ^8.0.0"
}
```

No `enforce: 'pre'` — not needed. OXC reads files directly from disk; it does not participate in Vite's per-file transform pipeline.

---

## 3. Hook implementation

### `configResolved`

Capture the resolved root and merge it with user options. Create the NAPI session:

```typescript
configResolved(config) {
  root = config.root
  sessionId = napi.createSession({ srcDirs: resolvedSrcDirs(options, root) })
}
```

### `configureServer`

Run the initial cold extraction once the dev server is ready, then populate the virtual module:

```typescript
configureServer(server) {
  napi.initializeSession(sessionId, resolvedOptions).then(json => {
    currentOutput = JSON.parse(json) as ExtractionOutput
    server.environments.client.hot.send('oxc-react-docgen:ready', { components: currentOutput.components })
  })
}
```

### `hotUpdate`

Called by Vite on every file change. Guard: only process `.tsx` / `.ts` files in `srcDirs`; skip SSR environment; return `undefined` (not `[]`) to let React Fast Refresh continue uninterrupted.

```typescript
async hotUpdate(this: { environment: { name: string; hot: { send: Function } } }, opts) {
  if (this.environment.name !== 'client') return
  if (!isSrcFile(opts.file)) return

  const json = await napi.extractFileIncremental(opts.file, sessionId, resolvedOptions)
  const update = JSON.parse(json) as IncrementalUpdate

  // Patch in-memory output
  for (const comp of update.updatedComponents) {
    currentOutput.components[comp.displayName] = comp
  }

  this.environment.hot.send('oxc-react-docgen:update', {
    file: opts.file,
    updatedComponents: update.updatedComponents,
  })

  // Return undefined — do NOT return [] (that would suppress React Fast Refresh)
}
```

**Why `return undefined` not `return []`:** returning `[]` tells Vite "no modules need HMR for this file," which prevents React Fast Refresh from processing the changed component. We want type metadata updated AND the component to re-render.

### `resolveId` + `load`

Virtual module that exposes the current extraction output to the app at runtime:

```typescript
resolveId(id) {
  if (id === 'virtual:oxc-react-docgen') return '\0virtual:oxc-react-docgen'
},

load(id) {
  if (id !== '\0virtual:oxc-react-docgen') return
  return `export default ${JSON.stringify(currentOutput)}`
}
```

The `\0` prefix is the Vite convention for virtual modules — it prevents other plugins from trying to resolve or transform it.

### `buildEnd`

Release the NAPI session to free memory:

```typescript
buildEnd() {
  napi.closeSession(sessionId)
}
```

---

## 4. HMR client integration

Consumers subscribe to custom events in their app code:

```typescript
// In app — e.g. a Storybook addon or dev tool
if (import.meta.hot) {
  import.meta.hot.on('oxc-react-docgen:update', ({ file, updatedComponents }) => {
    // refresh prop tables, update Storybook controls, etc.
  })
}
```

The plugin does not inject this client code automatically — consumers wire it up via the virtual module or the HMR event directly.

---

## 5. Session ID stability

`createSession` is called in `configResolved`, before the dev server starts. The session ID is stable for the lifetime of the Vite process. On `buildEnd` (or server close) the session is released via `closeSession`.

If `configureServer` is called before `initializeSession` resolves (race between Vite startup and cold extraction), the `hotUpdate` handler queues updates and replays them once initialization completes. Implementation: a `Promise` ref that `hotUpdate` awaits before calling `extractFileIncremental`.

---

## 6. Testing

### Layer 1 — unit tests (no binary)

`packages/vite-plugin/tests/unit/`  
`vi.mock('@oxc-react-docgen/napi', factory)` — mock all NAPI calls.  
Test each hook's logic in isolation by calling `plugin.hotUpdate.call(thisCtx, opts)` directly.  
`pool: 'threads'` (safe — no native binary loaded).

### Layer 2 — integration tests (requires built binary)

`packages/vite-plugin/tests/integration/`  
`createServer({ configFile: false, middlewareMode: true, plugins: [oxcReactDocgen(...)] })`  
Gate with `NAPI_BINARY_AVAILABLE` env var.  
`pool: 'forks'` (required — native `.node` binary cannot load in worker threads).

### Layer 3 — snapshot validation

Existing `apps/validate/` harness once `run-ours.ts` is implemented.

---

## 7. Decisions log

| Decision | Rationale |
| --- | --- |
| Single `Plugin`, not `Plugin[]` | No internal composition needed; simpler for consumers to compose |
| Vite `^7.0.0 \|\| ^8.0.0` | 7 is current stable; 8 ships Rolldown (March 2026) — no API surface we use changes |
| No `enforce: 'pre'` | OXC reads disk directly; no need to intercept Vite's transform pipeline |
| `return undefined` from `hotUpdate` | Returning `[]` suppresses React Fast Refresh for the changed file |
| `environment.hot.send()` not `server.ws.send()` | Vite 6+ environment API; `server.ws` is Vite 5 compat only |
| `\0virtual:` prefix | Vite convention for virtual modules — prevents transform pipeline interference |
| Phase 5b (Rolldown) dropped | Rolldown 1.x has no external native Rust plugin mechanism; all plugins must use JS |
