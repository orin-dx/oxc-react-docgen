---
oxc-react-docgen-core: patch
oxc-react-docgen: patch
oxc-react-docgen-napi: patch
---

Lint policy (`unsafe_code`, clippy `all`) moved from per-crate `#![...]` attributes into a single `[workspace.lints]` table in the workspace root, with each crate opting in via `[lints] workspace = true`. `unsafe_code` is now `"deny"` rather than `"forbid"` — no crate currently uses `unsafe`, including the NAPI FFI boundary, but `deny` allows a local `#[allow(unsafe_code)]` override if one is ever genuinely needed there.

Also dropped 11 dependencies that `cargo-machete` confirmed were unused: `indexmap`, `oxc_module_lexer`, `thiserror`, `tokio`, `dirs`, `miette` (core); `anyhow`, `clap_mangen`, `lsp-types`, `tracing-indicatif` (cli); `rustc-hash` (binding). No behavior change — smaller dependency graph and faster builds.
