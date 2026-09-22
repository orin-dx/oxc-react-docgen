# Type Checker Integration: Future Work

This document describes which resolver gaps genuinely require a TypeScript type checker, the current TypeScript 7 ecosystem, and the two integration routes that must be evaluated before one is selected. “Corsa” was the native port's codename, not the name of a guaranteed drop-in compiler API.

---

## What we can fix structurally (no type checker needed)

The six fundamental gap patterns discovered during the RDT compatibility audit break into two groups:

**Fixable now — pure AST/structural:**

| Gap | Root cause | Fix location |
| --- | --- | --- |
| TSMethodSignature param loss | Hardcoded `vec![Raw("...")]` instead of reading `ms.params.items` | `extractor/mod.rs:359-374`, `476-492` |
| React namespace recognition (SVGAttributes, etc.) | Missing entries in `is_react_builtin` and `html_element_for` | `react_types.rs`, `resolver/named.rs` |
| `Readonly<T>` transparent wrapper | Missing arm in `extractor/alias.rs` | `extractor/alias.rs` |
| `typeof` expression depth | `typeof Primitive.button` not followed through | `extractor/mod.rs` TSTypeQuery arm |
| Inline `Pick<T,K>` in `extends` | Step 1 silences Pick regardless of type_args | `resolver/chain.rs` step 0.5 |

**Implementation plan:** `docs/archive/2026-06-28-structural-gap-fixes.md` (done — see `docs/STATUS.md`)

**Deferred — require type inference:**

| Gap | Why it needs a type checker |
| --- | --- |
| Generic parameter substitution (`List<T>` where T flows from call-site) | OXC gives us the AST node `T`, but to know T's concrete value we need the checker's `getTypeAtLocation` or its tsgo equivalent |
| Conditional types (`T extends U ? A : B`) | Already emitted as `opaque` — correct behavior. Full evaluation requires the checker |
| Mapped types (`{ [K in keyof T]: ... }`) | Already emitted as `opaque` — correct behavior |
| Multi-level generic propagation | A chain like `type ButtonProps = ComponentProps<typeof Button>` where Button is defined elsewhere can be 3-4 hops; our import resolver follows same-file chains but cross-file generic substitution requires the type checker |

---

## TypeScript 7.0 ecosystem status (verified 2026-09-11)

### What happened

TypeScript 7.0 shipped 2026-07-08. This is the Go rewrite of the compiler and language service, not a JavaScript refactor. The key changes:

- **TypeScript 6.x** remains the JavaScript implementation and compatibility API for tools that require `ts.createProgram`/`TypeChecker`.
- **TypeScript 7.0** is the native Go implementation; Microsoft reports roughly 10× build speed on many projects and ships a multi-threaded LSP language service.
- **The Strada programmatic API is not supported by the native implementation.** TypeScript 7's curated programmatic API remains a separately evolving surface and must be inspected at implementation time rather than inferred from TypeScript 6 names.
- A consumer can keep the TypeScript 6 API installed side-by-side for tooling while using TypeScript 7 for checking. `react-docgen-typescript` compatibility therefore depends on the installed API package and cannot be summarized as simply “broken under TypeScript 7.”

### Programmatic API

The native team has described a curated, message-passing API rather than a complete port of the TypeScript 6 in-process surface. As of this verification pass:

- TypeScript 7 itself is stable and its LSP-backed editor surface includes definitions, references, rename, hover, call hierarchy, and other semantic operations.
- A stable public programmatic contract for the exact per-node type queries this project needs is not assumed. API shape and availability must be re-read from the pinned release before a design names methods.
- Dated quarter estimates and hypothetical `checker.*` calls are not requirements.

### tsgolint precedent (OXC team)

The OXC team already ships `tsgolint` as a real semantic-analysis precedent:

- It separates Oxlint's syntax/structural frontend from a TypeScript-native semantic backend.
- It proves semantic analysis can be added without replacing the fast structural default.
- Its integration boundary is optimized for lint rules and is not a stable library contract for arbitrary prop-type queries.

