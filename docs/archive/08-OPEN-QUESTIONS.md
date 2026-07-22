# Open Questions & Architecture Decisions

Tracks gaps in the phase specs, decisions needed before implementation, and
accuracy/DX concerns to carry forward into each phase.

---

## Config File Name

**Decision:** `docgen.config.ts` (not `oxc-react-docgen.config.ts`)

Follows `vite.config.ts` / `eslint.config.js` convention. Short, obvious in
context, no collision risk. Config shape mirrors `PipelineOptions` + a
`propFilter` function.

TODO (Phase 4b): implement config file loading in the CLI.

---

## Validation Strategy

We compare output against two baselines on the same fixture files:

| Tool | Basis | What it proves |
|------|-------|----------------|
| `react-docgen` 7.x | babel, JS/TSX only | detection accuracy vs the original |
| `react-docgen-typescript` 2.x | tsc, full type resolution | the primary drop-in target |
| **ours** | OXC, no tsc | speed + correctness |

Infrastructure: `packages/validate/` — run `pnpm baseline` then `pnpm compare`.

### Validation fixture libraries (priority order)

1. **shadcn/ui** — cva + forwardRef + VariantProps ✅ in fixtures
2. **Radix UI** — ComponentPropsWithoutRef + asChild ✅ in fixtures
3. **MUI** — deepest type chains, SxProps, OverridableStringUnion ✅ in fixtures
4. **Chakra UI** — HTMLChakraProps, ThemingProps ✅ in fixtures
5. **Mantine** — StylesApiProps, polymorphic component ✅ in fixtures
6. **React Aria** — ARIA-first, render props, no HTML inheritance ✅ in fixtures

### Accuracy dimensions to validate

- Components detected (false positives, false negatives)
- Prop names (completeness)
- Type accuracy (`"string" | "number"` vs `"enum"`)
- Required/optional correctness
- Default values extracted
- JSDoc descriptions preserved
- `parent.fileName` pointing to correct file (critical for RDT propFilter compat)
- Speed: our tool vs rdt (target: 10-100x faster)

---

## React Best Practices — Implementation Gaps

### React 19 `ref` as plain prop
In React 19, `ref` is passed as a regular prop — `forwardRef` is no longer
needed and is deprecated. The extractor must detect:
```tsx
// React 19 — ref is just a prop
function Button({ ref, ...props }: ButtonProps & { ref?: Ref<HTMLButtonElement> }) {}
```
`ReactVersion.ref_as_prop` exists in `react_types.rs` — Phase 2a must wire it up.

### Compound components
`Dialog.Trigger`, `Select.Item` etc. are not addressed in Phase 2a spec.
Pattern: `const DialogTrigger = Dialog.Trigger` or `Dialog.Trigger = DialogTrigger`.
Decision needed: detect and emit as separate components, or skip?
**Recommendation:** emit as `Dialog.Trigger` display name, separate ComponentEntry.

### `displayName` fallback
Some HOC-wrapped components set `Comp.displayName = 'Button'` after definition.
Phase 2a extractor must scan for `MemberExpression.displayName = StringLiteral`
assignments to correct the component name.

### `defaultProps` (deprecated but present in .d.ts)
MUI ships `defaultProps` in its .d.ts declarations. We should read them as
default values rather than ignoring them. Phase 3a resolver decision.

### Generic components
`Table<TData extends object>`, `Select<T>` etc. — the extractor currently
only handles non-generic component patterns. Phase 2a: when a component's
props type has type params, emit `Named { name, args }` and let the resolver
degrade gracefully.

---

## Performance

### SLOs (non-negotiable)
- `parse_single_file`: < 10µs per file
- `full_pipeline` (50 components): < 10ms wall clock
- Watch mode incremental update: < 5ms per changed file

### DTS Cache (`cache.rs`) — UNDESIGNED
This is the main cross-run perf mechanism. Needs design before Phase 3a.
Required interface:
```rust
pub struct DtsCache { ... }
impl DtsCache {
    pub fn load_from_disk() -> Self;
    pub fn get(&self, path: &Utf8Path, mtime: SystemTime) -> Option<SourceData>;
    pub fn insert(&self, path: &Utf8Path, mtime: SystemTime, data: SourceData);
    pub fn save_to_disk(&self);
}
```
Storage: MessagePack (rmp-serde) keyed by `(path, mtime)`.
Location: `dirs::cache_dir() / "oxc-react-docgen" / "dts-cache.msgpack"`.

