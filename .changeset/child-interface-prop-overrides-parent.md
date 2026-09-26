---
oxc-react-docgen-core: patch
---

A prop redeclared in a child interface now replaces the inherited one. `interface Child extends Base { x: "a" | "b" }` used to resolve `x` to `Base`'s type, description and parent. An undocumented redeclaration keeps the inherited description, as react-docgen-typescript does.
