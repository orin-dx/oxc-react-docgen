/**
 * Fixture for JSDoc/TSDoc patterns not exercised by other fixtures.
 *
 * Covered:
 *   - ComponentEntry.tags (non-empty) via @see, @since, @category on the component const
 *   - @since tag on a prop (not present in any other fixture)
 *   - Component description extracted from JSDoc on the variable declaration
 */
import * as React from 'react';

export interface CardProps extends React.HTMLAttributes<HTMLDivElement> {
  /**
   * The visual appearance of the card.
   * @default "flat"
   * @deprecated Use `appearance` instead.
   */
  variant?: 'flat' | 'outlined' | 'elevated';
  /**
   * Header slot content.
   * @since 2.0.0
   */
  header?: React.ReactNode;
}

/**
 * A card for grouping related content.
 *
 * @see https://example.com/components/card
 * @since 1.0.0
 * @category Layout
 */
export const Card = React.forwardRef<HTMLDivElement, CardProps>(
  ({ variant = 'flat', ...props }, ref) => <div ref={ref} {...props} />,
);
Card.displayName = 'Card';
