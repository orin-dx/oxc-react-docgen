# Architecture Decision Records

Short, dated records of decisions that are expensive to reverse and not obvious
from reading the code. Not a changelog, not a design doc, not a place to
explain what the code does — comments and `ARCHITECTURE.md` already do that.

## When to write one

Ask: if this gets reverted in a year, what breaks, and would anyone know why?
If the answer is "nothing" or "it's obvious," skip it.

Good candidates:

- Choosing OXC over the TypeScript compiler
- A serialization format with a hidden constraint (msgpack's positional encoding)
- Dropping a dependency, or an approach that was tried and abandoned

Not ADR material:

- A bug fix — commit message, and if it's the kind of thing that could recur,
  a line in `rdt-coverage.md`
- A behavior-preserving refactor — commit message is enough
- Anything already obvious from the code or from `CLAUDE.md`'s non-negotiables

## Format

One file per decision: `docs/adr/NNNN-short-title.md`, numbered sequentially,
never reused. Copy `0000-template.md` to start.

- **Never edit history.** If a decision changes, write a new ADR and mark the
  old one's status `Superseded by 000X`. Don't rewrite the old file to look
  like you knew all along.
- **Status** is one of: `Accepted`, `Superseded by NNNN`, `Deprecated`.
- Keep it short. If the technical depth doesn't fit on a screen, split it into
  its own doc and link it — see `0004-defer-type-checker-integration.md`
  pointing at `type-checker-integration.md` instead of inlining it.

## Writing it

Write like you're explaining it to the next engineer over their shoulder, not
presenting it to a committee.

**Cut the filler.** If a sentence still means the same thing without a word,
delete the word.

| Instead of | Write |
|---|---|
| "It's worth noting that this decision was made in order to leverage OXC's robust parsing capabilities" | "OXC parses in parallel with no type-checking pass" |
| "This represents a significant improvement in performance" | "32ms instead of several seconds" |
| "We decided to essentially utilize a positional encoding approach" | "We use positional encoding" |
| "In conclusion, this architecture provides a solid foundation going forward" | (delete — you already said the thing) |

**Banned by default:** *leverage, utilize, robust, seamless, cutting-edge,
essentially, basically, in order to, it's worth noting, it is important to
note, at the end of the day.* If one is genuinely the precise technical word
for something, fine — but check first.

**Concrete beats vague.** Name the file, the number, the benchmark. "Slow" is
not a fact; "several seconds on a mid-size design system" is.

**Active voice, plain structure.** "We chose X because Y" beats "X was chosen
due to Y considerations." Sentence-case headers, not Title Case.

**Show the trade-off, not just the win.** A decision with no downside isn't a
decision, it's a fact. Say what it costs.

## What good looks like

Read `0002-manual-serialize-for-prop-type.md` — a real bug behind it, a real
number (`recursion_limit = 2048`), a real consequence, four sections, no
throat-clearing.
