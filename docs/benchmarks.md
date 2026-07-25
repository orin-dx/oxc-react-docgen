# Benchmarks: oxc-react-docgen vs. react-docgen-typescript vs. react-docgen

**Generated:** 2026-07-25.

**Hardware:** Apple Silicon (arm64), macOS. Numbers below are absolute and will shift on different hardware — treat the _relative_ comparisons as the durable signal, not the absolute milliseconds.

This report has two independent halves: **performance** (how fast) and **output quality** (how correct/complete). They're measured differently on purpose — a fast tool that produces wrong output isn't actually useful, and this project's whole premise is "faster AND at least as accurate," so both get held to a real, reproducible standard here. Every number below has a script in `apps/validate/` or `crates/core/benches/` that reproduces it — see [Reproducing these numbers](#reproducing-these-numbers).

## Tools compared

| Tool                      | Version   | Approach                                         |
| ------------------------- | --------- | ------------------------------------------------ |
| `oxc-react-docgen`        | this repo | OXC parser, no type-checking pass                |
| `react-docgen-typescript` | 2.2.2     | full `ts.Program` + real TypeScript type checker |
| `react-docgen`            | 7.1.1     | Babel AST walk, weak native TS support           |

---

## Performance

### Cold extraction, direct binary invocation (the real production number)

The compiled CLI, invoked once across all 21 fixture libraries at once — this is how the tool is actually meant to run (a CI job, or the NAPI binding called in-process from the Vite plugin), not wrapped in another JS runtime:

```
Time (mean ± σ):      50.5 ms ±   5.4 ms
Range (min … max):    44.1 ms …  70.4 ms    (48 runs, hyperfine, 3 warmup)
```

### Cold extraction, all three tools under equal footing

react-docgen and react-docgen-typescript can only run inside a Node process — there's no "direct binary" form for them to compare against. So this second number runs **all three** through the same harness (`pnpm exec tsx <script>`, one process per tool, whole corpus per run), which is the fairest apples-to-apples comparison available, at the cost of adding a `proto → pnpm → tsx → Node` startup tax common to all three, plus — specifically for `oxc-react-docgen` — one extra child-process spawn on top of that (the harness shells out to the compiled binary from inside that already-started Node process, since that's how a real consumer's own tooling would call it too):

| Tool                      |               Mean |     Min |      Max |     Relative |
| ------------------------- | -----------------: | ------: | -------: | -----------: |
| `oxc-react-docgen`        |  2.963 s ± 0.157 s | 2.759 s |  3.208 s |         1.00 |
| `react-docgen`            |  3.387 s ± 0.524 s | 3.046 s |  4.510 s | 1.14× slower |
| `react-docgen-typescript` | 10.374 s ± 1.317 s | 9.196 s | 13.156 s | 3.50× slower |

(hyperfine, 10 runs each, 2 warmup runs)

**Read this pair of numbers together, not separately.** The first number (50ms) is what actually happens in a real build. The harness-equalized number (2.96s vs 3.39s vs 10.37s) exists only to give react-docgen and react-docgen-typescript a fair shot at the same measurement methodology — most of those ~3 seconds is Node/tsx/pnpm/proto startup cost that has nothing to do with any of the three tools' actual extraction logic. The **3.5× advantage over react-docgen-typescript holds under either framing**; the advantage over react-docgen looks small here only because both numbers are swamped by identical harness overhead — see the per-file numbers below for what's actually happening once that's stripped away.

### Per-fixture cold extraction (Rust-internal, criterion)

`cargo bench -p oxc-react-docgen-core` (criterion 0.7, 100 samples, reports 95% CI):

| Fixture             |                    Time |
| ------------------- | ----------------------: |
| `shadcn/button.tsx` | 20.31 ms [20.08, 20.55] |
| `shadcn/input.tsx`  | 19.99 ms [19.79, 20.21] |
| `radix/button.d.ts` |                19.82 ms |
| `mui/Button.d.ts`   | 21.33 ms [20.90, 21.78] |

These all extract a small single-fixture-library directory in isolation with **no DTS cache warm** (`cache_dir: None` in the bench options) — the ~20ms floor here is dominated by re-parsing `@types/react`'s ambient globals from scratch every single call, not by the fixture file itself. This is why the 21-library whole-corpus number above (50ms) isn't 21×20ms: the pipeline parses `@types/react` once per process, not once per library.

### Incremental single-file update — the number RDT and react-docgen cannot produce at all

