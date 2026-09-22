---
oxc-react-docgen-core: patch
oxc-react-docgen: patch
---

Fixed 7 doc-comment errors that broke `cargo doc` under `-D warnings`: 6 unescaped angle-bracket type names (`Ref<T>`, `RefObject<T>`, `HTMLButtonElement`, etc.) that rustdoc parsed as unclosed HTML tags, and one `[default]` that rustdoc parsed as a broken intra-doc link. No behavior change — fixes how the published crate's documentation renders on docs.rs.
