# RDT Spec Coverage Matrix

Maps each react-docgen-typescript (RDT) output field and component pattern to the fixtures that exercise it.
Run `cargo test -p oxc-react-docgen-core --test snapshots` to validate all entries.

---

## PropType kinds

Each `PropType` variant the resolver can emit; whether any fixture produces it in a snapshot.

| Kind | Covered by |
|------|-----------|
| `string` | all fixtures |
| `number` | all fixtures |
| `boolean` | all fixtures |
| `null` | mui, shadcn |
| `unknown` | mui |
| `reactNode` | chakra, mantine, mui, react-aria |
| `cssProperties` | chakra, mui, react-aria, shadcn |
| `elementType` | chakra, mantine, mui |
| `stringLiteral` | chakra, mantine, mui, react-aria, shadcn |
| `union` | chakra, mantine, mui, react-aria, shadcn |
| `named` | chakra, mantine, mui, react-aria |
| `eventHandler` | all fixtures |
| `ref` | mantine, mui |
| `object` | mantine |
| `literalUnion` | mantine, shadcn, **panda** (after defineRecipe arg-index fix) |
| `array` | **rdt-compat/types** (`string[]` syntax) |
| `tuple` | **rdt-compat/types** (`[number, number]`) |
| `numberLiteral` | **rdt-compat/types** (`1 \| 2 \| 4 \| 8`) |
| `boolLiteral` | **rdt-compat/types** (`true` literal type) |
| `undefined` | **rdt-compat/types** (explicit `?: undefined`) |
| `sxProps` | **rdt-compat/types** (unresolved `SxProps` ref → known-pattern shortcut) |
| `htmlAttributes` | **rdt-compat/types** (`ComponentPropsWithoutRef<'button'>` as prop value) |
| `intersection` | **rdt-compat/types** (`CSSProperties & { '--accent': string }`) |
| `opaque` | **rdt-compat/types** (inline conditional type `T extends A ? B : C`) |
| `void` | ❌ not covered — `() => void` captured as `eventHandler`, standalone `void` never appears |
| `never` | ❌ not covered — no real-world prop has type `never` |
| `any` | ❌ not covered — `any` suppressed by TypeScript `strict` in fixtures |

> **Note on `void` / `never`:** These kinds exist in the type system for completeness but are not
> realistic prop types. A function return type `() => void` is always emitted as `eventHandler`,
> and `never` typically indicates a broken discriminant. No fixture is planned for either.

---

## ComponentEntry fields

