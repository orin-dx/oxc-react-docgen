# Adversarial Analysis — 2026-06-28

Testing methodology: fresh `react-docgen-typescript` baseline vs our tool across all fixture libraries, prop-by-prop diff, plus adversarial code review of the resolver and extractor layers.

---

## Executive summary

| Severity           | Count | Summary                                     |
| ------------------ | ----- | ------------------------------------------- |
| Critical           | 3     | Silent data loss, wrong comparison baseline |
| Important          | 9     | Logic bugs producing wrong or missing props |
| Minor              | 5     | Wrong metadata, misleading errors           |
| Design differences | 6     | Intentional divergences from RDT            |

Coverage on shared TSX fixtures (rdt-compat + shadcn): **Avatar: ✅ perfect match**. All others have documented gaps. DTS fixtures (chakra, mantine, mui, radix, react-aria) cannot be compared to RDT because RDT requires real TypeScript source, not declaration files.

---

## CRITICAL

### C1: Inline union props type in forwardRef silently drops the entire component

**File:** `crates/core/src/extractor/component.rs:122`, `mod.rs:413-430`

`extract_type_name_from_type` handles only `TSTypeReference` (named types) and `TSParenthesizedType`. When `forwardRef<E, InterfaceA | InterfaceB>` has an inline union as the second type arg, the function returns `None`, the `?` short-circuits, the component mapping is never created, and **the component is absent from output with zero diagnostics and `componentsSkipped: 0`**.

Confirmed: `rdt-compat/discriminated-union.tsx` — `Accordion` is completely invisible. RDT correctly handles this and emits merged props with discriminant detection.

The same failure occurs for inline intersection as props type: `forwardRef<E, PropsA & PropsB>`.

**Impact:** Any component using an inline union or intersection as its props type arg to `forwardRef` is silently dropped. Radix Accordion, all tab/dialog components with per-variant props, any component with `Props & RefAttributes<E>` will be missing.

**Root cause confirmation:**

```rust
// component.rs:120-122
let type_params = inner.type_arguments.as_ref()?;
if type_params.params.len() >= 2 {
    let (props_name, type_args) = self.extract_type_name_from_type(&type_params.params[1])?;
    // ↑ Returns None for TSUnionType → whole function returns None → no ComponentMapping
```

**Fix:** Handle `TSUnionType` and `TSIntersectionType` in `extract_type_name_from_type` by generating a synthetic anonymous alias in `source_data.type_aliases` and returning its key. At minimum emit an `UnresolvableProps` diagnostic and increment `componentsSkipped`.

---

### C2: ComponentPropsWithoutRef emits a duplicate InheritedLayer

**File:** `crates/core/src/resolver/extends.rs:50-53`, `chain.rs:165-170`, `resolver/mod.rs:198-200`

When a component's interface has `extends React.ComponentPropsWithoutRef<"button">`, the `resolve_extends_ref` function returns the layer **in both** `ResolvedChain { inheritance: vec![layer.clone()] }` AND `Some(layer)`. Then in `resolve_interface_chain`:

```rust
// chain.rs:165-170
let (parent_chain, maybe_layer) = resolve_extends_ref(...);
if let Some(layer) = maybe_layer {
    chain.inheritance.push(layer);          // layer added (copy 1)
}
chain.merge_parent(parent_chain);           // merge_parent prepends parent_chain.inheritance
```

`merge_parent` at `mod.rs:198-200` prepends `parent.inheritance` (which also has the layer):

```rust
let mut new_inheritance = parent.inheritance;   // layer is here (copy 2)
new_inheritance.append(&mut self.inheritance);  // copy 1 is here
self.inheritance = new_inheritance;             // both end up in output
```

Any component using `interface ButtonProps extends React.ComponentPropsWithoutRef<"button">` emits two identical `InheritedLayer` entries in `ComponentEntry.inheritance`.

