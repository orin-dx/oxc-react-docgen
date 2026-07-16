/**
 * Ariakit — @ariakit/react Menu (simplified fixture)
 *
 * Adapted from ariakit/ariakit (MIT license), the real source file:
 *   https://github.com/ariakit/ariakit/blob/main/packages/ariakit-react-components/src/menu/menu.tsx
 *
 * `Menu` is the compound family's second level: it reads its store from
 * ./menu-context.ts (falling back to the nearest `MenuProvider`, exactly
 * like the real `useMenuProviderContext()` call below) rather than creating
 * its own. In real Ariakit its options interface is
 * `MenuOptions extends MenuListOptions<T>, Omit<HovercardOptions<T>, "store">`
 * — a chain through the (unfetched, out-of-scope) MenuList/Composite/
 * Hovercard/Popover/Dialog primitive packages. This fixture flattens that
 * chain to the fields `useMenu()` actually destructures off `props` in the
 * real file (`store`, `modal`, `portal`, `hideOnEscape`, `autoFocusOnShow`,
 * `hideOnHoverOutside`, `alwaysVisible`) — real field names/defaults, just
 * without the multi-package inheritance. The state-machine body (focus
 * management, popover positioning, hover-outside heuristics) is stubbed to a
 * plain `<div>`; only the prop shape is under test here.
 */
import type { KeyboardEvent, MouseEvent } from "react";
import { forwardRef } from "react";
import { useMenuProviderContext, type MenuStore, type Props } from "./menu-context.ts";

export interface MenuOptions {
  /**
   * Object returned by the
   * [`useMenuStore`](https://ariakit.com/reference/use-menu-store) hook. If
   * not provided, the closest
   * [`MenuProvider`](https://ariakit.com/reference/menu-provider)
   * component's context will be used.
   */
  store?: MenuStore;
  /**
   * Determines whether the menu is modal. Modal menus are rendered inside a
   * dialog, trap focus inside their contents, and make background content
   * inert. Nested submenus are never modal, regardless of this prop.
   * @default false
   */
  modal?: boolean;
  /**
   * Whether the menu should be rendered in a React Portal.
   * @default modal
   */
  portal?: boolean;
  /**
   * Whether pressing Escape should hide the menu.
   * @default true
   */
  hideOnEscape?: boolean | ((event: KeyboardEvent) => boolean);
  /**
   * Whether the menu should automatically receive focus after it's shown.
   * @default true
   */
  autoFocusOnShow?: boolean;
  /**
   * Whether the menu should hide when the mouse moves outside of it. By
   * default, this is `true` for submenus, and depends on whether the parent
   * menubar item is focused otherwise.
   */
  hideOnHoverOutside?: boolean | ((event: MouseEvent) => boolean);
  /**
   * Whether to keep the menu's items registered even while the menu itself
   * is closed.
   */
  alwaysVisible?: boolean;
}

export type MenuProps = Props<"div", MenuOptions>;

/**
 * Renders a dropdown menu element that's controlled by a
 * [`MenuButton`](https://ariakit.com/reference/menu-button) component.
 * @see https://ariakit.com/components/menu
 * @example
 * ```jsx {3-6}
 * <MenuProvider>
 *   <MenuButton>Edit</MenuButton>
 *   <Menu>
 *     <MenuItem>Undo</MenuItem>
 *     <MenuItem>Redo</MenuItem>
 *   </Menu>
 * </MenuProvider>
 * ```
 */
export const Menu = forwardRef<HTMLDivElement, MenuProps>(function Menu(
  {
    store,
    modal = false,
    portal = modal,
    hideOnEscape = true,
    autoFocusOnShow = true,
    hideOnHoverOutside,
    alwaysVisible,
    ...rest
  },
  ref,
) {
  const context = useMenuProviderContext();
  const resolvedStore = store || context;
  // Real implementation delegates to useMenu() -> useHovercard() ->
  // usePopover() -> ... and renders through createDialogComponent(). None of
  // that positioning/focus/portal machinery affects the prop shape, so it's
  // stubbed to a plain div here.
  void resolvedStore;
  void portal;
  void hideOnEscape;
  void autoFocusOnShow;
  void hideOnHoverOutside;
  void alwaysVisible;
  return <div ref={ref} role="menu" {...rest} />;
});
