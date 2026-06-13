import * as React from "react";

/**
 * Mantine — @mantine/core Button
 *
 * Mantine buttons use a polymorphic `component` prop pattern that lets you
 * render any element/component as the button root while keeping full type
 * safety. Supporting types from @mantine/core are stubbed inline so this
 * file is self-contained.
 *
 * Key Mantine patterns shown here:
 *   - `MantineColor` is a string alias (supports theme color keys like
 *     `'blue'`, `'red.5'`, or arbitrary CSS colors)
 *   - `MantineSize` / `MantineRadius` are theme-token string | number types
 *   - `StylesApiProps` provides `classNames`, `styles`, and `unstyled` for
 *     deep component customisation via the Styles API
 *   - `gradient` is a structured object used when `variant='gradient'`
 *   - `loaderProps` forwards to the internal Loader component
 *   - `leftSection` / `rightSection` (Mantine v7 naming, replaces leftIcon/rightIcon)
 *   - `component` enables polymorphic rendering (e.g. as an `<a>` or Next.js Link)
 */

// ---------------------------------------------------------------------------
// Mantine theme token types (simplified stubs)
// ---------------------------------------------------------------------------

/**
 * A Mantine color token: either a key from `theme.colors` (e.g. `'blue'`),
 * a key with shade index (e.g. `'blue.5'`), or any valid CSS color string.
 */
export type MantineColor = string;

/**
 * A Mantine size token: one of the predefined T-shirt sizes or a custom string.
 */
export type MantineSize = "xs" | "sm" | "md" | "lg" | "xl";

/**
 * Border-radius token: one of the predefined size keys, a number (pixels),
 * or an arbitrary CSS string.
 */
export type MantineRadius = MantineSize | (string & {}) | number;

/**
 * Props for Mantine's internal Loader component.
 */
export interface LoaderProps {
  /** The size of the loader. */
  size?: MantineSize | number;
  /** The color of the loader. Defaults to the current Button color. */
  color?: MantineColor;
  /**
   * The loader variant / animation type.
   * @default 'oval'
   */
  type?: "bars" | "dots" | "oval";
}

// ---------------------------------------------------------------------------
// Styles API types
// ---------------------------------------------------------------------------

/**
 * CSS-in-JS styles record: keys are component slot names, values are style
 * objects or functions returning style objects (receiving the component theme).
 */
export type StylesRecord<Selectors extends string> = Partial<
  Record<Selectors, React.CSSProperties>
>;

/**
 * StylesApiProps enables deep customisation of Mantine components.
 * `classNames` / `styles` accept a record keyed by component slots.
 * Slot names for Button: `'root' | 'inner' | 'label' | 'section' | 'loader'`.
 */
export interface StylesApiProps<Selectors extends string = string> {
  /**
   * A map of slot names to CSS class names. Applied in addition to Mantine's
   * own generated classes, giving you a hook for per-slot styling.
   *
   * @example
   * classNames={{ root: 'my-btn', label: 'my-btn__label' }}
   */
  classNames?: Partial<Record<Selectors, string>>;
  /**
   * A map of slot names to inline style objects. Applied alongside Mantine's
   * generated styles so you can override specific slots without targeting a
   * class name.
   *
   * @example
   * styles={{ root: { borderRadius: 0 } }}
   */
  styles?: StylesRecord<Selectors>;
  /**
   * If `true`, all Mantine-generated styles are removed. Useful when you want
   * to apply a completely custom visual design.
   * @default false
   */
  unstyled?: boolean;
}

// ---------------------------------------------------------------------------
// ButtonProps
// ---------------------------------------------------------------------------

/** The slot names exposed by Mantine's Button Styles API. */
export type ButtonStylesNames =
  | "root"
  | "inner"
  | "label"
  | "section"
  | "loader";

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    StylesApiProps<ButtonStylesNames> {
  /** The content of the button. */
  children?: React.ReactNode;
  /**
   * Key of `theme.colors` or any valid CSS color.
   * Determines the button colour for built-in variants.
   * @default theme.primaryColor
   */
  color?: MantineColor;
  /**
   * The visual variant of the button.
   * - `'default'` — neutral outlined button
   * - `'filled'` — solid filled button (most prominent)
   * - `'light'` — lightly tinted background
   * - `'outline'` — transparent background with a coloured border
   * - `'subtle'` — no border or background, only coloured text
   * - `'transparent'` — fully transparent, no background or border
   * - `'white'` — white background regardless of colour scheme
   * - `'gradient'` — linear gradient; requires the `gradient` prop
   * @default 'filled'
   */
  variant?:
    | "default"
    | "filled"
    | "light"
    | "outline"
    | "subtle"
    | "transparent"
    | "white"
    | "gradient";
  /**
   * Controls the height and padding of the button.
   * @default 'sm'
   */
  size?: MantineSize | `compact-${MantineSize}`;
  /**
   * Key of `theme.radius` or a number (px) for the border radius.
   * @default theme.defaultRadius
   */
  radius?: MantineRadius;
  /**
   * Gradient configuration used when `variant='gradient'`.
   * @example { from: 'blue', to: 'cyan', deg: 90 }
   */
  gradient?: {
    /** The starting color. Accepts a Mantine color token or CSS color. */
    from: string;
    /** The ending color. Accepts a Mantine color token or CSS color. */
    to: string;
    /**
     * The gradient angle in degrees.
     * @default 45
     */
    deg?: number;
  };
  /**
   * If `true`, a loader is displayed inside the button and it is disabled.
   * @default false
   */
  loading?: boolean;
  /**
   * Props forwarded to the internal `Loader` component shown when
   * `loading` is `true`.
   */
  loaderProps?: LoaderProps;
  /**
   * Element rendered in the left section of the button (before the label).
   * Typically an icon component.
   */
  leftSection?: React.ReactNode;
  /**
   * Element rendered in the right section of the button (after the label).
   * Typically an icon component.
   */
  rightSection?: React.ReactNode;
  /**
   * If `true`, the button stretches to fill its container's full width.
   * @default false
   */
  fullWidth?: boolean;
  /**
   * Sets the CSS `justify-content` of the inner flex container.
   * Useful for controlling icon + label alignment when `fullWidth` is set.
   * @default 'center'
   */
  justify?: React.CSSProperties["justifyContent"];
  /**
   * Allows rendering the button as any HTML element or React component,
   * preserving all button props and styles.
   *
   * @example
   * // Render as a Next.js Link
   * <Button component={Link} href="/home">Go Home</Button>
   */
  component?: React.ElementType;
  /**
   * If `true`, the button is disabled: non-interactive and visually dimmed.
   * @default false
   */
  disabled?: boolean;
  /**
   * Determines which auto-complete feature the browser offers.
   */
  autoContrast?: boolean;
}

export declare const Button: React.ForwardRefExoticComponent<
  ButtonProps & React.RefAttributes<HTMLButtonElement>
>;

export default Button;
