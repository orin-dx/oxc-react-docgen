---
oxc-react-docgen-core: patch
---

DOM and ES globals (`HTMLDivElement["dir"]`, `Date`, …) resolve again in projects on TypeScript 7, which moved `lib.dom.d.ts`/`lib.es5.d.ts` into its per-platform `@typescript/typescript-*` package.
