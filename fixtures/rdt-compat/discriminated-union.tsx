/**
 * Fixture for union-of-interfaces as the component's root props type.
 *
 * The Radix Accordion / MUI TextField pattern: the component accepts one of
 * several mutually exclusive prop shapes, distinguished by a literal `type`
 * discriminant. This fixture documents what the resolver actually produces —
 * discriminantProp detection, prop merging from the first union member, and
 * the discriminant prop's type showing the full union of literals.
 *
 * Exercises:
 *   - forwardRef<Ref, InterfaceA | InterfaceB> inline union prop type
 *   - Literal discriminant property (`type: 'single'` vs `type: 'multiple'`)
 *   - Array prop type (value?: string[] in the multiple branch)
 *
 * Observed resolver behaviour:
 *   - Accordion is extracted with all props from AccordionSingleProps.
 *   - discriminantProp: "type" is detected correctly.
 *   - The `type` prop shows `'single' | 'multiple'` (union from both members).
 *   - Duplicate prop names (value, defaultValue, onValueChange) resolve to the
 *     first member's types (AccordionSingleProps wins over AccordionMultipleProps).
 */
import * as React from 'react';

interface AccordionSingleProps {
  /** Selects single-item open mode. */
  type: 'single';
  /** The controlled open item ID (controlled). */
  value?: string;
  /** The initially open item ID (uncontrolled). */
  defaultValue?: string;
  /** Called when the open item changes. */
  onValueChange?(value: string): void;
  /** Whether the open item can be collapsed by clicking it again. */
  collapsible?: boolean;
}

interface AccordionMultipleProps {
  /** Selects multiple-item open mode. */
  type: 'multiple';
  /** The controlled set of open item IDs (controlled). */
  value?: string[];
  /** The initially open item IDs (uncontrolled). */
  defaultValue?: string[];
  /** Called when the open item set changes. */
  onValueChange?(value: string[]): void;
}

export const Accordion = React.forwardRef<
  HTMLDivElement,
  AccordionSingleProps | AccordionMultipleProps
>(({ ...props }, ref) => <div ref={ref} />);
Accordion.displayName = 'Accordion';
