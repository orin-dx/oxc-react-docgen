/**
 * Stand-in for `code/core/src/theming/index.ts`'s real re-export surface,
 * trimmed to the two names `Button.tsx` actually imports:
 * https://github.com/storybookjs/storybook/blob/next/code/core/src/theming/index.ts
 *
 * The real file re-exports `styled` verbatim:
 *   export { default as styled } from '@emotion/styled';
 * `@emotion/styled` is a real devDependency of this monorepo (already used,
 * unstubbed, by other fixtures pulling in @mui/material and @chakra-ui/react),
 * so this re-export resolves to its actual installed type definitions —
 * unlike the zendesk-garden fixture, where `styled-components` isn't
 * installed at all and is left as an unresolved bare specifier.
 *
 * `isPropValid` is really re-exported from `@emotion/is-prop-valid`
 * (`export { default as isPropValid } from '@emotion/is-prop-valid';`), but
 * that package is only a transitive dependency here and isn't hoisted to
 * this monorepo's node_modules root, so a trivial type-compatible stand-in
 * is inlined instead — its actual logic (filtering DOM-unsafe prop names)
 * is irrelevant to prop extraction; only its use as a `shouldForwardProp`
 * value matters.
 */
export { default as styled } from '@emotion/styled';

export const isPropValid = (prop: string): boolean => !prop.startsWith('$');
