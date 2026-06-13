import * as React from "react";
import { MantineColor, MantineRadius, MantineSize, StylesApiProps } from "./Button";

/**
 * Mantine — @mantine/core TextInput
 *
 * TextInput composes two internal Mantine abstractions:
 *   1. `InputWrapperBaseProps` — manages label, description, error, and asterisk
 *   2. `InputBaseProps` — manages the visual input shell (sections, radius, etc.)
 *
 * Native `<input>` HTML attributes are passed through directly.
 *
 * Key patterns shown here:
 *   - `inputWrapperOrder` controls the DOM ordering of label/input/description/error
 *   - `leftSection` / `rightSection` render adornments inside the input shell
 *   - `leftSectionPointerEvents` / `rightSectionPointerEvents` control interaction
 *   - `withAsterisk` adds a required asterisk without setting `required` on the input
 *   - `wrapperProps` passes arbitrary props to the wrapping `<div>`
 *   - `styles`, `classNames`, `unstyled` from Styles API for deep customisation
 */

// ---------------------------------------------------------------------------
// Styles API slot names for TextInput
// ---------------------------------------------------------------------------

/** The slot names exposed by Mantine's TextInput / Input Styles API. */
export type TextInputStylesNames =
  | "root"
  | "wrapper"
  | "input"
  | "section"
  | "label"
  | "description"
  | "error"
  | "required";

// ---------------------------------------------------------------------------
// InputWrapperBaseProps
// ---------------------------------------------------------------------------

/**
 * Props provided by the InputWrapper that surrounds all Mantine form inputs.
 * Controls the label, description, error message, and required asterisk.
 */
export interface InputWrapperBaseProps {
  /**
   * Label displayed above the input. Renders a `<label>` element that is
   * associated with the input via `htmlFor`.
   */
  label?: React.ReactNode;
  /**
   * A short description rendered below the label and above the input.
   * Useful for hints or formatting guidance.
   */
  description?: React.ReactNode;
  /**
   * Error content displayed below the input. When a non-empty string or
   * ReactNode is provided the input switches to its error visual state.
   */
  error?: React.ReactNode;
  /**
   * If `true`, an asterisk is appended to the label to indicate the field
   * is required. Unlike setting `required`, this only affects the visual
   * display and does not set HTML validation attributes.
   * @default false
   */
  withAsterisk?: boolean;
  /**
   * Props forwarded to the `<label>` element rendered by the wrapper.
   */
  labelProps?: React.LabelHTMLAttributes<HTMLLabelElement> & {
    [key: string]: unknown;
  };
  /**
   * Props forwarded to the description element.
   */
  descriptionProps?: React.HTMLAttributes<HTMLParagraphElement> & {
    [key: string]: unknown;
  };
  /**
   * Props forwarded to the error element.
   */
  errorProps?: React.HTMLAttributes<HTMLParagraphElement> & {
    [key: string]: unknown;
  };
  /**
   * Controls the ordering of the label, input, description, and error
   * sections within the wrapper. Defaults to the MUI-like order:
   * label → input → description → error.
   * @default ['label', 'input', 'description', 'error']
   */
  inputWrapperOrder?: Array<"label" | "input" | "description" | "error">;
  /** The `id` of the input element; also used as the label's `htmlFor`. */
  id?: string;
}

// ---------------------------------------------------------------------------
// InputBaseProps
// ---------------------------------------------------------------------------

/**
 * Props provided by Mantine's Input primitive — the shared visual shell
 * used by TextInput, PasswordInput, NumberInput, Textarea, etc.
 */
