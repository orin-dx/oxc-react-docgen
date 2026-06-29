/**
 * Fixture for the controlled/uncontrolled triple pattern.
 *
 * Ubiquitous in headless UI libraries (Radix UI, Ark UI, Headless UI):
 * every stateful component exposes value + defaultValue + onValueChange
 * for controlled/uncontrolled operation.
 *
 * Exercises:
 *   - value/defaultValue/onValueChange triple
 *   - open/defaultOpen/onOpenChange triple
 *   - method-shorthand handler syntax: `onValueChange?(value: string): void`
 *   - arrow-function handler syntax: `onOpenChange?: (open: boolean) => void`
 *
 * Observed resolver behaviour:
 *   - Arrow-function form resolves correctly: eventType reflects the param type
 *     (e.g. onOpenChange → eventType: "boolean").
 *   - Method-shorthand form does NOT extract the param type: eventType is "..."
 *     for onValueChange instead of "string". This is a known resolver limitation.
 */
import * as React from 'react';

export interface SelectProps {
  /** Currently selected value (controlled). */
  value?: string;
  /** Initial selected value (uncontrolled). */
  defaultValue?: string;
  /** Called when the selected value changes. Method-shorthand form. */
  onValueChange?(value: string): void;
  /** Whether the dropdown is open (controlled). */
  open?: boolean;
  /** Initial open state (uncontrolled). */
  defaultOpen?: boolean;
  /** Called when the open state changes. Arrow-function form for comparison. */
  onOpenChange?: (open: boolean) => void;
  /** Whether the select is disabled. */
  disabled?: boolean;
  children?: React.ReactNode;
}

export const Select = React.forwardRef<HTMLDivElement, SelectProps>(
  ({ children, ...props }, ref) => <div ref={ref}>{children}</div>,
);
Select.displayName = 'Select';
