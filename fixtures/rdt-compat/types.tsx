/**
 * Targeted fixture for PropType kinds not exercised by any other fixture.
 *
 * Covered: array, tuple, numberLiteral, boolLiteral, undefined, sxProps,
 *          htmlAttributes (ComponentPropsWithoutRef as value type),
 *          intersection (CSSProperties & inline object), opaque (conditional type).
 */
import * as React from 'react';

export interface TypeSamplerProps {
  /** Array written as T[] (not Array<T>) — exercises the `array` PropType kind. */
  items: string[];
  /** Positional tuple — exercises the `tuple` PropType kind. */
  position: [number, number];
  /** Numeric literals in a union — exercises `numberLiteral`. */
  step: 1 | 2 | 4 | 8;
  /** Standalone boolean literal — exercises `boolLiteral`. */
  alwaysEnabled?: true;
  /** Explicit undefined — exercises the `undefined` PropType kind. */
  nothing?: undefined;
  /**
   * Resolved via the SxProps known-pattern shortcut (fires when the name
   * cannot be resolved from source imports).
   */
  sx?: SxProps;
  /**
   * Inline intersection in a prop value — exercises `intersection`.
   * CssProperties resolves to cssProperties; the object literal stays as-is.
   */
  tokenStyle?: React.CSSProperties & { '--accent': string };
  /**
   * ComponentPropsWithoutRef<E> as a prop value — exercises `htmlAttributes`.
   * Distinct from inheriting ButtonHTMLAttributes at the component level.
   */
  buttonProps?: React.ComponentPropsWithoutRef<'button'>;
  /**
   * Inline conditional type — cannot expand without a type checker → `opaque`
   * with reason `conditionalType`.
   */
  derived?: string extends 'a' ? 'yes' : 'no';
}

export const TypeSampler = React.forwardRef<HTMLDivElement, TypeSamplerProps>(
  (props, ref) => <div ref={ref} />,
);
TypeSampler.displayName = 'TypeSampler';
