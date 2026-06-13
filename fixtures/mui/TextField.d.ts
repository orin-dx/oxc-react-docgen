import * as React from "react";
import { SxProps, Theme } from "./Button";

/**
 * MUI — @mui/material TextField
 *
 * TextField is a convenience wrapper around several MUI form primitives:
 * FormControl, InputLabel, Input/FilledInput/OutlinedInput, and FormHelperText.
 * It supports three `variant` modes, each with their own props type so that
 * the TypeScript types accurately describe which props are valid per variant.
 *
 * Key MUI TextField patterns:
 *   - Discriminated union on `variant` (standard | filled | outlined)
 *   - `InputProps` for the underlying Input component props
 *   - `inputProps` (lowercase) for the native `<input>` HTML attributes
 *   - `InputLabelProps` / `FormHelperTextProps` for sub-component customisation
 *   - `select` to render a Select instead of an Input
 *   - `multiline` + `rows` / `maxRows` for textarea mode
 *   - `sx` for the MUI System style override
 */

// ---------------------------------------------------------------------------
// Supporting sub-component prop types
// ---------------------------------------------------------------------------

/** Props forwarded to the inner InputLabel component. */
export interface InputLabelProps
  extends React.LabelHTMLAttributes<HTMLLabelElement> {
  /** If `true`, the label is displayed in an error state. */
  error?: boolean;
  /** If `true`, the label is displayed as required (with an asterisk). */
  required?: boolean;
  /** If `true`, the label is shrunk above the input. */
  shrink?: boolean;
  /** The variant of the input label. */
  variant?: "standard" | "filled" | "outlined";
  /** The system prop for styling overrides. */
  sx?: SxProps<Theme>;
}

/** Props forwarded to the FormHelperText component displayed below the field. */
export interface FormHelperTextProps
  extends React.HTMLAttributes<HTMLParagraphElement> {
  /** If `true`, the helper text is displayed in an error state. */
  error?: boolean;
  /** If `true`, the helper text is visible even when not focused. */
  focused?: boolean;
  /** If `true`, the helper text is required (rarely used directly). */
  required?: boolean;
  /** The variant of the form helper text. */
  variant?: "standard" | "filled" | "outlined";
  /** The system prop for styling overrides. */
  sx?: SxProps<Theme>;
}

/** Props forwarded to the underlying Input/FilledInput/OutlinedInput component. */
export interface BaseInputProps {
  /** The id of the `input` element. */
  id?: string;
  /** Callback fired when the value is changed. */
  onChange?: React.ChangeEventHandler<HTMLInputElement | HTMLTextAreaElement>;
  /** If `true`, the component is disabled. */
  disabled?: boolean;
  /** If `true`, the input will indicate an error. */
  error?: boolean;
  /** If `true`, the input will take up the full width of its container. */
  fullWidth?: boolean;
  /** If `true`, a `textarea` element will be rendered. */
  multiline?: boolean;
  /** The system prop for styling overrides. */
  sx?: SxProps<Theme>;
  /** Start `InputAdornment` for this component. */
  startAdornment?: React.ReactNode;
  /** End `InputAdornment` for this component. */
  endAdornment?: React.ReactNode;
  /** The short hint displayed in the `input` before the user enters a value. */
  placeholder?: string;
  /** Pass a ref to the `input` element. */
  inputRef?: React.Ref<HTMLInputElement>;
}

// ---------------------------------------------------------------------------
// Variant-specific TextField props types (discriminated union)
// ---------------------------------------------------------------------------

