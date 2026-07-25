# 0004. Defer full type-checker integration

**Status:** Accepted **Date:** 2026-07-22 (retroactive)

## Context

OXC's structural, no-type-checking approach (`0001`) can't resolve conditional types, mapped types over an unbound generic, or a few `@emotion/styled`-specific call shapes — these need real type inference. `typescript-go` (TypeScript 7.0's Rust/Go port, "Corsa") is the plausible future path, but its public API for this isn't stable yet.

## Decision

Don't integrate a type checker now. Every gap that would need one degrades to a `Diagnostic` plus an `Opaque` prop type instead of a wrong answer, and gets tracked in `docs/rdt-coverage.md`'s "Known gaps summary" table. Revisit when Corsa's API stabilizes.

## Consequences

- A real, bounded set of real-world types (see `docs/type-checker-integration.md` for the current list) shows up as `Opaque` in extraction output until this changes.
- No dependency on an unstable, fast-moving API in the meantime.
- The gap list needs to stay current in `docs/type-checker-integration.md` and `docs/rdt-coverage.md` as new patterns get found — otherwise this ADR quietly goes stale.
