# Type Checker Integration: Future Work

This document describes which resolver gaps genuinely require a TypeScript type checker,
the state of the tsgo (TypeScript 7.0) ecosystem, and the integration plan for when the
Corsa API is stable.

---

## What we can fix structurally (no type checker needed)

The six fundamental gap patterns discovered during the RDT compatibility audit break into
two groups:

**Fixable now — pure AST/structural:**

| Gap | Root cause | Fix location |
|-----|-----------|-------------|
| TSMethodSignature param loss | Hardcoded `vec![Raw("...")]` instead of reading `ms.params.items` | `extractor/mod.rs:359-374`, `476-492` |
| React namespace recognition (SVGAttributes, etc.) | Missing entries in `is_react_builtin` and `html_element_for` | `react_types.rs`, `resolver/named.rs` |
| `Readonly<T>` transparent wrapper | Missing arm in `extractor/alias.rs` | `extractor/alias.rs` |
| `typeof` expression depth | `typeof Primitive.button` not followed through | `extractor/mod.rs` TSTypeQuery arm |
| Inline `Pick<T,K>` in `extends` | Step 1 silences Pick regardless of type_args | `resolver/chain.rs` step 0.5 |

**Implementation plan:** `docs/superpowers/plans/2026-06-28-structural-gap-fixes.md`

**Deferred — require type inference:**

| Gap | Why it needs a type checker |
|-----|----------------------------|
| Generic parameter substitution (`List<T>` where T flows from call-site) | OXC gives us the AST node `T`, but to know T's concrete value we need the checker's `getTypeAtLocation` or its tsgo equivalent |
| Conditional types (`T extends U ? A : B`) | Already emitted as `opaque` — correct behavior. Full evaluation requires the checker |
| Mapped types (`{ [K in keyof T]: ... }`) | Already emitted as `opaque` — correct behavior |
| Multi-level generic propagation | A chain like `type ButtonProps = ComponentProps<typeof Button>` where Button is defined elsewhere can be 3-4 hops; our import resolver follows same-file chains but cross-file generic substitution requires the type checker |

---

## TypeScript 7.0 / tsgo ecosystem status (as of 2026-06-28)

### What happened

TypeScript 7.0 RC shipped 2026-06-18. This is the Go rewrite of the compiler ("tsgo"),
not a JavaScript refactor. The key changes:

- **TypeScript 6.0** = last JavaScript-based tsc (maintenance only)
- **TypeScript 7.0** = Go rewrite; 10× build speed, same semantics
- **Strada API** (ts.createProgram, checker.getTypeAtLocation, LanguageService) = **completely dropped** in TS 7.0 — no shim, no compatibility layer
- **react-docgen-typescript** depends on Strada; it is **broken under TS 7.0**

### Corsa API

The tsgo team is building "Corsa" as the replacement public API. Status:

- Corsa API: **not yet stable** — targeting TypeScript 7.1 (estimated late 2026 / early 2027)
- Current tsgo exports minimal surface: `check()`, `build()`, basic diagnostics
- No `getTypeAtLocation` equivalent yet
- Plugin system and language service extensions TBD

### tsgolint precedent (OXC team)

The OXC team already tackled cross-language integration with `tsgolint`:
- Spawns `tsgo` as a subprocess
- Communicates via JSON IPC using tsgo's internal (unstable) shims
- This pattern works but depends on internal APIs that will break when Corsa lands

We should NOT follow the tsgolint approach — it creates maintenance debt on an unstable
internal API. Wait for Corsa.

### Why oxc-react-docgen is unaffected by TS 7.0

We parse TypeScript with OXC (Rust), not tsc. We have no dependency on the Strada API.
Consumers running TS 7.0 can use oxc-react-docgen without any changes. This is a
competitive advantage during the TS 6→7 migration window.

---

## Integration architecture (when Corsa is stable)

The integration should be **opt-in**, not always-on. Most projects need only the structural
analysis for prop documentation. The type checker adds latency (~100-500ms cold start) and
a Node.js process dependency that many CI environments won't want.

### Proposed design

```
┌─────────────────────────────────────────────────────────┐
│  oxc-react-docgen  (current — always runs)              │
│  OXC AST parse → extractor → resolver → ExtractionOutput│
│  Props: structurally knowable types only                 │
│  Generics: emitted as PropType::Named or opaque         │
└──────────────────────────┬──────────────────────────────┘
                           │ optional
                  ─────────▼─────────
                 │  Corsa enrichment  │
                 │  (future opt-in)   │
                 │                   │
                 │  For each prop     │
                 │  with opaque/Named │
                 │  type: call Corsa  │
                 │  getTypeAtLocation │
                 │  → replace with    │
                 │  resolved PropType │
                  ───────────────────
```

**CLI flag:** `--with-type-checker` (default: off)
**Config key:** `docgen.config.ts → typeChecker: true | { path: string }`
**Node requirement:** Node 22+ (tsgo ships as npm package)

### Specific Corsa operations needed

When the Corsa API is available, the following operations unblock the deferred gaps:

| Operation | Corsa API (expected) | Gap resolved |
|-----------|---------------------|--------------|
| Resolve generic type argument at call site | `checker.getTypeArguments(typeRef)` | Generic param substitution |
| Evaluate conditional type | `checker.resolveConditionalType(node)` | Conditional opaque |
| Expand mapped type | `checker.getIndexedAccessType(type, key)` | Mapped opaque |
| Follow `typeof expr` | `checker.getTypeOfExpression(expr)` | typeof depth |

The integration point is `resolver/chain.rs:resolve_props_chain` — after step 5 fails to
find the interface in our global map, step 6 currently emits an `UnresolvableImport`
diagnostic. With Corsa, a step 5.5 would ask the type checker for the props instead.

### File plan (future)

```
crates/core/src/typechecker/          (new module, feature-gated)
  mod.rs              — feature gate, public API surface
  corsa_bridge.rs     — subprocess IPC with tsgo/Corsa
  prop_enricher.rs    — walk ExtractionOutput, replace opaque props
```

Feature gate: `cargo build --features=type-checker` — keeps the no-Node-dependency
build path as the default.

---

## Timeline

| Milestone | When | What it unlocks |
|-----------|------|----------------|
| TS 7.0 RC | 2026-06-18 (done) | Confirms Strada is dropped; competitive window opens |
| TS 7.1 release candidate | ~Q4 2026 | Corsa API draft; begin bridge prototype |
| TS 7.1 stable + Corsa stable | ~Q1 2027 | Implement generic substitution, `typeof` depth |
| Public release of `--with-type-checker` | ~Q1 2027 | Feature-complete RDT replacement |

Until then: fix the 5 structural gaps (see implementation plan), ship, and let users
migrate to oxc-react-docgen during the TS 7 disruption window.
