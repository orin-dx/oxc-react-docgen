/**
 * Fixture for union-of-interfaces as the component's root props type.
 *
 * The Radix Accordion / MUI TextField pattern: the component accepts one of
 * several mutually exclusive prop shapes, distinguished by a literal `type`
 * discriminant. This fixture documents what the resolver actually produces —
 * whether it detects discriminantProp, merges props, or degrades.
 *
 * Exercises:
 *   - ForwardRefExoticComponent<InterfaceA | InterfaceB>
 *   - Literal discriminant property (`type: 'single'` vs `type: 'multiple'`)
 *   - Array prop type (value?: string[] in the multiple branch)
 *
 * Observed resolver behaviour:
 *   - The Accordion component is NOT extracted at all. The resolver does not
 *     handle a union-of-interfaces as the root props type and silently skips
 *     the component. No discriminantProp detection, no prop merging, no
 *     diagnostic. This is a known limitation: union props types are unsupported.
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