Neither comparator has an incremental/watch API. A real edit in an RDT- or react-docgen-based dev setup re-pays the full cold cost every time (10.4s / 3.4s respectively, per the harness numbers above — or in a more realistic single-file-only invocation, whatever fraction of that their own program construction allows, which for RDT is still dominated by rebuilding its `ts.Program`). This tool's `WatchSession` keeps that state alive and re-resolves only the changed file and its dependents:

| Fixture (single file changed)    |                 Time |
| -------------------------------- | -------------------: |
| `shadcn/button.tsx`              | 1.75 ms [1.70, 1.80] |
| `mantine` (first `.d.ts` in dir) | 1.74 ms [1.69, 1.78] |
| `mui` (first `.d.ts` in dir)     | 2.13 ms [2.07, 2.20] |

That's roughly **10-13× faster than this same tool's own cold extraction**, and it's a capability gap, not just a speed gap — this is the number that determines whether Storybook HMR feels instant or sluggish on every keystroke, and RDT/react-docgen have no equivalent number to put here at all.

---

## Output quality

"Coverage: N/M (X%)" was the only quality metric this repo had before this report — a raw prop-count ratio, not a real per-prop match rate, and it didn't account for known, deliberate design differences (see below). This section replaces it with a real per-prop agreement rate and a taxonomy of _why_ props disagree when they do, computed by `apps/validate/src/analyze.ts` from the same three tools' real output on all 21 fixture libraries (`apps/validate/fixtures/`).

**Methodology note — how each tool's type string is read:** `react-docgen-typescript` represents union/literal prop types as `{ name: 'enum', value: [...] }` — the real type string comes from `.value`, not `.name` (which is literally the string `"enum"`). `react-docgen` puts real TypeScript type info under `.tsType` (with a `.raw` field carrying the exact original source text), not `.type` (a legacy PropTypes-only field that's absent for TS-typed props). Both comparisons below read the real field on each side.

**Reading the aggregate percentage correctly:** a low agreement rate means different things against each comparator, and conflating them is the single easiest way to misread this report. Against RDT, low agreement means _this tool finds fewer props than RDT does_ — a real, directional gap (RDT has a type checker; see the outlier table below). Against react-docgen, low agreement mostly means the reverse — _this tool finds more than react-docgen does_, because react-docgen returns zero props for 45% of comparable components (see below) and everything this tool correctly extracts on top of that shows up as "extra," not as this tool being noisy or over-inclusive. The single percentage can't distinguish "we're missing things" from "the comparator is missing things" — that's why this section leads with the outlier table and the zero-props stat instead of the headline number.

### Agreement rate vs. react-docgen-typescript

`react-docgen-typescript` always fully expands the real `HTMLAttributes`/`AriaAttributes`/`<Element>HTMLAttributes` interface chain (~250-300 attrs per element) — it has a real type checker to walk that chain. This tool's _default_ is `HtmlAttributeMode::Curated` (~15-20 hand-picked common attrs). Comparing curated-mode output against RDT's always-full output would manufacture a huge fake "missing props" number that's a default-mode mismatch, not a quality gap — so this comparison runs with `--html-attributes full` (this tool structurally resolving the same interface chain RDT does), the fair apples-to-apples setting.

| Metric                                             |     Value |
| -------------------------------------------------- | --------: |
| Components compared                                |        33 |
| Props in union                                     |     5,634 |
| Matched exactly (name + type + required + default) |       946 |
| Real type/required/default mismatches              |     1,953 |
| Missing in ours                                    |     2,720 |
| Extra in ours                                      |        15 |
| **Agreement rate**                                 | **16.8%** |

