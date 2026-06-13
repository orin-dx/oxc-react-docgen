import * as React from "react";

/**
 * Radix UI — @radix-ui/react-button (simplified fixture)
 *
 * The real Radix primitive uses `Primitive.button` which itself wraps
 * React.ComponentPropsWithoutRef<'button'>. The `asChild` prop enables
 * slot-based composition via @radix-ui/react-slot.
 */

type PrimitiveButtonProps = React.ComponentPropsWithoutRef<"button"> & {
  /**
   * Change the default rendered element for the one passed as a child,
   * merging their props and behavior.
   */
  asChild?: boolean;
};

export interface ButtonProps extends PrimitiveButtonProps {}

export type ButtonElement = React.ElementRef<"button">;

declare const Button: React.ForwardRefExoticComponent<
  ButtonProps & React.RefAttributes<ButtonElement>
>;

export { Button };
export type { PrimitiveButtonProps };
