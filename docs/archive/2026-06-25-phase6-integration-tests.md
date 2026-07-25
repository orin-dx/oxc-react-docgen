# Phase 6 — Integration Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `moon run validate:compare` so running a single command generates all three tool baselines (react-docgen, react-docgen-typescript, oxc-react-docgen) and prints a prop-diff comparison.

**Architecture:** Most of Phase 6 already exists — `run-ours.ts`, `compare.ts`, `baseline.ts`, `types.ts`, and `fixtures.ts` are complete. Three gaps remain: `baseline.ts` doesn't include running ours, no moon task exists for the CLI build, and no `moon.yml` wires `apps/validate`. We add those three pieces and update docs.

**Tech Stack:** moon (task orchestration), TypeScript/tsx (scripts), cargo (Rust build)

---

## File Map

| Action | File                            | Purpose                                              |
| ------ | ------------------------------- | ---------------------------------------------------- |
| Create | `crates/cli/moon.yml`           | Exposes `cli:build` so validate can depend on it     |
| Modify | `apps/validate/src/baseline.ts` | Add ours runner (currently only rdg + rdt)           |
| Modify | `apps/validate/package.json`    | Add `run:ours` and `compare:all` scripts             |
| Create | `apps/validate/moon.yml`        | `compare` task: deps `cli:build`, runs full pipeline |
| Modify | `docs/09-STATUS.md`             | Mark Phase 6 complete, remove stale bug entry        |

---

## Task 1: Create `crates/cli/moon.yml`

**Files:**

- Create: `crates/cli/moon.yml`

