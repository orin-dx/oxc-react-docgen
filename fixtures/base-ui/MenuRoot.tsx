import * as React from "react";

/**
 * Base UI — @base-ui-components/react (simplified fixture)
 *
 * Adapted from mui/base-ui (MIT license), the real source file:
 * https://github.com/mui/base-ui/blob/master/packages/react/src/menu/root/MenuRoot.tsx
 *
 * Base UI's Menu is generic over a `Payload` type carried from a `Menu.Trigger`
 * through to the menu's render-prop `children`, and its public API is exposed
 * as a namespace attached to the component (`MenuRoot.Props<Payload>`,
 * `MenuRoot.Actions`, `MenuRoot.ChangeEventDetails`, ...) instead of
 * free-standing named exports. This fixture keeps that real prop surface, the
 * generic `Payload` parameter, and the namespace-export shape close to
 * verbatim (docs included). What's stubbed out: the `floating-ui-react`
 * positioning/interaction hooks (`useDismiss`, `useListNavigation`,
 * `useTypeahead`, `FloatingTree`, ...), the internal `MenuStore`/`MenuHandle`
 * state machine, `@base-ui/utils/*` hooks, and sibling contexts
 * (`MenubarContext`, `ContextMenuRootContext`) — none of those packages are
 * installed in this repo, and none of them affect the prop-type shape this
 * fixture exists to exercise. The component body below is a minimal
 * reimplementation (plain `useState` + context) that preserves the real
 * prop names, defaults, and JSDoc rather than the real interaction logic.
 */

// ---- inlined stand-ins for internal Base UI plumbing not installed here ----

const REASONS = {
  none: "none",
  triggerPress: "trigger-press",
  triggerHover: "trigger-hover",
  triggerFocus: "trigger-focus",
  outsidePress: "outside-press",
  itemPress: "item-press",
  closePress: "close-press",
  focusOut: "focus-out",
  escapeKey: "escape-key",
  listNavigation: "list-navigation",
  cancelOpen: "cancel-open",
  siblingOpen: "sibling-open",
  imperativeAction: "imperative-action",
} as const;

/** Data passed to a render-prop `children` function; carries the active trigger's payload. */
type PayloadChildRenderFunction<Payload> = (params: {
  payload: Payload | undefined;
}) => React.ReactNode;

/** Details of custom change events emitted by Base UI components (real shape lives in internals/createBaseUIEventDetails.ts). */
interface BaseUIChangeEventDetails<Reason extends string> {
  reason: Reason;
  event: Event;
  cancel(): void;
  allowPropagation(): void;
  isCanceled: boolean;
  isPropagationAllowed: boolean;
  trigger: Element | undefined;
}

/** Imperative handle used to associate an out-of-tree trigger with this menu (real type lives in menu/store/MenuHandle.ts). */
declare class MenuHandle<Payload = unknown> {
  private __payload?: Payload;
}

interface MenubarContext {
  orientation: "horizontal" | "vertical";
  disabled: boolean;
  hasSubmenuOpen: boolean;
}

interface ContextMenuRootContext {
  positionerRef: React.RefObject<HTMLElement | null>;
  actionsRef: React.RefObject<{ setOpen: (open: boolean) => void } | null>;
}

interface MenuRootContextValue<Payload = unknown> {
  store: unknown;
  parent: MenuParent;
}

const MenuRootReactContext = React.createContext<MenuRootContextValue | undefined>(undefined);

// ---- component ----

/**
 * Groups all parts of the menu.
 * Doesn't render its own HTML element.
 *
 * Documentation: [Base UI Menu](https://base-ui.com/react/components/menu)
 */
export function MenuRoot<Payload>(props: MenuRoot.Props<Payload>) {
  const {
    children,
    open: openProp,
    onOpenChange,
    onOpenChangeComplete,
    defaultOpen = false,
    disabled = false,
    modal,
    loopFocus = true,
    orientation = "vertical",
    actionsRef,
    closeParentOnEsc = false,
    handle,
    triggerId,
    defaultTriggerId = null,
    highlightItemOnHover = true,
  } = props;

  const [uncontrolledOpen, setUncontrolledOpen] = React.useState(defaultOpen);
  const open = openProp ?? uncontrolledOpen;
  const [payload] = React.useState<Payload | undefined>(undefined);
  const [activeTriggerId, setActiveTriggerId] = React.useState(triggerId ?? defaultTriggerId);

  const setOpen = React.useCallback(
    (nextOpen: boolean, reason: MenuRoot.ChangeEventReason) => {
      const eventDetails: MenuRoot.ChangeEventDetails = {
        reason,
        event: new Event("base-ui"),
        cancel() {},
        allowPropagation() {},
        isCanceled: false,
        isPropagationAllowed: false,
        trigger: undefined,
        preventUnmountOnClose() {},
      };
      onOpenChange?.(nextOpen, eventDetails);
      if (eventDetails.isCanceled) {
        return;
      }
      setUncontrolledOpen(nextOpen);
      onOpenChangeComplete?.(nextOpen);
    },
    [onOpenChange, onOpenChangeComplete],
  );

  React.useImperativeHandle(
    actionsRef,
    () => ({
      unmount: () => setOpen(false, REASONS.imperativeAction),
      close: () => setOpen(false, REASONS.imperativeAction),
    }),
    [setOpen],
  );

  const context: MenuRootContextValue<Payload> = React.useMemo(
    () => ({
      store: { open, payload, disabled, modal, loopFocus, orientation, highlightItemOnHover, activeTriggerId },
      parent: { type: undefined },
    }),
    [open, payload, disabled, modal, loopFocus, orientation, highlightItemOnHover, activeTriggerId],
  );

  void handle;
  void closeParentOnEsc;
  void setActiveTriggerId;

  return (
    <MenuRootReactContext.Provider value={context as MenuRootContextValue}>
      {typeof children === "function" ? children({ payload }) : children}
    </MenuRootReactContext.Provider>
  );
}

