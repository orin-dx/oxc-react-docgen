/**
 * final-form/react-final-form — src/types.ts (trimmed to Field's dependency graph)
 *
 * Adapted from final-form/react-final-form (MIT license), the real source file:
 * https://github.com/final-form/react-final-form/blob/main/src/types.ts
 *
 * Kept verbatim: `FieldInputProps`, `FieldRenderProps`, `RenderableProps`,
 * `UseFieldAutoConfig`, `UseFieldConfig`, and `FieldProps` — the full type
 * chain ./Field.tsx's props resolve through, including the real
 *   children?: ((props: T) => React.ReactNode) | React.ReactNode
 * union on `RenderableProps` that Field.tsx narrows with
 * `typeof children === "function"`, and `FieldProps`'s real
 * `[key: string]: any` index signature (it spreads arbitrary DOM/component
 * props through as `...rest`, which Field.tsx merges back into the render
 * function's argument).
 *
 * Dropped (not reachable from Field's props): `ReactContext`,
 * `FormRenderProps`, `FormSpyRenderProps`, `FormProps`, `FormSpyProps`,
 * `SubmitEvent`, `FieldMetaState`.
 *
 * `FieldSubscription` and `FieldValidator` are normally imported from the
 * `final-form` core package (not installed in this repo, and this fixture
 * only exercises the react-final-form layer). Inlined below verbatim from
 * final-form/final-form's src/types.ts (MIT license):
 *   https://github.com/final-form/final-form/blob/master/src/types.ts
 * `FieldValidator`'s `meta` parameter is simplified from the real
 * `FieldState<FieldValue>` to `Record<string, any>` since `FieldState` isn't
 * otherwise part of Field's props surface.
 */
import * as React from "react";

type SupportedInputs = "input" | "select" | "textarea";

export interface FieldSubscription {
  active?: boolean;
  data?: boolean;
  dirty?: boolean;
  dirtySinceLastSubmit?: boolean;
  error?: boolean;
  initial?: boolean;
  invalid?: boolean;
  length?: boolean;
  modified?: boolean;
  modifiedSinceLastSubmit?: boolean;
  pristine?: boolean;
  submitError?: boolean;
  submitFailed?: boolean;
  submitSucceeded?: boolean;
  submitting?: boolean;
  touched?: boolean;
  valid?: boolean;
  validating?: boolean;
  value?: boolean;
  visited?: boolean;
}

export type FieldValidator<FieldValue = any> = (
  value: FieldValue,
  allValues: object,
  meta?: Record<string, any>,
) => any | Promise<any>;

export interface FieldInputProps<FieldValue = any, T = any> {
  name: string;
  onBlur: (event?: React.FocusEvent<T>) => void;
  onChange: (event: React.ChangeEvent<T> | any) => void;
  onFocus: (event?: React.FocusEvent<T>) => void;
  value: FieldValue;
  checked?: boolean;
  multiple?: boolean;
  type?: string;
}

export interface FieldRenderProps<
  FieldValue = any,
  T = any,
  _FormValues = any,
> {
  input: FieldInputProps<FieldValue, T>;
  meta: {
    active?: boolean;
    data?: Record<string, any>;
    dirty?: boolean;
    dirtySinceLastSubmit?: boolean;
    error?: any;
    initial?: any;
    invalid?: boolean;
    length?: number;
    modified?: boolean;
    modifiedSinceLastSubmit?: boolean;
    pristine?: boolean;
    submitError?: any;
    submitFailed?: boolean;
    submitSucceeded?: boolean;
    submitting?: boolean;
    touched?: boolean;
    valid?: boolean;
    validating?: boolean;
    visited?: boolean;
  };
}

export interface RenderableProps<T> {
  component?: React.ComponentType<any> | SupportedInputs;
  children?: ((props: T) => React.ReactNode) | React.ReactNode;
  render?: (props: T) => React.ReactNode;
}

export interface UseFieldAutoConfig {
  afterSubmit?: () => void;
  allowNull?: boolean;
  beforeSubmit?: () => void | false;
  component?: RenderableProps<any>["component"];
  data?: Record<string, any>;
  defaultValue?: any;
  format?: (value: any, name: string) => any;
  formatOnBlur?: boolean;
  initialValue?: any;
  isEqual?: (a: any, b: any) => boolean;
  multiple?: boolean;
  parse?: (value: any, name: string) => any;
  type?: string;
  validate?: FieldValidator<any>;
  validateFields?: string[];
  value?: any;
}

export interface UseFieldConfig extends UseFieldAutoConfig {
  subscription?: FieldSubscription;
}

export interface FieldProps<
  FieldValue = any,
  T = any,
  _FormValues = Record<string, any>,
> extends UseFieldConfig,
    Omit<RenderableProps<FieldRenderProps<FieldValue, T>>, "children"> {
  name: string;
  children?: RenderableProps<FieldRenderProps<FieldValue, T>>["children"];
  input?: Partial<FieldInputProps<FieldValue, T>>; // Allow overriding input props
  [key: string]: any; // Allow additional props for HTML elements
}
