/**
 * Ariakit — @ariakit/react MenuItem (simplified fixture)
 *
 * Adapted from ariakit/ariakit (MIT license), the real source file:
 *   https://github.com/ariakit/ariakit/blob/main/packages/ariakit-react-components/src/menu/menu-item.tsx
 *
 * `MenuItem` is the deepest level kept in this fixture (Provider -> Menu ->
 * MenuButton -> MenuItem is 4 levels sharing one store type). It reads the
 * *scoped* context deliberately (`useMenuScopedContext(true)` below, mapped
 * to `useMenuScopedContext` in ./menu-context.ts) rather than the unscoped
 * `MenuProvider` context that `Menu`/`MenuButton` read — real Ariakit does
 * this so a `MenuItem` that's also acting as a submenu button binds to the
 * *nearest* menu/menubar store rather than skipping to an ancestor
 * `MenuProvider`. It also reads `MenuListHiddenContext`, a second, unrelated
 * context from the same shared file. Together these two context reads are
 * the other half of the cross-contamination check this fixture exists for:
 * does resolving `MenuItem`'s props accidentally pick up `Menu`'s or
 * `MenuButton`'s prop shape because they all pull from ./menu-context.ts?
 *
 * Real `MenuItemOptions extends CompositeItemOptions<T>,
 * CompositeHoverOptions<T>` (unfetched/out-of-scope Composite primitives)
 * plus its own `store` and `hideOnClick` fields. Flattened here to
 * `store`/`hideOnClick` (own, verbatim) plus `preventScrollOnKeyDown`/
 * `focusOnHover`/`blurOnHoverEnd`, the real ancestor-package fields
 * `useMenuItem()` actually destructures off `props` in the source file
 * above.
 */
import type { MouseEvent } from "react";
import { forwardRef, useContext } from "react";
import {
  MenuListHiddenContext,
  useMenuScopedContext,
  type MenuStore,
  type Props,
} from "./menu-context.ts";

export interface MenuItemOptions {
  /**
   * Object returned by the
   * [`useMenuStore`](https://ariakit.com/reference/use-menu-store) or
   * `useMenubarStore` hooks. If not provided, the closest
   * [`Menu`](https://ariakit.com/reference/menu),
   * [`MenuList`](https://ariakit.com/reference/menu-list), or
   * `Menubar`/`MenubarProvider` component's context will be used.
   */
  store?: MenuStore;
  /**
   * Determines if the menu should hide when this item is clicked.
   *
   * **Note**: This behavior isn't triggered if this menu item is rendered as
   * a link and modifier keys are used to either open the link in a new tab
   * or download it.
   *
   * Live examples:
   * - [Sliding Menu](https://ariakit.com/examples/menu-slide)
   * @default true
   */
  hideOnClick?: boolean | ((event: MouseEvent<HTMLElement>) => boolean);
  /**
   * Whether pressing an arrow key while this item is active should prevent
   * the default browser scroll behavior.
   * @default true
   */
  preventScrollOnKeyDown?: boolean;
  /**
   * Whether hovering over this item should move focus to it. Defaults to
   * `true`, except within a menubar where an item is already expanded.
   */
  focusOnHover?: boolean | ((event: MouseEvent<HTMLElement>) => boolean);
  /**
   * Whether the menu container should receive focus when the pointer stops
   * hovering over this item. Defaults to `true` only when this item is
   * inside a menu (as opposed to a menubar).
   */
  blurOnHoverEnd?: boolean | ((event: MouseEvent<HTMLElement>) => boolean);
}

export type MenuItemProps = Props<"div", MenuItemOptions>;

/**
 * Renders a menu item inside
 * [`MenuList`](https://ariakit.com/reference/menu-list) or
 * [`Menu`](https://ariakit.com/reference/menu) components.
 * @see https://ariakit.com/components/menu
 * @example
 * ```jsx {4-5}
 * <MenuProvider>
 *   <MenuButton>Edit</MenuButton>
 *   <Menu>
 *     <MenuItem>Undo</MenuItem>
 *     <MenuItem>Redo</MenuItem>
 *   </Menu>
 * </MenuProvider>
 * ```
 */
export const MenuItem = forwardRef<HTMLDivElement, MenuItemProps>(
  function MenuItem(
    {
      store,
      hideOnClick = true,
      preventScrollOnKeyDown = true,
      focusOnHover,
      blurOnHoverEnd,
      ...rest
    },
    ref,
  ) {
    // Only the scoped menu context is read here — see header comment.
    const menuContext = useMenuScopedContext(true);
    const menuListHidden = useContext(MenuListHiddenContext);
    const resolvedStore = store || menuContext;
    // Real implementation wires up onClick (auto-hide-on-click) plus
    // useCompositeItem/useCompositeHover for roving-tabindex focus
    // management and hover-driven active state. Stubbed to a plain div.
    void resolvedStore;
    void preventScrollOnKeyDown;
    void focusOnHover;
    void blurOnHoverEnd;
    void menuListHidden;
    return <div ref={ref} role="menuitem" {...rest} />;
  },
);