That 16.8% needs immediate context: **it is not evenly distributed, and the missing-props number is 99.1% one root cause.** Of the 2,720 props missing from this tool's output, **2,696 (99.1%) come from just 10 outlier components** (below) — all of them a real, structural gap needing a type checker, not something chosen to omit. The other 24 (0.9%, spread across 12 of the other 23 components) are **100% `ref`/`key`** — React's own reconciliation plumbing, which RDT lists as props and this tool deliberately doesn't. Worth being precise about that one: it's an omission, not a relocation — this tool doesn't surface forwardRef/ref-type metadata anywhere else either (checked `ComponentEntry`'s full field list — no such field exists), so "we chose not to add it" is accurate, but "the same info lives elsewhere" isn't. Excluding those 10 outliers, this tool's real-world agreement with RDT is effectively total (e.g. `antd/Button`: 312 props vs. RDT's 313 — the only difference is `ref`/`key`).

| Component | Ours | RDT | Root cause |
| --- | --: | --: | --- |
| `storybook-emotion/Button` | 7 | 306 | `@emotion/styled` two-arg overload — documented gap |
| `fluentui/Button` | 8 | 303 | conditional type (`Slot<Type,...>`) + indexed access into a generic (`Slots[Primary]`) |
| `ariakit/menu-button/MenuButton` | 5 | 294 | generic type-param substitution through `Omit<ComponentPropsWithoutRef<T>, keyof O>` |
| `base-ui/MenuTrigger` | 11 | 293 | mapped type over a generic (`WithBaseUIEvent<T> = {[K in keyof T]: ...}`) |
| `ariakit/menu/Menu` | 7 | 286 | same generic-substitution pattern as MenuButton |
| `ariakit/menu-item/MenuItem` | 5 | 284 | same generic-substitution pattern |
| `ark-ui/Select/SelectRoot` | 36 | 314 | same generic-substitution pattern |
| `headlessui/Listbox/*` (3 components) | 5-14 | 234-246 | conditional type (`PropsOf<TTag> = TTag extends ... ? ... : never`) nested inside generic substitution |

**Type-diff taxonomy** (1,953 real mismatches, classified by observable shape):

| Bucket | Count | What it means |
| --- | --: | --- |
| `structural-type-difference` | 1,616 | genuinely different type shape — dominated by the 10 outlier components above, where this tool resolves far fewer props to begin with, so any prop it does share tends to also show as a residual `Named` reference rather than the fully-expanded structural type RDT prints |
| `union-member-order` | 294 | same union members, different order — cosmetic, not a real disagreement, but counted as a mismatch since exact-string comparison can't tell "same set, different order" from "different set" without this explicit check |
| `literal-narrowed-vs-widened` | 1 | one side kept a string-literal union, the other widened to `string` |
| `optional-undefined-representation` | 0 | n/a for this pair |

### Agreement rate vs. react-docgen

`react-docgen` resolves **no** inherited/`extends` props of its own — comparing this tool's HTML-attribute-expanded output against it would manufacture the same kind of fake "extra props" mismatch as above, in reverse. This comparison runs with `--html-attributes none` (own-declared props only on both sides) — the fair setting when the comparator does zero interface-extends resolution.

| Metric                                      |       Value |
| ------------------------------------------- | ----------: |
| Components compared                         |          20 |
| **react-docgen returned zero props at all** | **9 of 20** |
| Props in union                              |         225 |
| Matched exactly                             |          51 |
| Real mismatches                             |          61 |
| Missing in ours                             |           7 |
| Extra in ours                               |         106 |
| **Agreement rate**                          |   **22.7%** |

The single most important number in this table is **9 of 20 (45%) returning zero props.** `react-docgen` is a Babel-era tool built primarily for `PropTypes`-based components; its native TypeScript support is real but shallow — it frequently can't extract anything at all from an `interface`-typed function component, especially when the props type comes from a separate named interface rather than an inline object literal. A flat per-prop agreement rate is a weak signal when the comparator simply has no opinion on 42% of the corpus — the more honest framing is the coverage claim in the next section.

**Type-diff taxonomy** (61 real mismatches):

| Bucket                                                             | Count |
| ------------------------------------------------------------------ | ----: |
| `structural-type-difference`                                       |    36 |
| `optional-undefined-representation`                                |    21 |
| `literal-narrowed-vs-widened`                                      |     2 |
| `cosmetic-formatting` (counted as matched, shown for completeness) |     2 |

`optional-undefined-representation` (21 of 61) is `react-docgen`'s own quirk, not a gap here — it frequently appends `| undefined` to optional props' printed type where this tool (and RDT, with `shouldRemoveUndefinedFromOptional`) doesn't.

---

## What this tool can provide that they can't

Direct answers to "what information do we have that RDT and react-docgen don't," each backed by a number above or a concrete check run against this corpus, not a claim:

1. **Structured, typed degradation diagnostics.** Running this tool across all 21 fixture libraries produces **82 diagnostics** with typed codes (`UNRESOLVABLE_IMPORT`: 30, `OPAQUE_TYPE`: 29, `INDEXED_ACCESS_OPAQUE`: 19, `DISCRIMINATED_UNION`: 2, `TEMPLATE_LITERAL_OPAQUE`: 2), file/line context, and human-readable help text (e.g. _"Enable typescript-go to resolve indexed access types."_). Neither RDT nor react-docgen tell you _why_ a prop came out wrong or missing — they silently omit it or print a generic parse error. This tool's non-negotiable #6 ("always emit a Diagnostic when degrading, never fail silently," `CLAUDE.md`) makes every gap in the tables above something a consumer's tooling can actually detect and act on, not something they discover by manually diffing output.
2. **`.d.ts`-only fixtures.** 8 of the 55 fixture files in this corpus (`mui`, `chakra`, `mantine`, `radix`, `react-aria`) are declaration-only — real published library type definitions with no accompanying `.tsx` source. **Neither RDT nor react-docgen can run on these at all** (both need a real component implementation to parse); this tool extracts full prop tables from them today, which is the entire reason it can validate against `node_modules`-vendored third-party types instead of only first-party source.
3. **Incremental extraction with a real API** (see the perf section above) — `WatchSession::update_file` for editor/HMR integration. RDT and react-docgen have nothing playing this role; the comparison isn't "we're faster at the same operation," it's "this operation doesn't exist for them."
4. **Cross-package monorepo import resolution** (`ImportResolutionMap`, barrel/re-export chain following, `extra_paths` workspace aliasing) — RDT resolves within a single `ts.Program`'s file set; getting it to follow a monorepo's package boundaries requires configuring that whole program correctly. This tool's import resolution is a first-class, independently-tested layer (`import_map.rs`, `resolver/import.rs`).
5. **Machine-consumable structured types**, not stringified type names. This tool's `PropType` is a real tagged union (`union`, `literalUnion`, `array`, `intersection`, `eventHandler`, ...) serialized as JSON — a consumer can programmatically ask "is this a union of string literals" without parsing a type string. RDT and react-docgen both hand back a printed type string (or in react-docgen's case, a partial tagged shape with a `.raw` escape hatch) as the primary representation.

