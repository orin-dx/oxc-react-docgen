/**
 * Ariakit — @ariakit/react MenuButton (simplified fixture)
 *
 * Adapted from ariakit/ariakit (MIT license), the real source file:
 *   https://github.com/ariakit/ariakit/blob/main/packages/ariakit-react-components/src/menu/menu-button.tsx
 *
 * `MenuButton` is a sibling of `Menu` under the same `MenuProvider`, but it
 * also *re-provides* the store to its own subtree via
 * `MenuContextProvider` (see the real `useWrapElement(...)` call this
 * mirrors) — that's the "submenu button" case, where a `MenuItem` renders a
 * nested `MenuButton`/`Menu` pair sharing a *child* store scoped under the
 * parent menu's context. This is the crux of the cross-contamination risk
 * this fixture exists to check: `MenuButton` both *reads* the context Menu
 * reads and *writes* a new one, using the exact same `MenuContextProvider`
 * symbol imported from ./menu-context.ts.
 *
 * Real `MenuButtonOptions extends HovercardAnchorOptions<T>,
 * PopoverDisclosureOptions<T>, CompositeTypeaheadOptions<T>` (all
 * unfetched/out-of-scope primitive packages) plus its own `store` and
 * `typeahead` fields. Flattened here to `store`/`typeahead` (own, verbatim)
 * plus `accessibleWhenDisabled`/`focusable`/`showOnHover`, the real
 * ancestor-package fields `useMenuButton()` actually destructures off
 * `props` in the source file above.
 */
import type { MouseEvent } from "react";
import { forwardRef } from "react";
import {
  MenuContextProvider,
  useMenuProviderContext,
  type MenuStore,
  type Props,
} from "./menu-context.ts";

export interface MenuButtonOptions {
  /**
   * Object returned by the
   * [`useMenuStore`](https://ariakit.com/reference/use-menu-store) hook. If
   * not provided, the closest
   * [`MenuProvider`](https://ariakit.com/reference/menu-provider)
   * component's context will be used.
   */
  store?: MenuStore;
  /**
   * Determines whether pressing a character key while focusing on the
   * [`MenuButton`](https://ariakit.com/reference/menu-button) should move
   * focus to the [`MenuItem`](https://ariakit.com/reference/menu-item)
   * starting with that character.
   *
   * By default, it's `true` for menu buttons in a
   * [`Menubar`](https://ariakit.com/reference/menubar), but `false` for
   * other menu buttons.
   */
  typeahead?: boolean;
  /**
   * Whether the menu button should be focusable even when it's disabled.
   */
  accessibleWhenDisabled?: boolean;
  /**
   * Whether the menu button can receive focus, including via keyboard tab
   * order, independent of the native `disabled` attribute.
   */
  focusable?: boolean;
  /**
   * Whether hovering over the menu button should show its menu, and after
   * how long (in milliseconds). Also accepts a callback for custom logic.
   */
  showOnHover?: boolean | number | ((event: MouseEvent) => boolean);
}

export type MenuButtonProps = Props<"button", MenuButtonOptions>;

/**
 * Renders a menu button that toggles the visibility of a
 * [`Menu`](https://ariakit.com/reference/menu) component when clicked or
 * when using arrow keys.
 * @see https://ariakit.com/components/menu
 * @example
 * ```jsx {2}
 * <MenuProvider>
 *   <MenuButton>Edit</MenuButton>
 *   <Menu>
 *     <MenuItem>Undo</MenuItem>
 *     <MenuItem>Redo</MenuItem>
 *   </Menu>
 * </MenuProvider>
 * ```
 */
export const MenuButton = forwardRef<HTMLButtonElement, MenuButtonProps>(
  function MenuButton(
    { store, typeahead, accessibleWhenDisabled, focusable, showOnHover, ...rest },
    ref,
  ) {
    const context = useMenuProviderContext();
    const resolvedStore = store || context;
    // Real implementation wires up onFocus/onKeyDown/onClick handlers that
    // open the menu, plus useHovercardAnchor/usePopoverDisclosure/
    // useCompositeTypeahead. Stubbed to a plain button; only the
    // context-(re)providing structure and prop shape are under test.
    void typeahead;
    void accessibleWhenDisabled;
    void focusable;
    void showOnHover;
    return (
      <MenuContextProvider value={resolvedStore}>
        <button ref={ref} type="button" aria-haspopup="menu" {...rest} />
      </MenuContextProvider>
    );
  },
);
