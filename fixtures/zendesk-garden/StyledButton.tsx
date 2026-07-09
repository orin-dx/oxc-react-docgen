import { SELECTOR_FOCUS_VISIBLE, componentStyles } from '@zendeskgarden/react-theming';
import { em } from 'polished';
import { ButtonHTMLAttributes } from 'react';
import styled, { css, DefaultTheme, ThemeProps } from 'styled-components';

/**
 * Zendesk Garden — @zendeskgarden/react-buttons StyledButton (simplified fixture)
 *
 * Adapted from the real source, Apache-2.0 licensed:
 * https://github.com/zendeskgarden/react-components/blob/main/.packages/buttons/src/styled/StyledButton.ts
 * (companion type: .packages/buttons/src/types/index.ts)
 *
 * Zendesk Garden builds its styled-components primitives via
 * `styled.button.attrs<T>(props => ({...}))<T>\`...\`` — a generic type
 * argument applied to `.attrs()`, and again to the tagged-template call
 * itself. `IStyledButtonProps` extends `ButtonHTMLAttributes<HTMLButtonElement>`
 * and adds transient ($-prefixed) style props; every style function below
 * receives `IStyledButtonProps & ThemeProps<DefaultTheme>`. This is a
 * different generic shape than the CVA/tailwind-variants patterns already
 * covered by the shadcn/mantine fixtures — the props type is threaded
 * through `styled`'s own generics rather than through a `cva()`/`tv()` call.
 *
 * Trimmed/stubbed for this fixture:
 *   - CSS-in-JS bodies (colorStyles/groupStyles/iconStyles/sizeStyles) are
 *     collapsed to trivial stand-ins — the real declarations are pure theme
 *     lookups + CSS strings, irrelevant to prop extraction.
 *   - `IButtonProps` (real shape, from the sibling `.packages/buttons/src/types/index.ts`
 *     in the upstream package) is inlined below instead of imported via a
 *     relative path, so this fixture is a single file.
 *   - `StyledIcon` / `StyledSplitButton` (sibling styled components used only
 *     as nested-selector interpolation targets inside the CSS) are stubbed
 *     as trivial `styled.svg` / `styled.div` calls.
 *   - `@zendeskgarden/react-theming` and `polished` imports are kept as real,
 *     unresolved specifiers (consistent with this repo's panda/button.tsx
 *     fixture, which does the same for `@pandacss/dev`) — they're only used
 *     inside style-function bodies, never in a type position that prop
 *     extraction needs to resolve. `styled-components` itself is not
 *     installed in this repo either; left unresolved for the same reason.
 *   - `PACKAGE_VERSION` is a webpack `DefinePlugin` global in the real build;
 *     stubbed here with a `declare const`.
 */

declare const PACKAGE_VERSION: string;

export const SIZE = ['small', 'medium', 'large'] as const;

/** Inlined from the sibling `.packages/buttons/src/types/index.ts` in the real package. */
export interface IButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Applies danger styling */
  isDanger?: boolean;
  /** Specifies the button size */
  size?: (typeof SIZE)[number];
  /** Stretches the button fill to its container width */
  isStretched?: boolean;
  /** Applies neutral button styling */
  isNeutral?: boolean;
  /** Applies primary button styling */
  isPrimary?: boolean;
  /** Applies basic button styling */
  isBasic?: boolean;
  /** Applies link (anchor) button styling */
  isLink?: boolean;
  /** Applies pill button styling */
  isPill?: boolean;
  /** Applies inset `box-shadow` styling on focus */
  focusInset?: boolean;
}

export const COMPONENT_ID = 'buttons.button';

export interface IStyledButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  $isUnderlined?: boolean;
  $isDanger?: boolean;
  $size?: IButtonProps['size'];
  $isStretched?: boolean;
  $isNeutral?: boolean;
  $isPrimary?: boolean;
  $isBasic?: boolean;
  $isLink?: boolean;
  $isPill?: boolean;
  $focusInset?: boolean;
}

// Sibling styled components, referenced only as nested-selector interpolation
// targets in the CSS below; their own prop shapes aren't under test here.
const StyledIcon = styled.svg``;
const StyledSplitButton = styled.div``;

const getBorderRadius = (props: IStyledButtonProps & ThemeProps<DefaultTheme>) => {
  if (props.$isPill) {
    return '100px';
  }

  return props.theme.borderRadii.md;
};

export const getHeight = (props: IStyledButtonProps & ThemeProps<DefaultTheme>) => {
  if (props.$size === 'small') {
    return `${props.theme.space.base * 8}px`;
  } else if (props.$size === 'large') {
    return `${props.theme.space.base * 12}px`;
  }

  return `${props.theme.space.base * 10}px`;
};

// Trimmed: the real implementation branches on $isLink/$isPrimary/$isDanger/
// $isNeutral/$isBasic/$focusInset to compute themed border/background/
// foreground colors (~230 lines). Collapsed to a stub — not relevant to prop
// extraction.
const colorStyles = (props: IStyledButtonProps & ThemeProps<DefaultTheme>) => css`
  color: inherit;
`;

// Trimmed: the real implementation merges adjacent button borders/z-index
// for button-group layout and handles RTL. Collapsed to a stub.
const groupStyles = (props: IStyledButtonProps & ThemeProps<DefaultTheme>) => css`
  position: relative;
`;

const iconStyles = (props: IStyledButtonProps & ThemeProps<DefaultTheme>) => {
  const $size = props.$size === 'small' ? props.theme.iconSizes.sm : props.theme.iconSizes.md;

  return css`
    width: ${$size};
    height: ${$size};
  `;
};

const sizeStyles = (props: IStyledButtonProps & ThemeProps<DefaultTheme>) => {
  if (props.$isLink) {
    return css`
      padding: 0;
    `;
  }

  return css`
    height: ${getHeight(props)};
    font-size: ${em(props.theme.fontSizes.md, props.theme.fontSizes.md)};
  `;
};

/*
 * 1. <a> element reset
 * 2. FF <input type="submit"> fix
 * 3. Shifting :focus-visible from LVHFA order to preserve `text-decoration` on hover
 */
export const StyledButton = styled.button.attrs<IStyledButtonProps>(props => ({
  'data-garden-id': (props as any)['data-garden-id'] || COMPONENT_ID,
  'data-garden-version': PACKAGE_VERSION,
  type: props.type || 'button'
}))<IStyledButtonProps>`
  display: ${props => (props.$isLink ? 'inline' : 'inline-flex')};
  align-items: ${props => !props.$isLink && 'center'};
  justify-content: ${props => !props.$isLink && 'center'};
  margin: 0;
  border: ${props => `${props.$isLink ? `0px solid` : props.theme.borders.sm} transparent`};
  border-radius: ${props => getBorderRadius(props)};
  cursor: pointer;
  width: ${props => (props.$isStretched ? '100%' : '')};
  overflow: hidden;
  text-decoration: ${props => (props.$isUnderlined ? 'underline' : 'none')}; /* [1] */

  ${props => sizeStyles(props)};

  &::-moz-focus-inner {
    /* [2] */
    border: 0;
    padding: 0;
  }

  /* [3] */
  ${SELECTOR_FOCUS_VISIBLE} {
    text-decoration: none;
  }

  ${props => colorStyles(props)};

  & ${StyledIcon} {
    ${props => iconStyles(props)}
  }

  ${StyledSplitButton} && {
    ${props => groupStyles(props)}
  }

  ${componentStyles}
`;
