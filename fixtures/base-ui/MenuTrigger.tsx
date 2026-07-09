import * as React from "react";

/**
 * Base UI — @base-ui-components/react (simplified fixture)
 *
 * Adapted from mui/base-ui (MIT license), the real source file:
 * https://github.com/mui/base-ui/blob/master/packages/react/src/menu/trigger/MenuTrigger.tsx
 *
 * Companion to ./MenuRoot.tsx. Demonstrates Base UI's `render`-prop
 * polymorphism (`BaseUIComponentProps`, from packages/react/src/internals/types.ts
 * and packages/react/src/types/index.ts, reproduced near-verbatim below) as an
 * alternative to Radix's `asChild` pattern: consumers pass a `ReactElement` or a
 * `(props, state) => ReactElement` function to `render` instead of a boolean flag.
 * It also keeps the real generic `Payload` parameter (`handle`/`payload` props)
 * and the `interface + const + namespace` triple-merge trick Base UI uses to get
 * a generically-callable component that still exposes `MenuTrigger.Props<Payload>`.
 *
 * Stubbed out: `floating-ui-react` hover/click/focus interaction hooks, the
 * `MenuStore`/`useMenuRootContext` wiring, `CompositeItem`/menubar integration,
 * and `@base-ui/utils/*` helpers — none of those packages are installed here and
 * none of them affect the prop-type shape this fixture exists to exercise.
 */

// ---- inlined stand-ins for internal Base UI plumbing not installed here ----
// (mirrors packages/react/src/internals/types.ts and packages/react/src/types/index.ts)

type HTMLProps<T = any> = React.HTMLAttributes<T> & { ref?: React.Ref<T> | undefined };

/** Shape of the render prop: a function taking props to spread and the component's state, returning an element. */
type ComponentRenderFn<Props, State> = (props: Props, state: State) => React.ReactElement<unknown>;

type BaseUIEvent<E extends React.SyntheticEvent<Element, Event>> = E & {
  preventBaseUIHandler: () => void;
  readonly baseUIHandlerPrevented?: boolean | undefined;
};

type WithPreventBaseUIHandler<T> = T extends (event: infer E) => any
  ? E extends React.SyntheticEvent<Element, Event>
    ? (event: BaseUIEvent<E>) => ReturnType<T>
    : T
  : T extends undefined
    ? undefined
    : T;

/** Adds a `preventBaseUIHandler` method to all event handlers. */
type WithBaseUIEvent<T> = {
  [K in keyof T]: WithPreventBaseUIHandler<T[K]>;
};

/**
 * Props shared by all Base UI components.
 * Contains `className` (string or callback taking the component's state as an argument)
 * and `render` (function or element to customize rendering) — this is the mechanism
 * that replaces Radix's `asChild` boolean with function-based polymorphism.
 */
type BaseUIComponentProps<
  ElementType extends React.ElementType,
  State,
  RenderFunctionProps = HTMLProps,
> = Omit<
  WithBaseUIEvent<React.ComponentPropsWithRef<ElementType>>,
  "className" | "color" | "defaultValue" | "defaultChecked" | "style"
> & {
  /**
   * CSS class applied to the element, or a function that
   * returns a class based on the component's state.
   */
  className?: string | ((state: State) => string | undefined) | undefined;
  /**
   * Allows you to replace the component's HTML element
   * with a different tag, or compose it with another component.
   *
   * Accepts a `ReactElement` or a function that returns the element to render.
   */
  render?: React.ReactElement | ComponentRenderFn<RenderFunctionProps, State> | undefined;
  /**
   * Style applied to the element, or a function that
   * returns a style object based on the component's state.
   */
  style?: React.CSSProperties | ((state: State) => React.CSSProperties | undefined) | undefined;
};

interface NativeButtonProps {
  /**
   * Whether the component renders a native `<button>` element when replacing it
   * via the `render` prop.
   * Set to `false` if the rendered element is not a button (for example, `<div>`).
   * @default true
   */
  nativeButton?: boolean | undefined;
}

/** Imperative handle used to associate an out-of-tree trigger with a menu (real type lives in menu/store/MenuHandle.ts). */
declare class MenuHandle<Payload = unknown> {
  private __payload?: Payload;
}

function renderByProp<Props extends Record<string, unknown>, State>(
  render: React.ReactElement | ComponentRenderFn<Props, State> | undefined,
  props: Props,
  state: State,
  defaultElement: React.ReactElement,
): React.ReactElement {
  if (typeof render === "function") {
    return render(props, state);
  }
  if (render) {
    return React.cloneElement(render, props);
  }
  return React.cloneElement(defaultElement, props);
}

// ---- component ----

/**
 * A button that opens the menu.
 * Renders a `<button>` element.
 *
 * Documentation: [Base UI Menu](https://base-ui.com/react/components/menu)
 */
export const MenuTrigger = React.forwardRef(function MenuTrigger(
  componentProps: MenuTrigger.Props,
  forwardedRef: React.ForwardedRef<HTMLElement>,
) {
  const {
    render,
    className,
    style,
    disabled = false,
    nativeButton = true,
    id,
    openOnHover,
    delay = 100,
    closeDelay = 0,
    handle,
    payload,
    ...elementProps
  } = componentProps;

  const state: MenuTriggerState = {
    disabled,
    open: false,
  };

  void openOnHover;
  void delay;
  void closeDelay;
  void handle;
  void payload;

  const resolvedClassName = typeof className === "function" ? className(state) : className;
  const resolvedStyle = typeof style === "function" ? style(state) : style;

  const props = {
    ...elementProps,
    id,
    disabled,
    className: resolvedClassName,
    style: resolvedStyle,
    ref: forwardedRef,
    type: nativeButton ? ("button" as const) : undefined,
    "aria-haspopup": "menu" as const,
  };

  return renderByProp(render, props, state, <button type="button" />);
}) as MenuTrigger;

export interface MenuTrigger {
  <Payload>(
    componentProps: MenuTriggerProps<Payload> & React.RefAttributes<HTMLElement>,
  ): React.ReactElement | null;
}

export interface MenuTriggerProps<Payload = unknown>
  extends NativeButtonProps,
    BaseUIComponentProps<"button", MenuTriggerState> {
  children?: React.ReactNode;
  /**
   * Whether the component should ignore user interaction.
   * @default false
   */
  disabled?: boolean | undefined;
  /**
   * A handle to associate the trigger with a menu.
   */
  handle?: MenuHandle<Payload> | undefined;
  /**
   * A payload to pass to the menu when it is opened.
   */
  payload?: Payload | undefined;
  /**
   * How long to wait before the menu may be opened on hover. Specified in milliseconds.
   *
   * Requires the `openOnHover` prop.
   * @default 100
   */
  delay?: number | undefined;
  /**
   * How long to wait before closing the menu that was opened on hover.
   * Specified in milliseconds.
   *
   * Requires the `openOnHover` prop.
   * @default 0
   */
  closeDelay?: number | undefined;
  /**
   * Whether the menu should also open when the trigger is hovered.
   */
  openOnHover?: boolean | undefined;
}

export interface MenuTriggerState {
  /**
   * Whether the menu is currently open and was opened by this trigger.
   */
  open: boolean;
  /**
   * Whether the trigger is disabled.
   */
  disabled: boolean;
}

export namespace MenuTrigger {
  export type Props<Payload = unknown> = MenuTriggerProps<Payload>;
  export type State = MenuTriggerState;
}
