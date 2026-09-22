---
oxc-react-docgen-core: patch
---

Ambient `.d.ts` files are merged for their types only, never their components. `@types/react` declares `class PureComponent<P, S, SS> extends Component<P, S, SS>`, which matched the class-component detector — full HTML-attribute mode emitted `PureComponent` as a 0-prop component alongside a `Cannot resolve type 'P'` diagnostic. Both are gone, from batch extraction and from the watch session's first revision.
