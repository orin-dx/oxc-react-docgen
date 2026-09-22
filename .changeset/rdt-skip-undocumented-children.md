---
oxc-react-docgen: patch
---

`--format rdt` omits a `children` prop that has no description, matching react-docgen-typescript's default `skipChildrenPropWithoutDoc`. A documented `children` is still emitted, and `--format canonical` is unchanged.
