import * as React from "react";

/**
 * React Aria — @react-aria/button / @adobe/react-spectrum Button (simplified fixture)
 *
 * React Aria components are headless/unstyled and use a hook-first model.
 * Key React Aria patterns shown here:
 *   - No HTML attribute inheritance by default (accessibility-first design)
 *   - `onPress` / `onPressStart` / `onPressEnd` instead of `onClick`
 *   - `isDisabled` instead of `disabled` (WAI-ARIA naming convention)
 *   - `elementType` / `href` for polymorphic link-button
 *   - `excludeFromTabOrder` for focus management
 *   - render-prop children for state-driven rendering
 */

export interface PressEvent {
  /** The type of press event being fired. */
  type: "pressstart" | "pressend" | "pressup" | "press";
  /** The pointer type that triggered the press event. */
  pointerType: "mouse" | "pen" | "touch" | "keyboard" | "virtual";
  /** The target element of the press event. */
  target: Element;
  /** Whether the shift keyboard modifier was held during the press event. */
  shiftKey: boolean;
  /** Whether the ctrl keyboard modifier was held during the press event. */
  ctrlKey: boolean;
  /** Whether the meta keyboard modifier was held during the press event. */
  metaKey: boolean;
  /** Whether the alt keyboard modifier was held during the press event. */
  altKey: boolean;
  /** X position relative to the target. */
  x: number;
  /** Y position relative to the target. */
  y: number;
  /** Stops propagation of the press event. */
  continuePropagation(): void;
}

export interface ButtonRenderProps {
  /** Whether the button is currently hovered with a mouse. */
  isHovered: boolean;
  /** Whether the button is currently in a pressed state. */
  isPressed: boolean;
  /** Whether the button is focused, either via a mouse or keyboard. */
  isFocused: boolean;
  /** Whether the button is keyboard focused. */
  isFocusVisible: boolean;
  /** Whether the button is disabled. */
  isDisabled: boolean;
}

export interface ButtonProps {
  /** The content to display in the button. */
  children?: React.ReactNode | ((renderProps: ButtonRenderProps) => React.ReactNode);
  /**
   * The behavior of the button when used in an HTML form.
   * @default 'button'
   */
  type?: "button" | "submit" | "reset";
  /**
   * Whether the button is disabled.
   * @default false
   */
  isDisabled?: boolean;
  /**
   * Whether the element should not be included in the tab order.
   * Use when the button is rendered inside a group that manages focus.
   * @default false
   */
  excludeFromTabOrder?: boolean;
  /** Whether keyboard presses should trigger press events. */
  allowFocusWhenDisabled?: boolean;
  /** The URL that the hyperlink points to. If provided, the button is rendered as an anchor. */
  href?: string;
  /** Hints at the linked URL's format; used when `href` is specified. */
  hrefLang?: string;
  /** Where to display the linked URL; used when `href` is specified. */
  target?: string;
  /** The relationship of the linked URL as space-separated link types. */
  rel?: string;
  /** Download a URL instead of navigating to it. */
  download?: string | boolean;
  /** Specifies the MIME type of the linked URL. */
  ping?: string;
  /** How much of the referrer to send when following the link. */
  referrerPolicy?: React.HTMLAttributeReferrerPolicy;
  /** A router options object. Only applies when `href` is specified and a router is configured. */
  routerOptions?: object;
  /** Handler that is called when the press is released over the target. */
  onPress?: (e: PressEvent) => void;
  /** Handler that is called when a press interaction starts. */
  onPressStart?: (e: PressEvent) => void;
  /** Handler that is called when a press interaction ends, either over the target or when the pointer leaves the target. */
  onPressEnd?: (e: PressEvent) => void;
  /** Handler that is called when the press state changes. */
  onPressChange?: (isPressed: boolean) => void;
  /** Handler that is called when a press is released over the target regardless of whether it started on the target or not. */
  onPressUp?: (e: PressEvent) => void;
  /** Handler that is called when the element receives focus. */
  onFocus?: (e: React.FocusEvent<HTMLButtonElement>) => void;
  /** Handler that is called when the element loses focus. */
  onBlur?: (e: React.FocusEvent<HTMLButtonElement>) => void;
  /** Handler that is called when the element's focus status changes. */
  onFocusChange?: (isFocused: boolean) => void;
  /** Handler that is called when a key is pressed. */
  onKeyDown?: (e: React.KeyboardEvent<HTMLButtonElement>) => void;
  /** Handler that is called when a key is released. */
  onKeyUp?: (e: React.KeyboardEvent<HTMLButtonElement>) => void;
  /** An accessibility label for this element. */
  "aria-label"?: string;
  /** Identifies the element (or elements) that labels the current element. */
  "aria-labelledby"?: string;
  /** Identifies the element (or elements) that describes the object. */
  "aria-describedby"?: string;
  /** Identifies the element (or elements) that provide a detailed, extended description for the object. */
  "aria-details"?: string;
  /** Indicates to assistive technology that an element is expanded or collapsed. */
  "aria-expanded"?: boolean | "true" | "false";
  /** Identifies the element (or elements) whose contents or presence are controlled by the current element. */
  "aria-controls"?: string;
  /** Indicates the availability and type of interactive popup element that can be triggered by an element. */
  "aria-haspopup"?: boolean | "menu" | "listbox" | "tree" | "grid" | "dialog" | "true" | "false";
  /** Indicates whether the element, or another grouping element it controls, is currently selected or checked. */
  "aria-pressed"?: boolean | "true" | "false" | "mixed";
  /** Defines a string value that labels the current element. */
  id?: string;
  /** Allows an author to specify, on a per-element basis, which accessibility features the element exposes to the accessibility tree. */
  role?: string;
  /** Additional CSS class names to apply. */
  className?: string | ((renderProps: ButtonRenderProps) => string);
  /** Additional inline styles. */
  style?: React.CSSProperties | ((renderProps: ButtonRenderProps) => React.CSSProperties);
}

/** A button allows a user to perform an action, with mouse, touch, and keyboard interactions. */
declare const Button: React.ForwardRefExoticComponent<
  ButtonProps & React.RefAttributes<HTMLButtonElement>
>;

export { Button };
