import * as React from "react";

/**
 * MUI — @mui/material Button
 *
 * The MUI Button extends ButtonBase which extends React.ButtonHTMLAttributes.
 * Key MUI-isms demonstrated here:
 *   - `variant`, `color`, `size` as discriminated unions
 *   - `sx` prop for runtime style overrides via the MUI System
 *   - `disableElevation`, `disableFocusRipple`, `disableRipple` for ripple control
 *   - `startIcon` / `endIcon` for adornments
 *   - `href` for link-like button (renders <a>)
 *   - `component` for polymorphism
 *   - `loading`, `loadingIndicator`, `loadingPosition` for async actions (MUI v6+)
 */

// ---------------------------------------------------------------------------
// Supporting types (simplified stubs — real types live in @mui/system)
// ---------------------------------------------------------------------------

/** Opaque theme token. In real MUI this is a deeply nested object. */
export interface Theme {
  palette: Record<string, unknown>;
  spacing: (...args: number[]) => string;
  breakpoints: Record<string, unknown>;
  [key: string]: unknown;
}

/**
 * The `sx` prop accepts any CSS property as well as MUI System shorthands.
 * In real MUI this is a complex union; simplified here as a style-object.
 */
export type SxProps<T extends object = Theme> =
  | React.CSSProperties
  | ((theme: T) => React.CSSProperties)
  | ReadonlyArray<
      | boolean
      | React.CSSProperties
      | ((theme: T) => React.CSSProperties)
      | null
      | undefined
    >;

// ---------------------------------------------------------------------------
// ButtonBase
// ---------------------------------------------------------------------------

export interface ButtonBaseProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /** The component used for the root node. Enables polymorphic rendering. */
  component?: React.ElementType;
  /** If `true`, the component is disabled. */
  disabled?: boolean;
  /**
   * If `true`, the ripple effect is disabled globally on this component.
   * @default false
   */
  disableRipple?: boolean;
  /**
   * If `true`, the touch ripple effect is disabled.
   * @default false
   */
  disableTouchRipple?: boolean;
  /**
   * If `true`, the base button will have a keyboard focus ripple.
   * @default false
   */
  focusRipple?: boolean;
  /**
   * This prop can help identify which element has keyboard focus.
   * The class name will be applied when the element gains the focus through the keyboard interaction.
   */
  focusVisibleClassName?: string;
  /** Callback fired when the component is focused with a keyboard. */
  onFocusVisible?: React.FocusEventHandler<HTMLButtonElement>;
  /**
   * The system prop that allows defining system overrides as well as additional CSS styles.
   * @see https://mui.com/system/getting-started/the-sx-prop/
   */
  sx?: SxProps<Theme>;
  /**
   * @default 0
   */
  tabIndex?: NonNullable<React.HTMLAttributes<HTMLElement>["tabIndex"]>;
  /** Props applied to the `TouchRipple` element. */
  TouchRippleProps?: object;
  /** A ref that points to the `TouchRipple` element. */
  touchRippleRef?: React.Ref<unknown>;
}

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

export interface ButtonProps extends ButtonBaseProps {
  /** The content of the component. */
  children?: React.ReactNode;
  /**
   * Override or extend the styles applied to the component.
   * @see https://mui.com/api/button/#css
   */
  classes?: Partial<Record<string, string>>;
  /**
   * The color of the component.
   * It supports both default and custom theme colors, which can be added as shown in the
   * [palette customization guide](https://mui.com/material-ui/customization/palette/#custom-colors).
   * @default 'primary'
   */
  color?:
    | "inherit"
    | "primary"
    | "secondary"
    | "success"
    | "error"
    | "info"
    | "warning";
  /**
   * If `true`, no elevation is used.
   * @default false
   */
  disableElevation?: boolean;
  /**
   * If `true`, the keyboard focus ripple is disabled.
   * @default false
   */
  disableFocusRipple?: boolean;
  /**
   * Element placed after the children.
   * Typically an icon component, e.g. `<ArrowForwardIcon />`.
   */
  endIcon?: React.ReactNode;
  /**
   * If `true`, the button will take up the full width of its container.
   * @default false
   */
  fullWidth?: boolean;
  /**
   * The URL to link to when the button is clicked.
   * If defined, an `<a>` element will be used as the root node.
   */
  href?: string;
  /**
   * If `true`, the loading indicator is visible and the button is disabled.
   * @default false
   */
  loading?: boolean;
  /**
   * Element placed before the children if the button is in loading state.
   * The node should contain an element with `role="progressbar"` with an accessible name.
   * By default, the children position is center.
   * @default <CircularProgress color="inherit" size={16} />
   */
  loadingIndicator?: React.ReactNode;
  /**
   * The loading indicator can be positioned on the start, end, or the center of the button.
   * @default 'center'
   */
  loadingPosition?: "start" | "end" | "center";
  /**
   * The size of the component. `small` is equivalent to the dense button styling.
   * @default 'medium'
   */
  size?: "small" | "medium" | "large";
  /**
   * Element placed before the children.
   * Typically an icon component, e.g. `<BookmarkIcon />`.
   */
  startIcon?: React.ReactNode;
  /**
   * The system prop that allows defining system overrides as well as additional CSS styles.
   * @see https://mui.com/system/getting-started/the-sx-prop/
   */
  sx?: SxProps<Theme>;
  /**
   * The variant to use.
   * @default 'text'
   */
  variant?: "text" | "outlined" | "contained";
}

declare const Button: React.ForwardRefExoticComponent<
  ButtonProps & React.RefAttributes<HTMLButtonElement>
>;

export default Button;