| Field | Status | Covered by |
|-------|--------|-----------|
| `displayName` | ✅ | all fixtures |
| `filePath` | ✅ | all fixtures |
| `description` (non-empty) | ✅ | mui Button, panda button |
| `description` (component JSDoc, no prop leak) | ⚠️ BUG | see [Bug: component description leak](#bug-component-description-set-to-last-props-jsdoc) |
| `props` | ✅ | all fixtures |
| `inheritance` (non-empty) | ✅ | all fixtures |
| `notableInherited` | ✅ | all fixtures |
| `discriminantProp: null` | ✅ | all fixtures |
| `discriminantProp: "variant"` | ✅ | mui TextField |
| `composes` (non-empty) | ✅ | mantine, shadcn, chakra, react-aria |
| `tags` (component-level, non-empty) | ✅ | **rdt-compat/jsdoc** (`@see`, `@since`, `@category` on component) |
| `methods: []` | ✅ | all fixtures (always empty; class components not in scope) |

---

## ParsedProp fields

| Field | Status | Covered by |
|-------|--------|-----------|
| `name` | ✅ | all fixtures |
| `type` | ✅ | all fixtures |
| `required: true` | ✅ | all fixtures |
| `required: false` | ✅ | all fixtures |
| `defaultValue: null` | ✅ | all fixtures |
| `defaultValue.value` from JSDoc `@default` | ✅ | mui, mantine |
| `defaultValue.value` from code destructor | ✅ | mui (`{ color = 'primary' }`) |
| `defaultValue.computed: false` | ✅ | mui, mantine |
| `defaultValue.computed: true` | ❌ | not covered — no fixture uses computed defaults |
| `description` (non-empty) | ✅ | mui, radix |
| `tags.default` | ✅ | mui, mantine |
| `tags.deprecated` | ✅ | mantine |
| `tags.see` | ✅ | mui, mantine |
| `tags.example` | ✅ | mantine |
| `tags.since` | ✅ | **rdt-compat/jsdoc** (`@since` on `header` prop) |
| `parent` (non-null) | ✅ | all fixtures |
| `parent: null` | ✅ | shadcn (CVA-derived props) |
| `declarations` (non-empty) | ✅ | all fixtures |
| `declarations: []` | ✅ | shadcn |

---

## Component patterns

| Pattern | Status | Covered by |
|---------|--------|-----------|
| `React.forwardRef` | ✅ | shadcn, panda, mantine, react-aria |
| `React.memo` | ✅ | **rdt-compat/memo** |
| CVA variants (`cva()`) | ✅ | shadcn |
| CVA enum output in `enums` | ✅ | shadcn |
| PandaCSS `defineRecipe()` variants | ✅ | **panda** (fixed: arg-index bug corrected 2026-06-28) |
| PandaCSS `defineSlotRecipe()` | ⚠️ | see [Note: slot recipes](#note-slot-recipes-partial-support) |
| Discriminated union (per-variant props) | ✅ | mui TextField |
| Union-of-interfaces as root props type | ⚠️ BUG | see [Bug: union-of-interfaces](#bug-union-of-interfaces-root-props-type-silently-skips-component) |
| `asChild` slot pattern | ✅ | radix, shadcn, panda |
| Intersection-based prop inheritance | ✅ | chakra, mantine, mui |
| `Omit<T, Keys>` on inherited props | ✅ | chakra, mantine, mui |
| `Pick<SourceInterface, Keys>` | ⚠️ BUG | see [Bug: Pick not resolved](#bug-pickt-keys-not-resolved-even-for-source-types) |
| Controlled/uncontrolled triple | ✅ | **rdt-compat/controlled** (value + defaultValue + onValueChange) |
| Method-shorthand handler (arrow form) | ✅ | **rdt-compat/controlled** (`onOpenChange?: (open: bool) => void` → correct `eventType`) |
| Method-shorthand handler (shorthand form) | ⚠️ BUG | see [Bug: method-shorthand](#bug-method-shorthand-handler-loses-param-type) |
| JSDoc prop description | ✅ | radix, mui |
| JSDoc `@default` → `defaultValue` | ✅ | mui, mantine |
| JSDoc `@deprecated` on prop | ✅ | mantine |
| JSDoc `@see` / `@example` on prop | ✅ | mui, mantine |
| JSDoc description on component | ✅ | mui Button, panda button |
| JSDoc tags on component (`@see`, `@since`, `@category`) | ✅ | **rdt-compat/jsdoc** |
| Render-prop children | ✅ | react-aria (`ButtonRenderProps`) |
| HTML element inheritance (`notableInherited`) | ✅ | all fixtures |
| Template literal type → opaque | ❌ | mantine has `"compact-${MantineSize}"` but it's not seen as `opaque` in snapshots — verify |
| HOC pattern | ✅ | extractor unit test only (no snapshot fixture) |
| Generic component | ❌ | no fixture — generic type args ignored in resolver |
| `defaultProps` static assignment | ❌ | no fixture |
| Class component | ❌ | out of scope (modern React only) |

---

## ExtractionOutput top-level fields

| Field | Status | Covered by |
|-------|--------|-----------|
| `components` (non-empty) | ✅ | all fixtures |
| `enums` (non-empty) | ✅ | shadcn, **panda** (after defineRecipe fix) |
| `diagnostics` (non-empty) | ✅ | mantine, mui (UNRESOLVABLE_IMPORT warnings) |
| `stats.componentsExtracted` | ✅ | all fixtures |
| `stats.componentsSkipped` | ✅ | all fixtures |
| `stats.filesParsed` | ✅ | all fixtures |
| `stats.opaqueCount` | ❌ | never non-zero in any snapshot |

---

## Known gaps summary

| Priority | Gap | Path to cover |
|----------|-----|--------------|
| ✅ done | `opaque` kind | `fixtures/rdt-compat/types.tsx` (inline conditional) |
| ✅ done | `array` kind | `fixtures/rdt-compat/types.tsx` (`string[]`) |
| ✅ done | `tuple` kind | `fixtures/rdt-compat/types.tsx` (`[number, number]`) |
| ✅ done | `numberLiteral` kind | `fixtures/rdt-compat/types.tsx` (`1 \| 2 \| 4 \| 8`) |
| ✅ done | `sxProps` kind | `fixtures/rdt-compat/types.tsx` (unresolved `SxProps` ref) |
| ✅ done | `htmlAttributes` kind | `fixtures/rdt-compat/types.tsx` (`ComponentPropsWithoutRef<'button'>`) |
| ✅ done | `React.memo` wrapper | `fixtures/rdt-compat/memo.tsx` |
| ✅ done | `boolLiteral` kind | `fixtures/rdt-compat/types.tsx` (`true` literal) |
| ✅ done | `undefined` kind | `fixtures/rdt-compat/types.tsx` (explicit `?: undefined`) |
| ✅ done | `intersection` in prop value | `fixtures/rdt-compat/types.tsx` (`CSSProperties & { … }`) |
| ✅ done | Component-level JSDoc `tags` | `fixtures/rdt-compat/jsdoc.tsx` |
| ✅ done | `tags.since` on prop | `fixtures/rdt-compat/jsdoc.tsx` |
| ✅ done | PandaCSS `defineRecipe` arg-index bug | `interface.rs:77` fix; panda snapshot updated |
| ✅ done | Controlled/uncontrolled triple | `fixtures/rdt-compat/controlled.tsx` |
| ✅ done | `Pick<SourceInterface>` (documented) | `fixtures/rdt-compat/pick-source.tsx` (reveals bug — see below) |
| ✅ done | Union-of-interfaces root props (documented) | `fixtures/rdt-compat/discriminated-union.tsx` (reveals bug — see below) |
| N/A | `void` kind | standalone `void` not a real prop type |
| N/A | `never` kind | broken discriminant, not a real prop type |
| N/A | `any` kind | suppressed by `strict` mode |

---

## Known bugs (discovered via fixtures)

### Bug: method-shorthand handler loses param type

**Fixture:** `fixtures/rdt-compat/controlled.tsx`

**Symptom:** `onValueChange?(value: string): void` (TSMethodSignature form) emits `eventType: "..."` instead of `eventType: "string"`.

**Comparison:** Arrow-function form `onOpenChange?: (open: boolean) => void` correctly emits `eventType: "boolean"`.

**Root cause:** The extractor's method-signature handling doesn't extract parameter types the same way as property-signature handling. TSMethodSignature parameters are stored differently from TSFunctionType parameters.

**Impact:** All real Radix UI `.d.ts` handler props use method-shorthand syntax. Every handler in Radix Select, Dialog, Accordion, etc., will show `eventType: "..."`.

---

### Bug: `Pick<T, Keys>` not resolved even for source types

**Fixture:** `fixtures/rdt-compat/pick-source.tsx`

**Symptom:** `interface IconButtonProps extends Pick<ButtonBaseProps, 'disabled' | 'type' | 'form'>` — the `disabled`, `type`, and `form` props from the Pick are completely absent from the output. Only own props `icon` and `label` appear.

**Expected:** `disabled`, `type`, `form` should appear with `parent: { name: "ButtonBaseProps" }`.

**Root cause:** `Pick` resolution in `alias.rs` likely reaches `resolve_base_as_chain` which tries to find the base interface, but the picked subset isn't materialized into the extractor's `interface_members` map. The resolution either fails silently or returns an empty chain.

**Impact:** Any component that uses `Pick<LocalInterface, Keys>` in an `extends` clause loses all picked props. This is common in design system component families that share a base prop interface.

---

### Bug: union-of-interfaces root props type silently skips component

**Fixture:** `fixtures/rdt-compat/discriminated-union.tsx`

**Symptom:** `Accordion` with props type `AccordionSingleProps | AccordionMultipleProps` is completely absent from the output. No diagnostic is emitted. `componentsSkipped` does not increment.

**Expected:** Either the props should be merged (with `discriminantProp: "type"` detected) OR a `Diagnostic` should be emitted explaining the limitation.

**Root cause:** The resolver's component detection path checks whether the resolved props type is a direct interface reference or intersection chain. A union at the root level falls outside this check and the component is silently dropped.

**Impact:** All Radix Accordion-like components (union type per discriminant value) are invisible. MUI `TextFieldProps` as `StandardTextFieldProps | FilledTextFieldProps | OutlinedTextFieldProps` works in the MUI fixture only because it's handled via a different resolution path (likely the DTS special case).

**Workaround:** At minimum, emit a `ComponentDiagnostic` with kind `UnresolvableProps` so the caller knows the component was seen but not extracted.

---

### Bug: component description set to last prop's JSDoc

**Fixture:** `fixtures/rdt-compat/controlled.tsx`, `fixtures/rdt-compat/pick-source.tsx`, `fixtures/rdt-compat/memo.tsx`

**Symptom:** When a component has no JSDoc comment of its own, `ComponentEntry.description` is set to the description of the last prop in the interface instead of being empty.

Examples from snapshots:
- `Select.description = "Whether the select is disabled."` (from the `disabled` prop)
- `IconButton.description = "Accessible label for screen readers."` (from the `label` prop)
- `Avatar.description = "Avatar diameter in pixels. @default 40"` (from the `size` prop)

**Expected:** `description` should be `""` when there is no component-level JSDoc.

**Root cause:** The extractor's `find_jsdoc` proximity scan finds the nearest preceding block comment. When there is no component JSDoc, it finds the last prop's JSDoc instead (within the 120-byte proximity threshold).

---

## Note: slot recipes — partial support

`defineSlotRecipe` (PandaCSS) and `tv({ slots })` (tailwind-variants) are in the recognized callee list and benefit from the 2026-06-28 arg-index fix. However, slot recipe variant values are objects (`{ root: "class", header: "class" }`) rather than plain strings. The current `try_collect_cva_call` variant value extraction reads only string values, so slot recipe variants will have 0 entries in `global.enums` even after the arg-index fix.

The `RecipeVariantProps<typeof slotRecipe>` reference will fall back to opaque output with `raw: "VariantProps<typeof slotRecipe>"` in the `composes` array. This is documented as a limitation, not a bug.

**To support slot recipes:** extend the variant value extractor to accept both `string` and `ObjectExpression` values, treating the object's string values as the display representation.
