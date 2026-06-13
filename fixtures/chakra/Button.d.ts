import * as React from "react";

/**
 * Chakra UI — @chakra-ui/button Button
 *
 * Chakra UI components wrap native HTML elements with Chakra's style system.
 * Key Chakra patterns demonstrated here:
 *   - `HTMLChakraProps<'button'>` merges native button attributes with Chakra's
 *     style prop system (shorthand props like `px`, `color`, `bg`, etc.)
 *   - `ThemingProps<'Button'>` provides `variant`, `colorScheme`, `size`
 *     resolved from the theme — typed as plain `string` not literals
 *   - `isLoading`, `isDisabled`, `isActive` follow the Chakra naming convention
 *   - `leftIcon` / `rightIcon` accept ReactElement (not just ReactNode)
 *   - `as` prop enables polymorphic rendering via Chakra's `As` helper
 */

// ---------------------------------------------------------------------------
// Supporting types (simplified stubs of Chakra's internal types)
// ---------------------------------------------------------------------------

/**
 * Chakra's style props — a superset of React.CSSProperties extended with
 * shorthand aliases (e.g. `px` → `paddingLeft` + `paddingRight`) and
 * responsive arrays/objects. Simplified here as a style-object intersection.
 */
export type ChakraStyleProps = React.CSSProperties & {
  /** Shorthand for `padding-left` and `padding-right`. */
  px?: React.CSSProperties["paddingLeft"];
  /** Shorthand for `padding-top` and `padding-bottom`. */
  py?: React.CSSProperties["paddingTop"];
  /** Shorthand for `margin-left` and `margin-right`. */
  mx?: React.CSSProperties["marginLeft"];
  /** Shorthand for `margin-top` and `margin-bottom`. */
  my?: React.CSSProperties["marginTop"];
  /** Shorthand for `background-color`. */
  bg?: React.CSSProperties["backgroundColor"];
  /** Shorthand for `border-radius`. */
  rounded?: React.CSSProperties["borderRadius"];
  [key: string]: unknown;
};

/**
 * HTMLChakraProps<Tag> merges the native HTML attributes for `Tag` with
 * Chakra's style prop system. The `as` prop overrides the rendered element.
 */
export type HTMLChakraProps<Tag extends React.ElementType> =
  Omit<React.ComponentPropsWithoutRef<Tag>, "color"> &
    ChakraStyleProps & {
      /**
       * The element or component to render as.
       * Enables polymorphic rendering (e.g. render a button as an anchor).
       */
      as?: React.ElementType;
    };

/**
 * ThemingProps resolves the `variant`, `colorScheme`, and `size` keys
 * from the component's theme definition. Because they are theme tokens
 * they are typed as `string` — not string literals — to allow custom themes.
 */
export interface ThemingProps<ThemeComponent extends string = string> {
  /**
   * The visual variant of the component as defined in the component theme.
   * Common built-in values: `'solid'`, `'outline'`, `'ghost'`, `'link'`.
   */
  variant?: string;
  /**
   * The color scheme to use, maps to a key in `theme.colors`.
   * Common values: `'blue'`, `'green'`, `'red'`, `'gray'`, `'teal'`, etc.
   */
  colorScheme?: string;
  /**
   * The size of the component as defined in the component theme.
   * Common values: `'xs'`, `'sm'`, `'md'`, `'lg'`.
   */
  size?: string;
  /** @internal Used for type inference on the theme component key. */
  styleConfig?: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// ButtonSpinner
// ---------------------------------------------------------------------------

export interface ButtonSpinnerProps {
  /** Custom spinner element. */
  children?: React.ReactNode;
  className?: string;
  /** The label shown alongside the spinner when `loadingText` is set. */
  label?: React.ReactNode;
  /**
   * Whether the spinner appears before or after the loading text.
   * @default 'start'
   */
  placement?: "start" | "end";
  /** @deprecated use `placement` instead */
  hasLoadingText?: boolean;
}

// ---------------------------------------------------------------------------
// ButtonProps
// ---------------------------------------------------------------------------

export interface ButtonProps
  extends HTMLChakraProps<"button">,
    ThemingProps<"Button"> {
  /** The content to render inside the button. */
  children?: React.ReactNode;
  /**
   * If `true`, the button will show a spinner and be disabled.
   * @default false
   */
  isLoading?: boolean;
  /**
   * If `true`, the button will be styled in its active state.
   * @default false
   */
  isActive?: boolean;
  /**
   * If `true`, the button will be disabled.
   * @default false
   */
  isDisabled?: boolean;
  /**
   * If `true`, the button will take up the full width of its container.
   * @default false
   * @deprecated Use `width="full"` or `width="100%"` style prop instead.
   */
  isFullWidth?: boolean;
  /**
   * The label to show when `isLoading` is `true`. When provided the spinner is
   * placed before this text using the `spinnerPlacement` side.
   */
  loadingText?: string;
  /**
   * If added, the button will show an icon before the button's label.
   * Receives a `ReactElement` so the component can clone it and inject size.
   */
  leftIcon?: React.ReactElement;
  /**
   * If added, the button will show an icon after the button's label.
   * Receives a `ReactElement` so the component can clone it and inject size.
   */
  rightIcon?: React.ReactElement;
  /**
   * The space between the button icon and label.
   * Accepts any valid CSS spacing token.
   * @default '0.5rem'
   */
  iconSpacing?: React.CSSProperties["marginLeft"];
  /**
   * Replace the spinner component when `isLoading` is `true`.
   */
  spinner?: React.ReactElement;
  /**
   * Determines where to place the spinner when `isLoading` is `true`
   * and `loadingText` is set.
   * @default 'start'
   */
  spinnerPlacement?: "start" | "end";
  /** The element or component to render as. Enables polymorphic usage. */
  as?: React.ElementType;
  /**
   * The type of the button, used in form contexts.
   * @default 'button'
   */
  type?: "button" | "submit" | "reset";
}

export declare const Button: React.ForwardRefExoticComponent<
  ButtonProps & React.RefAttributes<HTMLButtonElement>
>;

export declare const ButtonGroup: React.ForwardRefExoticComponent<
  HTMLChakraProps<"div"> &
    ThemingProps<"Button"> & {
      /** If `true`, the borderRadius of button that are direct children will be altered to look flushed. */
      isAttached?: boolean;
      /** If `true`, all wrapped button will be disabled. */
      isDisabled?: boolean;
      /** The spacing between the buttons. */
      spacing?: React.CSSProperties["marginLeft"];
    } & React.RefAttributes<HTMLDivElement>
>;
