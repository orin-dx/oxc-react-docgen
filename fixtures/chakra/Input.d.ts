import * as React from "react";
import { HTMLChakraProps, ThemingProps } from "./Button";

/**
 * Chakra UI — @chakra-ui/input Input
 *
 * Chakra's Input wraps the native `<input>` element with the Chakra style
 * system and adds accessibility-first boolean state props.
 *
 * Key patterns shown here:
 *   - `HTMLChakraProps<'input'>` provides all native input attributes plus
 *     Chakra shorthand style props (px, py, bg, rounded, etc.)
 *   - `ThemingProps<'Input'>` provides `variant`, `colorScheme`, `size`
 *     as theme-token strings
 *   - `isDisabled`, `isReadOnly`, `isRequired`, `isInvalid` follow Chakra
 *     boolean prop naming (mirrors WAI-ARIA state semantics)
 *   - `focusBorderColor` / `errorBorderColor` accept CSS color strings or
 *     Chakra theme color tokens (e.g. `'blue.500'`)
 */

// ---------------------------------------------------------------------------
// InputProps
// ---------------------------------------------------------------------------

export interface InputProps
  extends HTMLChakraProps<"input">,
    ThemingProps<"Input"> {
  /**
   * The border color when the input is focused. Use a Chakra color token
   * (e.g. `'blue.400'`) or any valid CSS color.
   */
  focusBorderColor?: string;
  /**
   * The border color when `isInvalid` is `true`. Use a Chakra color token
   * (e.g. `'red.500'`) or any valid CSS color.
   */
  errorBorderColor?: string;
  /**
   * If `true`, the form control will be disabled. Passed to the underlying
   * `<input>` as the `disabled` attribute and applies the disabled style.
   * @default false
   */
  isDisabled?: boolean;
  /**
   * If `true`, the input is marked read-only. Passed as `readOnly` to the
   * underlying `<input>` and styles the field accordingly.
   * @default false
   */
  isReadOnly?: boolean;
  /**
   * If `true`, the form control will be required.
   * Passed as `required` and adds an aria-required attribute.
   * @default false
   */
  isRequired?: boolean;
  /**
   * If `true`, the input will indicate an error state via aria-invalid and
   * the `errorBorderColor`.
   * @default false
   */
  isInvalid?: boolean;
  /**
   * If `true`, the input element will span the full width of its parent.
   * @default false
   */
  isFullWidth?: boolean;
  /**
   * The visual variant of the input as defined in the Input component theme.
   * Built-in values: `'outline'`, `'filled'`, `'flushed'`, `'unstyled'`.
   */
  variant?: string;
  /**
   * The size of the input. Resolves from the Input component theme.
   * Built-in values: `'xs'`, `'sm'`, `'md'`, `'lg'`.
   */
  size?: string;
  /**
   * Maps to a key in `theme.colors`. Controls the colour family applied to
   * focus rings and interactive states.
   */
  colorScheme?: string;
  /**
   * The HTML `type` attribute for the underlying `<input>` element.
   * @default 'text'
   */
  type?: React.HTMLInputTypeAttribute;
  /**
   * The placeholder text shown before the user enters a value.
   */
  placeholder?: string;
  /**
   * The controlled value of the input.
   */
  value?: string | ReadonlyArray<string> | number | undefined;
  /**
   * The uncontrolled default value.
   */
  defaultValue?: string | number | ReadonlyArray<string> | undefined;
  /** Callback fired when the input value changes. */
  onChange?: React.ChangeEventHandler<HTMLInputElement>;
  /** Callback fired when the input gains focus. */
  onFocus?: React.FocusEventHandler<HTMLInputElement>;
  /** Callback fired when the input loses focus. */
  onBlur?: React.FocusEventHandler<HTMLInputElement>;
  /**
   * The HTML `name` attribute. Used with forms.
   */
  name?: string;
  /**
   * The HTML `id` attribute. Should match the `htmlFor` of an associated label.
   */
  id?: string;
  /**
   * The HTML autocomplete attribute.
   */
  autoComplete?: string;
  /**
   * If `true`, the input element is focused on the initial render.
   * @default false
   */
  autoFocus?: boolean;
  /**
   * The maximum number of characters allowed.
   */
  maxLength?: number;
  /**
   * The minimum number of characters required.
   */
  minLength?: number;
  /**
   * A regular expression that the value must match for form validation.
   */
  pattern?: string;
}

export declare const Input: React.ForwardRefExoticComponent<
  InputProps & React.RefAttributes<HTMLInputElement>
>;

// ---------------------------------------------------------------------------
// InputGroup — composes Input with left/right elements
// ---------------------------------------------------------------------------

export interface InputGroupProps extends HTMLChakraProps<"div"> {
  /** The size to propagate to all child Input components. */
  size?: string;
  /** The variant to propagate to all child Input components. */
  variant?: string;
}

export declare const InputGroup: React.ForwardRefExoticComponent<
  InputGroupProps & React.RefAttributes<HTMLDivElement>
>;

// ---------------------------------------------------------------------------
// InputAdornment elements
// ---------------------------------------------------------------------------

export interface InputAddonProps extends HTMLChakraProps<"div"> {}

/** Element placed to the left of the Input inside an InputGroup. */
export declare const InputLeftAddon: React.ForwardRefExoticComponent<
  InputAddonProps & React.RefAttributes<HTMLDivElement>
>;

/** Element placed to the right of the Input inside an InputGroup. */
export declare const InputRightAddon: React.ForwardRefExoticComponent<
  InputAddonProps & React.RefAttributes<HTMLDivElement>
>;

export interface InputElementProps extends HTMLChakraProps<"div"> {
  /** The size of the input element (inherits from InputGroup). */
  size?: string;
}

/** Icon or interactive element positioned inside the left side of the Input. */
export declare const InputLeftElement: React.ForwardRefExoticComponent<
  InputElementProps & React.RefAttributes<HTMLDivElement>
>;

/** Icon or interactive element positioned inside the right side of the Input. */
export declare const InputRightElement: React.ForwardRefExoticComponent<
  InputElementProps & React.RefAttributes<HTMLDivElement>
>;