**Impact:** Consumers iterating `inheritance` to build attribute tables will process the button attrs twice. The `notable_inherited` rendering is guarded by `contains_key` so display is fine, but the serialized JSON has duplicate layers which breaks any downstream comparison or tooling that relies on `inheritance` being a set.

---

### C3: Compare harness fixture key collision silently drops all but the last component per file

**File:** `apps/validate/src/run-ours.ts:101`

```typescript
fixture: `${lib}/${c.filePath.split('/').pop()?.replace(/\.tsx?$/, '').replace(/\.d$/, '') ?? c.displayName}`,
```

Every component extracted from the same file gets the same fixture key. The baseline loader in `compare.ts` builds a `Map` with these keys:

```typescript
return new Map(results.map((r) => [r.fixture, r]))
```

`Map` construction silently overwrites duplicate keys. For `chakra/Input.d.ts` (6 components: Input, InputGroup, InputLeftAddon, InputLeftElement, InputRightAddon, InputRightElement), only `InputRightElement` survives. The comparison then runs against `InputRightElement` data and labels it as representing the whole `chakra/Input` fixture.

**Impact:** All multi-component files show fabricated comparison data. Any coverage analysis based on this harness is wrong for Chakra UI, Ant Design, and any library that co-locates multiple components per file.

**Fix:** Key by `${lib}/${basename}/${comp.displayName}` and update compare.ts accordingly.

---

## IMPORTANT

### I1: `find_discriminant_prop` only scans the first union member for candidates

**File:** `crates/core/src/resolver/chain.rs:226-228`

```rust
let first_props = &members[0].1;
'outer: for prop in first_props {  // only members[0] is used as candidate source
```

Discriminant detection requires the candidate prop to exist in ALL members with distinct string literal values. But candidates are only pulled from `members[0]`. For a union like `ButtonProps | FilledButtonProps | OutlinedButtonProps` where `ButtonProps` has no string literal props and the discriminant `variant: "filled"` / `variant: "outlined"` only appears in members 1 and 2, no discriminant is found and the union is flat-merged.

**Impact:** Discriminated union detection depends on member ordering — a fragile invariant invisible to callers. Any union type where a shared base interface (no literals) is listed first will fail discriminant detection even when one clearly exists.

**Fix:** Collect discriminant candidates from the **intersection** of all members' prop names that have string literal types in at least one member.

---

### I2: Non-Named union members silently dropped when ≥ 2 Named members are present

**File:** `crates/core/src/resolver/alias.rs:153-218`

```rust
let named_members: Vec<(&str, Vec<ParsedProp>)> = members
    .iter()
    .filter_map(|m| {
        if let CollectedType::Named { name, .. } = m { ... Some(...) }
        else { None }  // Object/Intersection/Literal silently discarded
    })
    .collect();

if named_members.len() < 2 { /* fallback resolves all */ return chain; }

// When named_members.len() >= 2, ONLY named_members is used:
for (_, member_props) in &named_members { ... }
```

For `type Props = ButtonProps | IconButtonProps | { extraInline: string }`:

- `ButtonProps` and `IconButtonProps` → `named_members.len() == 2` → discriminated-union path
- `{ extraInline: string }` → never reaches the merge loop
- `extraInline` is silently absent from output with no diagnostic

**Impact:** Mixed unions (named interfaces + inline objects) lose all inline members. Somewhat common in Radix UI composable components.

---

### I3: `resolve_union_alias` discards `type_args` for Named union members

**File:** `crates/core/src/resolver/alias.rs:157-160`

```rust
if let CollectedType::Named { name, .. } = m {  // `..` discards `args`
    let chain = resolve_props_chain(name.as_str(), &[], ...);  // always empty args
```

For `type Props = Container<"sm"> | Container<"lg">`, both members are resolved as `resolve_props_chain("Container", &[], ...)`. The `"sm"` / `"lg"` args are thrown away, `T` is unresolved, and the `variant` prop shows as `Named("T")` instead of `"sm"` or `"lg"`.

