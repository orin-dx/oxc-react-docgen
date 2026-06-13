import { defineRecipe } from "@pandacss/dev"
import { css, cx } from "../styled-system/css"
import { splitCssProps } from "../styled-system/jsx"
import type { RecipeVariantProps } from "../styled-system/types"
import * as React from "react"

// PandaCSS recipe definition (typically in panda.config.ts, shown here for fixture completeness)
export const buttonRecipe = defineRecipe({
  className: "button",
  description: "A button component",
  base: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    fontWeight: "semibold",
    borderRadius: "md",
    cursor: "pointer",
    transitionProperty: "colors",
    _disabled: {
      opacity: 0.5,
      pointerEvents: "none",
    },
  },
  variants: {
    visual: {
      solid: {
        bg: "colorPalette.500",
        color: "white",
        _hover: { bg: "colorPalette.600" },
      },
      outline: {
        borderWidth: "1px",
        borderColor: "colorPalette.500",
        color: "colorPalette.500",
        _hover: { bg: "colorPalette.50" },
      },
      ghost: {
        color: "colorPalette.500",
        _hover: { bg: "colorPalette.50" },
      },
    },
    size: {
      sm: { px: "3", py: "1.5", fontSize: "sm", h: "8" },
      md: { px: "4", py: "2", fontSize: "md", h: "10" },
      lg: { px: "6", py: "3", fontSize: "lg", h: "12" },
    },
  },
  defaultVariants: {
    visual: "solid",
    size: "md",
  },
})

// The variant props type derived from the recipe
export type ButtonVariantProps = RecipeVariantProps<typeof buttonRecipe>

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    ButtonVariantProps {
  asChild?: boolean
}

/**
 * A PandaCSS recipe-driven Button component.
 * Variant props (`visual`, `size`) are extracted from HTML attributes at runtime.
 */
export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  (props, ref) => {
    const [variantProps, htmlProps] = splitCssProps(props)
    const { visual, size, asChild: _asChild, className, ...rest } = {
      ...variantProps,
      ...htmlProps,
    } as ButtonProps

    return (
      <button
        ref={ref}
        className={cx(buttonRecipe.raw({ visual, size }), className)}
        {...rest}
      />
    )
  }
)
Button.displayName = "Button"
