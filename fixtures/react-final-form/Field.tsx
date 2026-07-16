import * as React from "react";
import type { FieldProps, FieldRenderProps } from "./types";

/**
 * final-form/react-final-form — src/Field.tsx (simplified fixture)
 *
 * Adapted from final-form/react-final-form (MIT license), the real source file:
 * https://github.com/final-form/react-final-form/blob/main/src/Field.tsx
 *
 * This is react-docgen-typescript's primary "children as a render function"
 * pattern: `children` is real-typed as
 *   ((props: FieldRenderProps<FieldValue, T> & typeof rest) => React.ReactNode) | React.ReactNode
 * (see `RenderableProps<T>` in ./types.ts) and `FieldComponent` branches on
 * `typeof children === "function"` at runtime to decide whether to invoke it
 * as a render prop or pass it through as ordinary React children. The whole
 * `FieldComponent` body below — including that branch and the exact cast on
 * `children` — is kept verbatim; it's what this fixture exists to exercise.
 *
 * Stubbed out: the real `./renderComponent` import (~60 lines shared with
 * FormSpy that juggle property descriptors so lazily-computed field state
 * isn't eagerly evaluated) and the real `./useField` import (~370 lines
 * wiring a field subscription up to final-form's core `FormApi`, using
 * `useConstantCallback`/`useLatest`/`shallowEqual`). Neither affects the
 * *prop types* under test, so both collapse to minimal local stand-ins with
 * the same call signatures.
 */

// Stub of ./renderComponent — see header comment.
function renderComponent<T>(
  props: { component?: unknown; children?: unknown; render?: (props: T) => React.ReactNode } & Record<
    string,
    any
  >,
  lazyProps: Record<string, any>,
  name: string,
): React.ReactNode {
  const { render, children, component, ...rest } = props;
  const result = { ...lazyProps, ...rest } as T;
  if (component) return React.createElement(component as any, result as any);
  if (render) return render(result);
  if (typeof children === "function") return (children as any)(result);
  throw new Error(
    `Must specify either a render prop, a render function as children, or a component prop to ${name}`,
  );
}

// Stub of ./useField — see header comment.
function useField<
  FieldValue = any,
  T extends HTMLElement = HTMLElement,
>(name: string, _config: Record<string, unknown>): FieldRenderProps<FieldValue, T> {
  return {
    input: {
      name,
      onBlur: () => {},
      onChange: () => {},
      onFocus: () => {},
      value: undefined as unknown as FieldValue,
    },
    meta: {},
  };
}

function FieldComponent<
  FieldValue = any,
  T extends HTMLElement = HTMLElement,
  FormValues = Record<string, any>,
>(
  {
    afterSubmit,
    allowNull,
    beforeSubmit,
    children,
    component,
    data,
    defaultValue,
    format,
    formatOnBlur,
    initialValue,
    input,
    isEqual,
    multiple,
    name,
    parse,
    subscription,
    type,
    validate,
    validateFields,
    value,
    ...rest
  }: FieldProps<FieldValue, T, FormValues>,
  ref: React.Ref<T>,
) {
  const field: FieldRenderProps<FieldValue, T> = useField(name, {
    afterSubmit,
    allowNull,
    beforeSubmit,
    component,
    data,
    defaultValue,
    format,
    formatOnBlur,
    initialValue,
    isEqual,
    multiple,
    parse,
    subscription,
    type,
    validate,
    validateFields,
    value,
  });

  // Merge provided input prop with field.input
  const mergedField = input
    ? { ...field, input: { ...field.input, ...input } }
    : field;

  if (typeof children === "function") {
    return (
      children as (
        props: FieldRenderProps<FieldValue, T> & typeof rest,
      ) => React.ReactNode
    )({ ...mergedField, ...rest });
  }

  if (typeof component === "string") {
    // ignore meta, combine input with any other props
    const { name: inputName, ...restInputProps } = mergedField.input;

    // Ensure multiple select has array value
    if (
      component === "select" &&
      multiple &&
      !Array.isArray(restInputProps.value)
    ) {
      restInputProps.value = [] as any;
    }

    return React.createElement(component, {
      name: inputName, // Pass name explicitly to avoid shadowing DOM properties
      ...restInputProps,
      children,
      ref,
      ...rest,
    });
  }

  if (!name) {
    throw new Error("prop name cannot be undefined in <Field> component");
  }

  return renderComponent(
    { children, component, ...rest, ...mergedField },
    {},
    `Field(${name})`,
  );
}

// Create a properly typed forwardRef component that preserves generics
const Field = React.forwardRef(FieldComponent as any) as <
  FieldValue = any,
  T extends HTMLElement = HTMLElement,
  FormValues = Record<string, any>,
>(
  props: FieldProps<FieldValue, T, FormValues> & { ref?: React.Ref<T> },
) => React.ReactElement | null;

export default Field;
