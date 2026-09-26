---
oxc-react-docgen-core: patch
oxc-react-docgen: patch
oxc-react-docgen-napi: patch
---

An unrecognized `htmlAttributes` value (a typo in `docgen.config.ts` or in the JS options) is now an error naming the value, instead of silently falling back to `curated`, as `reactVersion` already does.
