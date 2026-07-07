/**
 * Fixture for destructured parameter defaults in forwardRef and FC-typed components.
 *
 * The dominant shadcn/Radix authoring pattern destructures props with defaults
 * directly in the render function's parameter list: `({ variant = 'primary' }) => ...`.
 * try_forward_ref and try_fc_annotation previously hardcoded param_defaults to empty,
 * so these defaults never made it into ComponentMapping.
 */
import * as React from 'react';

export interface ToggleProps {
  /** Whether the toggle is pressed. */
  pressed?: boolean;
  /** Size variant. */
  size?: 'sm' | 'md' | 'lg';
}

export const Toggle = React.forwardRef<HTMLButtonElement, ToggleProps>(
  ({ pressed = false, size = 'md' }, ref) => <button ref={ref} aria-pressed={pressed} data-size={size} />
);
Toggle.displayName = 'Toggle';

export interface ChipProps {
  /** The chip's label text. */
  label: string;
  /** Whether the chip shows a close button. */
  closable?: boolean;
}

export const Chip: React.FC<ChipProps> = ({ label, closable = true }) => (
  <span>
    {label}
    {closable && <button aria-label="Remove" />}
  </span>
);
