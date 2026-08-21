## Type of change
- [ ] Bug fix
- [ ] New feature / pattern support
- [ ] Performance
- [ ] Refactor (no behavior change)
- [ ] Docs

## Why
What problem this solves or what pattern it adds support for — not a restatement of the diff.

## Test plan
- [ ] `cargo test -p oxc-react-docgen-core` passes
- [ ] `cargo clippy -p oxc-react-docgen-core -- -D warnings` passes
- [ ] Ran `cargo insta review` and accepted only intentional snapshot changes (if extraction output changed)
- [ ] `pnpm --filter @oxc-react-docgen/vite-plugin test` passes (if the Vite plugin or NAPI surface changed)

## Related
Closes #<issue>