interface TextFieldPropsBase {
  /** Override or extend the styles applied to the component. */
  classes?: Partial<Record<string, string>>;
  /**
   * The color of the component. Supports default and custom theme colors.
   * @default 'primary'
   */
  color?: "primary" | "secondary" | "error" | "info" | "success" | "warning";
  /** The default value. Use when the component is not controlled. */
  defaultValue?: unknown;
  /** If `true`, the component is disabled. */
  disabled?: boolean;
  /**
   * If `true`, the label is displayed in an error state.
   * @default false
   */
  error?: boolean;
  /**
   * Props applied to the `FormHelperText` element.
   */
  FormHelperTextProps?: Partial<FormHelperTextProps>;
  /**
   * If `true`, the input will take up the full width of its container.
   * @default false
   */
  fullWidth?: boolean;
  /**
   * The helper text content. Rendered below the input using FormHelperText.
   */
  helperText?: React.ReactNode;
  /**
   * The id of the `input` element. Use this prop to make `label` and
   * `helperText` accessible for screen readers.
   */
  id?: string;
  /**
   * Props applied to the `InputLabel` element. Pointer events on the label
   * are disabled by default.
   */
  InputLabelProps?: Partial<InputLabelProps>;
  /**
   * [Attributes](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/input#Attributes)
   * applied to the `input` element.
   */
  inputProps?: React.InputHTMLAttributes<HTMLInputElement>;
  /**
   * Props applied to the Input element. It will be a `FilledInput`,
   * `OutlinedInput`, or `Input` component depending on the `variant` prop value.
   */
  InputProps?: Partial<BaseInputProps>;
  /** Pass a ref to the `input` element. */
  inputRef?: React.Ref<HTMLInputElement>;
  /**
   * The label content.
   */
  label?: React.ReactNode;
  /**
   * If `dense` or `normal`, will adjust vertical spacing of this and
   * contained components.
   * @default 'none'
   */
  margin?: "dense" | "normal" | "none";
  /**
   * Maximum number of rows to display when multiline option is set to true.
   */
  maxRows?: number | string;
  /**
   * If `true`, a `textarea` element will be rendered instead of an `input`.
   * @default false
   */
  multiline?: boolean;
  /**
   * Name attribute of the `input` element.
   */
  name?: string;
  /** Callback fired when the value is changed. */
  onChange?: React.ChangeEventHandler<HTMLInputElement | HTMLTextAreaElement>;
  /** Callback fired when the input loses focus. */
  onBlur?: React.FocusEventHandler<HTMLInputElement | HTMLTextAreaElement>;
  /** Callback fired when the input gains focus. */
  onFocus?: React.FocusEventHandler<HTMLInputElement | HTMLTextAreaElement>;
  /**
   * The short hint displayed in the `input` before the user enters a value.
   */
  placeholder?: string;
  /**
   * If `true`, the label is displayed as required and the `input` element is
   * required.
   * @default false
   */
  required?: boolean;
  /**
   * Number of rows to display when multiline option is set to true.
   */
  rows?: number | string;
  /**
   * Render a [`Select`](https://mui.com/material-ui/api/select/) element while
   * passing the Input element to `Select` as `input` parameter.
   * Prompts the user to supply a `children` node.
   * @default false
   */
  select?: boolean;
  /**
   * Props applied to the `Select` element.
   */
  SelectProps?: object;
  /**
   * The size of the component.
   * @default 'medium'
   */
  size?: "small" | "medium";
  /**
   * The system prop that allows defining system overrides as well as
   * additional CSS styles.
   */
  sx?: SxProps<Theme>;
  /**
   * Type of the `input` element. It should be
   * [a valid HTML5 input type](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/input#Form_%3Cinput%3E_types).
   */
  type?: React.HTMLInputTypeAttribute;
  /** The value of the `input` element, required for a controlled component. */
  value?: unknown;
  /** Hint text shown in the input when `autoComplete` is configured. */
  autoComplete?: string;
  /**
   * If `true`, the `input` element is focused during the first mount.
   * @default false
   */
  autoFocus?: boolean;
}

export interface StandardTextFieldProps extends TextFieldPropsBase {
  /**
   * The variant to use.
   * @default 'outlined'
   */
  variant: "standard";
  InputProps?: Partial<BaseInputProps>;
}

export interface FilledTextFieldProps extends TextFieldPropsBase {
  variant: "filled";
  InputProps?: Partial<BaseInputProps>;
}

export interface OutlinedTextFieldProps extends TextFieldPropsBase {
  variant: "outlined";
  InputProps?: Partial<BaseInputProps>;
}

export type TextFieldProps =
  | StandardTextFieldProps
  | FilledTextFieldProps
  | OutlinedTextFieldProps;

declare const TextField: React.ForwardRefExoticComponent<
  TextFieldProps & React.RefAttributes<HTMLDivElement>
>;

export default TextField;
