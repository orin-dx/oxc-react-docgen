/**
 * Fixture for SVGAttributes / HTMLProps / ComponentRef patterns.
 *
 * These React namespace types must be recognized without producing
 * UnresolvableImport warnings, and SVGAttributes should map an HTML element
 * for notableInherited.
 */
import * as React from 'react';

export interface IconProps extends React.SVGAttributes<SVGSVGElement> {
  /** Icon size in pixels. */
  size?: number;
  /** Icon color. */
  color?: string;
}

/**
 * A generic SVG icon wrapper.
 */
export const Icon = React.forwardRef<SVGSVGElement, IconProps>(
  ({ size = 24, color = 'currentColor', ...props }, ref) => (
    <svg ref={ref} width={size} height={size} fill={color} {...props} />
  ),
);
Icon.displayName = 'Icon';

export interface BoxProps extends React.HTMLProps<HTMLDivElement> {
  /** Apply padding. */
  padded?: boolean;
}

export const Box = React.forwardRef<HTMLDivElement, BoxProps>(
  ({ padded, ...props }, ref) => <div ref={ref} {...props} />,
);
Box.displayName = 'Box';