export interface InputBaseProps {
  /**
   * Sets `cursor: pointer` style on the input. Useful when the input
   * opens a picker (DatePicker, Select, etc.).
   * @default false
   */
  pointer?: boolean;
  /**
   * Content rendered inside the input shell on the left side.
   * Typically an icon component.
   */
  leftSection?: React.ReactNode;
  /**
   * Content rendered inside the input shell on the right side.
   * Typically an icon, clear button, or unit label.
   */
  rightSection?: React.ReactNode;
  /**
   * Width of the left section. Used to set the appropriate padding-left on
   * the `<input>` so the text does not overlap the section.
   * @default 36
   */
  leftSectionWidth?: React.CSSProperties["width"];
  /**
   * Width of the right section. Used to set the appropriate padding-right on
   * the `<input>` so the text does not overlap the section.
   * @default 36
   */
  rightSectionWidth?: React.CSSProperties["width"];
  /**
   * Controls pointer-events on the left section container. Set to `'none'`
   * to let clicks pass through to the input.
   * @default 'none'
   */
  leftSectionPointerEvents?: React.CSSProperties["pointerEvents"];
  /**
   * Controls pointer-events on the right section container. Set to `'all'`
   * when the section contains an interactive element (e.g. a clear button).
   * @default 'none'
   */
  rightSectionPointerEvents?: React.CSSProperties["pointerEvents"];
  /**
   * Props forwarded to the left section `<div>` wrapper.
   */
  leftSectionProps?: React.HTMLAttributes<HTMLDivElement>;
  /**
   * Props forwarded to the right section `<div>` wrapper.
   */
  rightSectionProps?: React.HTMLAttributes<HTMLDivElement>;
  /**
   * Controls the height and font size of the input.
   * @default 'sm'
   */
  size?: MantineSize;
  /**
   * Key of `theme.radius` or a number (px) for the border radius.
   * @default theme.defaultRadius
   */
  radius?: MantineRadius;
  /**
   * Visual variant of the input.
   * @default 'default'
   */
  variant?: "default" | "filled" | "unstyled";
  /**
   * If `true`, the input is disabled.
   * @default false
   */
  disabled?: boolean;
  /**
   * Key of `theme.colors` or any CSS color. Applied to the focused border
   * and ring when the input has an error.
   */
  error?: React.ReactNode;
  /**
   * Props passed to the wrapper `<div>` element surrounding input and sections.
   */
  wrapperProps?: React.HTMLAttributes<HTMLDivElement>;
}

// ---------------------------------------------------------------------------
// TextInputProps
// ---------------------------------------------------------------------------

export interface TextInputProps
  extends InputWrapperBaseProps,
    InputBaseProps,
    StylesApiProps<TextInputStylesNames>,
    Omit<React.InputHTMLAttributes<HTMLInputElement>, "size" | "color"> {
  /**
   * The controlled value of the input.
   */
  value?: string;
  /**
   * The uncontrolled default value.
   */
  defaultValue?: string;
  /**
   * Callback fired when the value changes.
   */
  onChange?: React.ChangeEventHandler<HTMLInputElement>;
  /**
   * Callback fired when the input gains focus.
   */
  onFocus?: React.FocusEventHandler<HTMLInputElement>;
  /**
   * Callback fired when the input loses focus.
   */
  onBlur?: React.FocusEventHandler<HTMLInputElement>;
  /**
   * The HTML `type` attribute of the `<input>` element.
   * @default 'text'
   */
  type?: React.HTMLInputTypeAttribute;
  /**
   * Placeholder text displayed when the input is empty.
   */
  placeholder?: string;
  /**
   * If `true`, the `<input>` element is marked as required.
   * Unlike `withAsterisk`, this sets the native required attribute
   * and triggers HTML5 form validation.
   * @default false
   */
  required?: boolean;
  /**
   * If `true`, the input is read-only.
   * @default false
   */
  readOnly?: boolean;
  /**
   * Hint for the browser's autocomplete feature.
   * @see https://developer.mozilla.org/en-US/docs/Web/HTML/Attributes/autocomplete
   */
  autoComplete?: string;
  /**
   * If `true`, the browser will focus the input on mount.
   * @default false
   */
  autoFocus?: boolean;
  /**
   * The maximum number of characters the user can enter.
   */
  maxLength?: number;
  /**
   * The minimum number of characters required.
   */
  minLength?: number;
  /**
   * A regex pattern the value must match for HTML5 form validation.
   */
  pattern?: string;
  /**
   * The `name` attribute for use in a form. Associates the input with a form field.
   */
  name?: string;
  /**
   * Key of `theme.colors` or any CSS color used for the focused border.
   * @default theme.primaryColor
   */
  color?: MantineColor;
  /**
   * A ref passed to the underlying `<input>` DOM element.
   */
  ref?: React.Ref<HTMLInputElement>;
}

export declare const TextInput: React.ForwardRefExoticComponent<
  TextInputProps & React.RefAttributes<HTMLInputElement>
>;

export default TextInput;
