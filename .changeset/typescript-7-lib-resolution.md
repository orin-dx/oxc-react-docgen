---
oxc-react-docgen-core: patch
---

DOM and ES globals (`HTMLDivElement["dir"]`, `Date`, …) resolve again in projects on TypeScript 7, which moved `lib.dom.d.ts`/`lib.es5.d.ts` into its per-platform `@typescript/typescript-*` package. A `typescript` install whose lib files can't be found (TypeScript 7 without its optional platform package) now warns instead of silently leaving those globals unexpanded.
