---
oxc-react-docgen-core: patch
---

String-literal union aliases now report bare member values. A `type Size = "sm" | "md"` alias previously came back with each member already quoted, so serialized output showed `""sm"" | ""md""` and a template literal over the alias expanded to `compact-"sm"`. `undefined` members are dropped (optionality is reported through `required`); a `null` member keeps the alias a plain union, since a string-only literal union can't represent it.

The DTS cache schema version moved to 4 — its key is a content hash and can't detect that the extractor now produces different data for identical input, so existing cache files are discarded rather than served stale.