### Watch mode incremental — UNIMPLEMENTED
`WatchSession::update_file` is `todo!()`. Needs reverse-dep graph walk.
Phase 3b must implement before Phase 4a (NAPI depends on it).

---

## Accuracy — Edge Cases Not in Spec

### Unhandled TypeScript utility types
Currently specified: `Omit`, `Pick`, `Partial`, `Required`.
Also present in real libraries:
- `Exclude<T, U>` — often used in variant types
- `Extract<T, U>` — opposite of Exclude
- `ReturnType<typeof fn>` — Mantine uses this for recipe return types
- `Parameters<typeof fn>[0]` — uncommon but present
- `NonNullable<T>` — already in `known.rs` as passthrough

**Recommendation for Phase 3a:** degrade unknown utility types to
`Opaque { reason: OpaqueReason::ConditionalType }` with a diagnostic,
rather than silently emitting empty props.

### JSDoc `@param` → prop description
The spec maps JSDoc `@tags` as a flat `BTreeMap<String, String>`.
Real libraries use `@param propName description` to document props.
Phase 2a extractor should parse `@param {type} propName description`
and merge into the prop's `description` field.

### JSDoc `@default` tag
Some libraries document defaults via JSDoc rather than destructuring:
```ts
/** @default "md" */
size?: 'sm' | 'md' | 'lg'
```
Phase 2a: parse `@default <value>` and populate `RawProp`'s default.
Phase 3a: surface as `ParsedProp.default_value`.

### Event handler type inference
The spec detects `MouseEventHandler` etc. from baked-in names.
Real libraries use: `(event: React.MouseEvent<HTMLButtonElement>) => void`.
Phase 2a: parse inline function types and classify as `EventHandler`.

---

## DX Checklist

- [ ] `docgen.config.ts` config file (Phase 4b)
- [ ] `oxc-react-docgen inspect Button` shows props in a table (Phase 4b) ✅ specified
- [ ] Shell completions (Phase 4b) ✅ specified
- [ ] Vite plugin `propFilter` option (Phase 5a) ✅ specified
- [ ] Storybook autodocs drop-in (Phase 5a) ✅ via __docgenInfo
- [ ] Diagnostic messages point to exact source location via miette (Phase 3a+)
- [ ] `--react-version` auto-detection from `package.json` peerDependencies (Phase 4b)
- [ ] Watch mode `oxc-react-docgen watch` (Phase 4b) ✅ specified

---

## Decisions Made

The following questions were resolved during the Phase 3/4/5 architectural review.
They are recorded here to prevent re-litigation.

| Topic | Decision |
|---|---|
| **styled-components** | SKIP — deprecated, no longer relevant for new projects |
| **emotion** | POST-v1 — high complexity, deferred |
| **Cache default location** | `{project_root}/node_modules/.cache/oxc-react-docgen` — NOT the user home directory. CI-friendly: invalidated with `node_modules`, matches Node.js tooling convention. |
| **Config file** | `docgen.config.ts` — discovered by walking up from the project root to the workspace root (stops at `pnpm-workspace.yaml`, `package.json` with `"workspaces"`, or `.git`). Single file at workspace root covers all packages. |
| **Vite plugin return type** | `Plugin[]` (not `Plugin`) — allows splitting extraction plugin from virtual module plugin, required for Vite 8 / Rolldown architecture |
| **Vite 8 migration** | Three breaking changes all spec'd: `hotUpdate` (not `handleHotUpdate`), `moduleType: 'js'` in transform return, `environment` API instead of `server` in hot update. See Phase 5a spec. |
| **`CollectedType` vs raw string** | `CollectedType` structured enum replaces `raw_type: String` throughout. Done in `types.rs` update. Resolver pattern-matches on it directly — no string parsing in the resolver. |
| **`InheritedLayer` + `notable_inherited`** | Added to `ComponentEntry` in `types.rs` update. `InheritedLayer` carries `file_name`, `html_element`, `omitted`, and `total_props`. `notable_inherited` is the curated subset for display in component docs. |
| **typescript-go integration** | POST-v1, opt-in via `resolveComplexTypes: true` option. Handles `Conditional` and `Mapped` types that degrade to `Opaque` in the current resolver. No changes to the core resolver are needed now — the opt-in flag gates a separate resolution pass. |