---

### I4: `resolve_base_as_chain` silently returns empty for Union base type

**File:** `crates/core/src/resolver/alias.rs:137`

```rust
_ => ResolvedChain::default(),
```

`resolve_base_as_chain` is the base resolver for `Omit`, `Pick`, `Partial`, `Required`. It handles `Named`, `Intersection`, `Object` — but NOT `Union`. For:

```typescript
type Props = Omit<ButtonProps | IconButtonProps, 'onClick'>
```

The `base` is `CollectedType::Union(...)` which falls to `_ =>` returning empty. Zero props, no diagnostic.

---

### I5: `merge_parent` `extend()` overwrites `or_insert()` in `inherited_by_name`

**File:** `crates/core/src/resolver/mod.rs:194, 203`

```rust
// Within merge_parent, processing parent.props:
self.inherited_by_name.entry(prop.name.clone()).or_insert(prop);  // (A) child wins...

// Later in merge_parent:
self.inherited_by_name.extend(parent.inherited_by_name);          // (B) ...until overwritten
```

`or_insert` at (A) establishes "child wins" ordering for prop metadata. But `extend` at (B) unconditionally replaces all keys in `inherited_by_name` with the grandparent's version. For multi-level extends chains, prop metadata in `notableInherited` can show descriptions or types from the wrong level.

---

### I6: `_type_args` silently ignored in `resolve_interface_chain`

**File:** `crates/core/src/resolver/chain.rs:154`

```rust
pub(super) fn resolve_interface_chain(
    iface: &CollectedInterface,
    _type_args: &[String],   // never used
```

Generic props interfaces (`interface Container<T> { items: T[] }`) are always resolved with `T` unsubstituted. `ButtonProps extends Container<ButtonItem>` → `items: Array(Named("T"))`. No diagnostic is emitted. This is the same deferred gap documented in the Corsa integration plan, but the underscore-suppressed parameter means the compiler won't warn if it's ever connected.

---

### I7: JSDoc `find_jsdoc` has no "consumed" tracking — same comment matched by multiple nodes

**File:** `crates/core/src/extractor/jsdoc.rs:12-28`

```rust
const PROXIMITY_THRESHOLD: u32 = 120; // bytes
let comment = self.comments.iter().rev()
    .find(|c| c.is_block && c.span_end <= span_start && span_start - c.span_end <= PROXIMITY_THRESHOLD);
```

The same block comment can be returned by multiple calls. When a component has no JSDoc but its props interface has one, the interface JSDoc (within 120 bytes of the component binding) is assigned as the component description. When an interface has its own JSDoc but the first prop's span is within 120 bytes, that JSDoc is also assigned to the first prop.

Confirmed observations from snapshots:

- `Avatar.description = "Avatar diameter in pixels. @default 40"` — the `size` prop's JSDoc
- `Select.description = "Whether the select is disabled."` — the `disabled` prop's JSDoc
- `IconButton.description = "Accessible label for screen readers."` — the `label` prop's JSDoc

**Impact:** Every component without an explicit JSDoc leaks a nearby prop's description. First props of documented interfaces get duplicate descriptions.

---

### I8: `htmlPropPattern` in compare.ts misses ~200 standard HTML attributes

**File:** `apps/validate/src/compare.ts:75`

```typescript
const htmlPropPattern = /^(on[A-Z]|aria-|data-|class|style|id|tab|ref|key$|role$)/
```

This covers event handlers, ARIA/data attrs, `className`, `style`, `id`, `tabIndex`, `ref`, `key`, `role`. It misses: `disabled`, `hidden`, `type`, `value`, `placeholder`, `checked`, `name`, `form`, `autoFocus`, `readOnly`, `required`, `src`, `href`, `alt`, `width`, `height`, `draggable`, `spellCheck`, `lang`, `dir`, `accept`, `pattern`, `maxLength`, `min`, `max`, and ~180 others.