The CLI crate package name in Cargo.toml is `oxc-react-docgen` (it's a binary). The workspace.yml already has `cli: "crates/cli"` registered, so `cli:build` becomes a valid moon task reference.

- [ ] **Step 1: Write the file**

```yaml
language: rust

tasks:
  build:
    command: cargo build -p oxc-react-docgen
    options:
      runFromWorkspaceRoot: true
    inputs:
      - 'src/**/*'
      - 'Cargo.toml'
      - '../../crates/core/src/**/*'
      - '../../crates/core/Cargo.toml'
```

Note: no `outputs` block — `target/` is gitignored and cargo is already incremental; moon doesn't need output tracking here.

- [ ] **Step 2: Verify moon sees the task**

```bash
moon run cli:build
```

Expected: cargo compiles (or says "nothing to do"), exits 0. Binary exists at `target/debug/oxc-react-docgen`.

- [ ] **Step 3: Commit**

```bash
git add crates/cli/moon.yml
git commit -m "feat: add moon build task for CLI crate"
```

---

## Task 2: Update `apps/validate/src/baseline.ts` to include ours

**Files:**

- Modify: `apps/validate/src/baseline.ts`

The current file runs `run-react-docgen.ts` and `run-react-docgen-typescript.ts`, then saves their JSON output to `./baselines/`. We add the same for `run-ours.ts`.

`run-ours.ts` calls `target/debug/oxc-react-docgen extract ...` — the binary must already exist when this script runs. The moon task in Task 4 ensures `cli:build` runs first.

- [ ] **Step 1: Add ours runner**

Current `apps/validate/src/baseline.ts` ends with:

```typescript
console.log('Running react-docgen-typescript baseline...')
writeFileSync('./baselines/react-docgen-typescript.json', runScript('run-react-docgen-typescript.ts'))
console.log('✅ react-docgen-typescript baseline saved')

console.log('\nBaselines saved to ./baselines/')
```

Change it to:

```typescript
console.log('Running react-docgen-typescript baseline...')
writeFileSync('./baselines/react-docgen-typescript.json', runScript('run-react-docgen-typescript.ts'))
console.log('✅ react-docgen-typescript baseline saved')

console.log('Running oxc-react-docgen baseline...')
writeFileSync('./baselines/oxc-react-docgen.json', runScript('run-ours.ts'))
console.log('✅ oxc-react-docgen baseline saved')

console.log('\nBaselines saved to ./baselines/')
```

- [ ] **Step 2: Verify (requires binary from Task 1)**

```bash
cd apps/validate && pnpm baseline
```

Expected output:

```
Running react-docgen baseline...
✅ react-docgen baseline saved
Running react-docgen-typescript baseline...
✅ react-docgen-typescript baseline saved
Running oxc-react-docgen baseline...
✅ oxc-react-docgen baseline saved

Baselines saved to ./baselines/
```

All three files must exist: `ls baselines/` shows `oxc-react-docgen.json  react-docgen-typescript.json  react-docgen.json`

- [ ] **Step 3: Commit**

```bash
git add apps/validate/src/baseline.ts
git commit -m "feat: include oxc-react-docgen in baseline generation"
```

---

## Task 3: Add scripts to `apps/validate/package.json`

**Files:**

- Modify: `apps/validate/package.json`

Add two scripts:

- `run:ours` — for manual one-off runs (outputs JSON to stdout)
- `compare:all` — for the moon task: runs baseline + compare in sequence

- [ ] **Step 1: Update scripts block**

Current scripts:

```json
"scripts": {
  "run:rdg": "tsx src/run-react-docgen.ts",
  "run:rdt": "tsx src/run-react-docgen-typescript.ts",
  "compare": "tsx src/compare.ts",
  "baseline": "tsx src/baseline.ts"
}
```

Replace with:

```json
"scripts": {
  "run:rdg": "tsx src/run-react-docgen.ts",
  "run:rdt": "tsx src/run-react-docgen-typescript.ts",
  "run:ours": "tsx src/run-ours.ts",
  "compare": "tsx src/compare.ts",
  "baseline": "tsx src/baseline.ts",
  "compare:all": "tsx src/baseline.ts && tsx src/compare.ts"
}
```

- [ ] **Step 2: Smoke-test compare:all (requires binary)**

```bash
cd apps/validate && pnpm compare:all 2>&1 | tail -10
```

Expected last lines:

```
============================================================
SUMMARY
============================================================
react-docgen-typescript: 879 total props across all components
oxc-react-docgen:        155 total props across all components
Coverage: 155/879 (18%)
Identical matches: 3  |  Both empty: 1  |  Misses: 0
```

(Numbers may differ slightly as fixtures evolve.)

- [ ] **Step 3: Commit**

```bash
git add apps/validate/package.json
git commit -m "feat: add run:ours and compare:all scripts to validate package"
```

---

## Task 4: Create `apps/validate/moon.yml`

**Files:**

- Create: `apps/validate/moon.yml`

The `compare` task runs `pnpm compare:all` (baseline + compare) with a dep on `cli:build`. The `cache: false` option is required because baselines are gitignored — moon can't hash them as outputs for its cache system.

Moon runs tasks from the project root (package dir) by default, so `./baselines/` resolves correctly to `apps/validate/baselines/`.

- [ ] **Step 1: Write the file**

```yaml
language: typescript

tasks:
  compare:
    command: pnpm compare:all
    deps:
      - 'cli:build'
    inputs:
      - 'src/**/*'
      - '../../../fixtures/**/*'
    options:
      cache: false
```

- [ ] **Step 2: Verify end-to-end via moon**

```bash
moon run validate:compare
```

Expected flow:

1. Moon runs `cli:build` (cargo build, ~1s if cached)
2. Moon runs `validate:compare` (generates baselines, prints diff)
3. Final lines show the SUMMARY block

- [ ] **Step 3: Commit**

```bash
git add apps/validate/moon.yml
git commit -m "feat: add moon compare task for validate app"
```

---

## Task 5: Update `docs/09-STATUS.md`

**Files:**

- Modify: `docs/09-STATUS.md`

Three changes: phase table row, remove stale bug entry, update Immediate Next Steps.

- [ ] **Step 1: Update the Phase Completion table**

Change:

```
| 6 — Integration tests | 🟡 Partially ready | run-ours.ts exists in apps/validate/; needs compare.ts, baseline command, and moon wiring |
```

To:

```
| 6 — Integration tests | ✅ Complete | moon run validate:compare — rdg + rdt + ours baselines → prop-diff comparison |
```

- [ ] **Step 2: Remove the stale "run-ours.ts missing" bug entry**

Remove this section from "Current Known Bugs":

```
### 🟢 Minor: `run-ours.ts` missing in apps/validate

The validation harness has `run-react-docgen.ts` and `run-react-docgen-typescript.ts` but no `run-ours.ts` to compare against. Can only be written after the NAPI binary is compiled.
```

(It was already wrong before this plan — `run-ours.ts` existed — now it's fully wired.)

- [ ] **Step 3: Update Immediate Next Steps**

Change from:

```
1. **Phase 6 — Integration tests** (`apps/validate/`) — `run-ours.ts` exists; need `compare.ts`, baseline snapshot, and `moon run validate:compare`
2. **Config file loading** — `crates/cli/src/config.rs` parses `docgen.config.ts` but discards the result (returns `None`); wire the JSON → `PipelineOptions` mapping
3. **Preset system** — named `OxcDocgenOptions` bundles in a `@oxc-react-docgen/presets` package (config-side only, no Rust changes)
```

To:

```
1. **Config file loading** — `crates/cli/src/config.rs` parses `docgen.config.ts` but discards the result (returns `None`); wire the JSON → `PipelineOptions` mapping
2. **Preset system** — named `OxcDocgenOptions` bundles in a `@oxc-react-docgen/presets` package (config-side only, no Rust changes)
```

- [ ] **Step 4: Commit**

```bash
git add docs/09-STATUS.md
git commit -m "docs: mark Phase 6 complete, remove stale run-ours.ts bug entry"
```

---

## Self-Review

**Spec coverage:**

- `compare.ts` ✅ already exists
- `baseline` command ✅ updated in Task 2 to include ours
- `moon run validate:compare` ✅ wired in Task 4

**Placeholder scan:** No TBDs or vague steps. All code shown verbatim.

**Type consistency:** No new types. All file paths are exact.

**Edge case — panda fixtures:** `fixtures.ts` already skips `panda` (`if (dir === 'panda') continue`) because of styled-system subdirectory complexity. The baseline scripts all go through `discoverFixtures()`, so panda is consistently excluded everywhere.

**Edge case — run-ours.ts CWD:** `run-ours.ts` resolves the CLI path via `resolve(__dirname, '../../../target/debug/oxc-react-docgen')` — this is an absolute path computed from the script file location, not from CWD. Safe to run from any directory.