export interface MenuRootState {}

export interface MenuRootProps<Payload = unknown> {
  /**
   * Whether the menu is initially open.
   *
   * To render a controlled menu, use the `open` prop instead.
   * @default false
   */
  defaultOpen?: boolean | undefined;
  /**
   * Whether to loop keyboard focus back to the first item
   * when the end of the list is reached while using the arrow keys.
   * @default true
   */
  loopFocus?: boolean | undefined;
  /**
   * Whether moving the pointer over items should highlight them.
   * Disabling this prop allows CSS `:hover` to be differentiated from the `:focus` (`data-highlighted`) state.
   * @default true
   */
  highlightItemOnHover?: boolean | undefined;
  /**
   * Determines if the menu enters a modal state when open.
   * - `true`: user interaction is limited to the menu: document page scroll is locked and pointer interactions on outside elements are disabled.
   * - `false`: user interaction with the rest of the document is allowed.
   *
   * On touch devices, a `true` modal blocks outside taps but leaves the page scrollable unless the popup spans nearly the full viewport width, matching native iOS behavior.
   *
   * Nested menus ignore this prop, and menus opened by hover are never modal.
   * @default true
   */
  modal?: boolean | undefined;
  /**
   * Event handler called when the menu is opened or closed.
   */
  onOpenChange?: ((open: boolean, eventDetails: MenuRoot.ChangeEventDetails) => void) | undefined;
  /**
   * Event handler called after any animations complete when the menu is opened or closed.
   */
  onOpenChangeComplete?: ((open: boolean) => void) | undefined;
  /**
   * Whether the menu is currently open.
   */
  open?: boolean | undefined;
  /**
   * The visual orientation of the menu.
   * Controls whether roving focus uses up/down or left/right arrow keys.
   * @default 'vertical'
   */
  orientation?: MenuRoot.Orientation | undefined;
  /**
   * Whether the component should ignore user interaction.
   * @default false
   */
  disabled?: boolean | undefined;
  /**
   * When in a submenu, determines whether pressing the Escape key
   * closes the entire menu, or only the current child menu.
   * @default false
   */
  closeParentOnEsc?: boolean | undefined;
  /**
   * A ref to imperative actions.
   * - `unmount`: Manually unmounts the menu.
   *   Call this after any externally controlled closing animation finishes.
   * - `close`: When specified, the menu can be closed imperatively.
   */
  actionsRef?: React.RefObject<MenuRoot.Actions | null> | undefined;
  /**
   * ID of the trigger that the menu is associated with.
   * This is useful in conjunction with the `open` prop to create a controlled menu.
   * There's no need to specify this prop when the menu is uncontrolled (that is, when the `open` prop is not set).
   */
  triggerId?: string | null | undefined;
  /**
   * ID of the trigger that the menu is associated with.
   * This is useful in conjunction with the `defaultOpen` prop to create an initially open menu.
   */
  defaultTriggerId?: string | null | undefined;
  /**
   * A handle to associate the menu with a trigger.
   * If specified, allows external triggers to control the menu's open state.
   */
  handle?: MenuHandle<Payload> | undefined;
  /**
   * The content of the menu.
   * This can be a regular React node or a render function that receives the `payload` of the active trigger.
   */
  children?: React.ReactNode | PayloadChildRenderFunction<Payload>;
}

export interface MenuRootActions {
  unmount: () => void;
  close: () => void;
}

export type MenuRootChangeEventReason =
  | typeof REASONS.triggerHover
  | typeof REASONS.triggerFocus
  | typeof REASONS.triggerPress
  | typeof REASONS.outsidePress
  | typeof REASONS.focusOut
  | typeof REASONS.listNavigation
  | typeof REASONS.escapeKey
  | typeof REASONS.itemPress
  | typeof REASONS.closePress
  | typeof REASONS.siblingOpen
  | typeof REASONS.cancelOpen
  | typeof REASONS.imperativeAction
  | typeof REASONS.none;

export type MenuRootChangeEventDetails = BaseUIChangeEventDetails<MenuRoot.ChangeEventReason> & {
  preventUnmountOnClose(): void;
};

export type MenuRootOrientation = "horizontal" | "vertical";

export type MenuParent =
  | {
      type: "menu";
      store: unknown;
    }
  | {
      type: "menubar";
      context: MenubarContext;
    }
  | {
      type: "context-menu";
      context: ContextMenuRootContext;
    }
  | {
      type: "nested-context-menu";
      context: ContextMenuRootContext;
      menuContext: MenuRootContextValue;
    }
  | {
      type: undefined;
    };

export namespace MenuRoot {
  export type State = MenuRootState;
  export type Props<Payload = unknown> = MenuRootProps<Payload>;
  export type Actions = MenuRootActions;
  export type ChangeEventReason = MenuRootChangeEventReason;
  export type ChangeEventDetails = MenuRootChangeEventDetails;
  export type Orientation = MenuRootOrientation;
}