**Impact:** When a component has no detected inherited element (`ourInheritedElements.length === 0`), standard HTML attrs like `disabled` and `type` are counted as `❗ REAL MISSES`. The `misses` counter in the summary is systematically inflated.

---

### I9: `ourInheritedElements.length > 0` gives a blanket ✅ that hides real prop misses

**File:** `apps/validate/src/compare.ts:74-86, 118-121`

When our tool reports ANY HTML element inheritance, every rdt-only prop is classified as "covered by inheritance" regardless of whether it actually appears in `notableInherited`. `shadcn/Input` shows `wins++` even though we expose 15 notable inherited attrs vs RDT's 309. The `notableInherited` field is never included in `NormalizedOutput` and never compared.

**Impact:** The final coverage percentage is meaningless. `wins` counts zero-own-prop components as full matches purely because we detected an inherited HTML element.

---

## MINOR

### M1: TSTupleElement non-optional/rest variants downgraded to Raw

**File:** `crates/core/src/extractor/mod.rs:319-331`

```rust
other => {
    use oxc_span::GetSpan;
    let span = other.span();
    let raw = self.source[span.start as usize..span.end as usize].to_owned();
    CollectedType::Raw(raw)
}
```

Plain tuple elements (`string` in `[string, number]`) are not `TSOptionalType` or `TSRestType`. They fall to the source-text raw fallback and become `CollectedType::Raw("string")` instead of `CollectedType::String`. Any resolver logic that pattern-matches on `CollectedType::String` won't fire for tuple members.

---

### M2: Cycle detection uses pre-canonicalization key

**File:** `crates/core/src/resolver/chain.rs:43-46`

```rust
let visit_key: CompactString = format!("{}:{}", consuming_file, type_name).into();
```

Two local aliases to the same canonical type (`import { Foo as A, Foo as B }`) produce different keys (`"file:A"`, `"file:B"`). Both resolve independently, doubling diagnostics and work. Not a correctness issue (depth limit is the real cycle guard), but wastes resources on alias-heavy DTS files.

---

### M3: `OpaqueReason::DepthExceeded` used for non-depth Raw fallthrough

**File:** `crates/core/src/resolver/collected.rs` (complex expression Raw fallthrough)

Complex raw strings that can't be parsed as identifiers emit `OpaqueReason::DepthExceeded`. The reason is semantically wrong — these types were not truncated by depth, they were unrecognized syntax. Tooling that branches on `OpaqueReason::DepthExceeded` (suggesting "fix circular refs") misleads users.

---

### M4: Double resolution and duplicate diagnostics for single-Named-member unions

**File:** `crates/core/src/resolver/alias.rs:153-177`

For `type Props = ButtonProps | { extra: string }`:

1. `named_members` collects and resolves `ButtonProps` via `branch_state`
2. `named_members.len() == 1 < 2` → fallback resolves ALL members again via `resolve_base_as_chain`

`ButtonProps` is resolved twice. Any warning it emits (unresolvable import, etc.) appears twice in `state.diagnostics`.

---

### M5: `classify_extends` namespace-qualified member access falls to SameFile

**File:** `crates/core/src/extractor/mod.rs:107-126`

`SVGAttributes`, `AllHTMLAttributes`, `TableHTMLAttributes` and a dozen others are missing from both `html_element_for` and `is_react_builtin`. For `interface Props extends React.SVGAttributes<SVGElement>`, the lookup strips `"React."` correctly but finds no match, then checks `imported_names.contains("React.SVGAttributes")` — which is always false (the import is `"React"` not `"React.SVGAttributes"`) — and falls through to `ExtendsRef::SameFile { name: "React.SVGAttributes" }`. The resolver then silently produces empty for an unrecognized same-file type.

(Planned fix in `docs/superpowers/plans/2026-06-28-structural-gap-fixes.md` Task 2.)

---

## Design differences vs react-docgen-typescript