Do not copy private/internal interfaces. Study its process and data boundary, then depend only on a published contract or isolate a pinned backend behind this project's own optional adapter.

### Why oxc-react-docgen is unaffected by TS 7.0

We parse TypeScript with OXC (Rust), not tsc. We have no dependency on the Strada API. Consumers running TS 7.0 can use oxc-react-docgen without any changes. This is a competitive advantage during the TS 6→7 migration window.

---

## Integration architecture

The integration remains **opt-in**, not always-on. Most projects need only structural prop documentation. A semantic backend adds project loading, a warm process/session, toolchain compatibility, failure modes, and memory cost that must be measured locally rather than asserted from an external benchmark.

Two routes are credible:

| Route | Benefit | Cost/risk | Gate |
| --- | --- | --- | --- |
| Stable TypeScript 7 programmatic/message API | Purpose-built queries and potentially lower protocol overhead | Exact API and stability for per-node type materialization remain release-dependent | Compile/behavior spike against a pinned public release |
| Warm TypeScript 7 language service over LSP | Available semantic operations, explicit capability negotiation, strong editor precedent | Protocol operations may not expose the exact expanded type shape; needs document-version binding, timeout/crash supervision, and lifecycle ownership | Capability probe plus fault and accuracy suite |

The existing `oxc-react-docgen lsp` command is an **outward-facing server scaffold** for editors. A semantic backend would be an **inward-facing client** to a TypeScript service. They share protocol vocabulary but not responsibility, state, or process ownership.

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
                 │ semantic enrichment│
                 │  (optional backend)│
                 │                   │
                 │  For each prop     │
                 │  with opaque/Named │
                 │  type: query the   │
                 │  selected backend  │
                 │  → replace with    │
                 │  resolved PropType │
                  ───────────────────
```

The eventual CLI/config shape is not specified here. It must be added in the implementation spec after a backend wins the evaluation; this document does not invent stable flags or runtime requirements.

### Semantic operations needed

Whichever backend is selected must expose the following operations without relying on hypothetical method names:

| Operation | Required returned fact | Gap resolved |
| --- | --- | --- |
| Resolve generic type argument at call site | Concrete type arguments plus declaration/source identity | Generic param substitution |
| Evaluate conditional type | Materialized result or an explicit unresolved/unsupported outcome | Conditional opaque |
| Expand mapped/indexed type | Property keys and value types with provenance | Mapped opaque |
| Follow `typeof expr` | Resolved expression type plus target identity | typeof depth |

The integration point remains after structural resolution cannot materialize a type. The structural result is preserved, and an optional enrichment request may replace an `Opaque`/unresolved leaf only when the backend returns a content-bound answer. Backend absence, unsupported capability, timeout, stale response, or crash leaves the current diagnostic plus `OpaqueReason`; it does not fail extraction or fabricate a type.

### Boundary and ownership

`crates/core` keeps an I/O-free enrichment request/result contract over owned data. It does not spawn a compiler, own an LSP connection, depend on an async runtime, or retain protocol types. Process/session ownership belongs in an optional backend crate or the CLI/binding layer, which converts backend responses into the core contract.

The selected adapter remains feature-gated and off by default. The feature/crate name is chosen in the implementation spec, not here.

---

## Evaluation and decision gates

Before selecting either route, freeze fixtures for each deferred type class and compare structural-only, programmatic API, and LSP arms when available. Report semantic correctness against a direct TypeScript checker oracle separately from compatibility with `react-docgen-typescript`; neither comparator is the other's ground truth.

The LSP arm also must pass absent executable, unsupported capability, initialization/request timeout, stale document version, malformed/oversized frame, crash/restart, restart-storm, partial project failure, and shutdown/reaping fixtures. Record cold/warm latency and peak RSS. Until a route clears both accuracy and lifecycle gates, structural analysis plus explicit `OpaqueReason` remains the correct product behavior.
