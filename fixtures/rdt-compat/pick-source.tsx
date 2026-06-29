/**
 * Fixture for Pick<T, Keys> over a source-defined interface.
 *
 * Pick works when T is in the same project. It does NOT work for library
 * types like Pick<ButtonHTMLAttributes<...>, Keys> because HTML attribute
 * types are not in the source graph.
 *
 * Exercises:
 *   - interface extends Pick<SourceInterface, 'key1' | 'key2'>
 *   - only picked keys appear (name and value must NOT appear in output)
 *
 * Observed resolver behaviour:
 *   - Pick<SourceInterface, Keys> is NOT expanded by the resolver even when T
 *     is source-defined. Only own props of IconButtonProps (icon, label) appear
 *     in the output; disabled, type, and form are absent. This is a known
 *     limitation: Pick<> unwrapping is not yet implemented.
 */
import * as React from 'react';

export interface ButtonBaseProps {
  /** Whether the button is disabled. */
  disabled?: boolean;
  /** The button's type attribute. */
  type?: 'button' | 'submit' | 'reset';
  /** Associates the button with a form element. */
  form?: string;
  /** The button's name (for form submission). */
  name?: string;
  /** The button's value (for form submission). */
  value?: string;
}

/**
 * An icon button that picks a subset of ButtonBaseProps.
 * Only `disabled`, `type`, and `form` are inherited — not `name` or `value`.
 */
export interface IconButtonProps extends Pick<ButtonBaseProps, 'disabled' | 'type' | 'form'> {
  /** The icon to display inside the button. */
  icon: React.ReactNode;
  /** Accessible label for screen readers. */
  label: string;
}

export const IconButton = React.forwardRef<HTMLButtonElement, IconButtonProps>(
  ({ icon, label, ...props }, ref) => (
    <button ref={ref} aria-label={label} {...props}>{icon}</button>
  ),
);
IconButton.displayName = 'IconButton';