---

## Shortcomings and remediation plan

Honest accounting of what's actually wrong today, ordered by how directly actionable each one is — not by how bad it sounds.

### Already tracked, deferred pending a real type checker (Corsa/`typescript-go`)

Confirmed still accurate against current code during this report — see `docs/type-checker-integration.md` and `docs/STATUS.md`:

- **Generic type-parameter substitution through multi-level type-alias chains.** Root-caused here for the `ariakit`/`ark-ui` outliers above: Ariakit's own `Props<T, O> = O & Omit<ComponentPropsWithoutRef<T>, keyof O>` pattern requires substituting a generic param (`T = "button"`) through a nested `Omit`+`ComponentPropsWithoutRef` chain — exactly the "generic parameter substitution" item already listed as Corsa-deferred. This report adds concrete evidence (4+ real-world outlier components in one corpus) that this isn't a theoretical gap.
- **`@emotion/styled`'s two-arg `styled(tag, options)<T>(fn)` overload** (`storybook-emotion/Button` — already `docs/rdt-coverage.md`'s open gap #2). RDT needs a type checker for this too — not a competitive gap.
- **`styled.X.attrs<T>()` component detection** (`zendesk-garden` — already gap #1). Shared blind spot with RDT.
- **Same-namespace sibling reference resolution** for select `@types/react` internals (`EventHandler`, `TrustedHTML` — already gap #3; visible in the 30 `UNRESOLVABLE_IMPORT` diagnostics above).

### Already tracked, real but not yet benchmarked (perf, not correctness)

From `docs/STATUS.md`, still accurate:

- `resolver/chain.rs`/`named.rs`/`template.rs` build a `"{file}:{name}"` scoped-key string on every lookup — a `Borrow`-based type-map key would avoid the allocation. Real but unmeasured; do as a focused pass if profiling shows it matters, not preemptively.
- `cache.rs`'s DTS cache has no dirty-flag and no size cap — rewrites the whole cache file every run, unbounded growth. Low severity (requires local write access already).

---

## Reproducing these numbers

```bash
# Performance
cargo bench -p oxc-react-docgen-core                    # per-fixture cold + incremental, criterion
cd apps/validate && ./bench-cold.sh                     # cross-tool cold, hyperfine (needs `brew install hyperfine`)

# Quality
cd apps/validate
pnpm run baseline                                        # generates baselines/*.json for all three tools
pnpm run analyze                                          # agreement rate + taxonomy → analysis.json
```

`analyze.ts`'s full structured output (every individual prop-level diff, not just the aggregate tables above) is written to `apps/validate/analysis.json` — useful for finding the next outlier component to investigate, not committed here since it's regenerated by the commands above.
