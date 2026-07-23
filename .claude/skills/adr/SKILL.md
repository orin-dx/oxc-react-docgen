---
name: adr
description: Apply when a decision was just made that's expensive to reverse and not obvious from the code, or when asked to write or review an Architecture Decision Record.
---

**Goal:** A future contributor (or you, in six months) understands why a
hard-to-reverse decision was made, in under a minute of reading.

## Decide if it's ADR material

Ask: if this gets reverted in a year, what breaks, and would anyone know why?
If the answer is "nothing" or "it's obvious from the code," it's not an ADR —
it's a commit message.

Real candidates: a format/dependency/architecture choice with a hidden
constraint, an approach that was tried and abandoned, a trade-off that isn't
visible just from reading the result. Not candidates: bug fixes,
behavior-preserving refactors, anything `CLAUDE.md`'s non-negotiables already
cover.

Unsure? Ask rather than guess — writing one nobody wanted is worse than
skipping one that mattered.

## Write it

Full rules and voice guide: `docs/adr/README.md`. Template:
`docs/adr/0000-template.md`. Read both before writing — don't rely on memory
of them.

Find the next number by listing `docs/adr/*.md` and incrementing the
highest. Copy the template, but treat its sections as a starting shape, not
a form to fill mechanically — a one-paragraph decision doesn't need a forced
"Alternatives considered" with nothing real in it. Cut sections that would
just say "n/a."

Ground every claim in something checkable — a file, a line, a number, a test
you actually ran. If you're about to write "this is faster" or "this was
necessary," go verify it first (read the code, run the test, check git
history) instead of asserting from a vague sense that it's probably true.

## Before you save it

Read it back once as the target contributor, not the author. Cut every
sentence that would survive being deleted — check against `docs/adr/README.md`'s
banned-word list and before/after table. If a section reads like a corporate
template ("In conclusion, this represents..."), delete it; ADRs stop when
the decision's been explained, they don't wrap up with a summary of
themselves.

Short beats complete. If the technical depth genuinely doesn't fit on a
screen, split it into its own doc and link it — see how
`0004-defer-type-checker-integration.md` points at
`docs/type-checker-integration.md` instead of inlining it.