These are intentional architectural choices, not bugs. Documented here to explain comparison deltas.

| # | Behavior | Ours | RDT | Notes |
| --- | --- | --- | --- | --- |
| D1 | HTML attribute expansion | `notableInherited` (15-20 key attrs) | Inline as props (250-300 attrs) | Our approach is better for design systems; RDT is required for strict RDT compatibility |
| D2 | Literal union type format | `"sm" \| "md" \| "lg"` as actual values | `enum` kind with values as `enumValues` | Our format is better for Storybook controls |
| D3 | `VariantProps` null modifier | `union([literalUnion([...]), null])` | Implementation-dependent | CVA/PandaCSS semantics: null opts out of variant; intentional |
| D4 | Conditional type evaluation | `opaque { raw: "T extends A ? B : C" }` | `enum` (TypeScript evaluates) | Needs Corsa API; documented in `docs/type-checker-integration.md` |
| D5 | Intersection type key preservation | `CSSProperties & { ... }` (drops key names) | `CSSProperties & { '--accent': string; }` | Inline object keys lost in object field rendering |
| D6 | `key` and `ref` props | Not emitted | Always emitted as `key: Key \| null` and `ref: Ref<E>` | React internals; arguably wrong for RDT to include them |

---

## What RDT does better (areas to match)

1. **Discriminated union at root props level** — RDT correctly handles `forwardRef<E, A | B>` by merging props and detecting the discriminant. We drop the component. RDT output for `discriminated-union.tsx`:

   ```
   type: enum (required)
   value: string | string[]
   onValueChange: ((value: string) => void) | ((value: string[]) => void)
   collapsible: boolean
   ```

2. **Inline Pick in extends** — RDT resolves `Pick<ButtonBaseProps, 'disabled' | 'type' | 'form'>` in an extends clause. We don't (planned fix: chain.rs step 0.5).

3. **Conditional type evaluation** — `string extends "a" ? "yes" : "no"` → RDT emits `enum`; we emit opaque. Requires Corsa API.

---

## What we do better than RDT

1. **SxProps recognition** — `sx: SxProps` (ours) vs `sx: any` (RDT). We use the known-pattern shortcut from `known.rs`; RDT can't resolve the MUI-specific type without its tsconfig.

2. **Literal union values** — `step: 1 | 2 | 4 | 8` with actual values (ours) vs `enum` kind (RDT) where you need to inspect `enumValues`. Better for programmatic prop generation.

3. **DTS file support** — We process 9+ libraries that RDT can't touch (chakra, mantine, mui, radix, react-aria). RDT requires full TypeScript source with compiler access.

4. **Speed** — 5ms for all shadcn components vs ~200ms+ for RDT on the same files. The 10-100× claim holds in practice.

5. **No TS 7.0 breakage** — RDT depends on Strada API which is dropped in TypeScript 7.0. We're unaffected.

---

## Priority fix order

| # | Bug | Effort | Impact |
| --- | --- | --- | --- |
| C1 | Inline union props type drops component | Medium | Accordion, all discriminated-union components |
| C3 | Compare harness fixture key collision | Trivial | Accurate comparison data |
| C2 | Duplicate InheritedLayer | Small | Clean inheritance output |
| I7 | JSDoc proximity leak | Small | Correct component descriptions |
| I1 | Discriminant detection candidates from members[0] only | Small | Correct discriminated unions |
| I2 | Non-Named union members silently dropped | Medium | Mixed union types |
| I4 | resolve_base_as_chain empty for Union base | Small | `Omit<A \| B, K>` pattern |
| I8/I9 | Compare harness HTML attr classification | Small | Trustworthy coverage numbers |
| M5 | SVGAttributes + React namespace | Small | Planned in structural-gap-fixes plan |

Bugs I1–I4 (discriminant detection, union merging) are all needed before shipping Radix UI Accordion-style component support. C1 (inline union props) is the root cause of the Accordion silent drop and should be fixed first.
