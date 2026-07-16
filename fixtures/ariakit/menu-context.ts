/**
 * Ariakit — @ariakit/react Menu context + store types (simplified fixture)
 *
 * Adapted from ariakit/ariakit (MIT license), the real source files:
 *   https://github.com/ariakit/ariakit/blob/main/packages/ariakit-react-components/src/menu/menu-context.tsx
 *   https://github.com/ariakit/ariakit/blob/main/packages/ariakit-react-components/src/menu/menu-store.ts
 *
 * This is the shared substrate every level of the Menu family builds on: one
 * scoped React Context, created once here, threaded through
 * MenuProvider -> Menu -> MenuButton -> MenuItem (see the sibling files in
 * this directory). Real Ariakit compound components (Menu, Select, Combobox,
 * Dialog, Hovercard, ...) all follow this exact shape — a `*-context.tsx`
 * defining `use*Context`/`use*ScopedContext`/`use*ProviderContext` plus a
 * `*ContextProvider`, and a `*-store.ts` defining the store's option/state
 * types — which is why it's the right file to prove deep context-sharing
 * against, rather than a shallower single-context family.
 *
 * Trimmed for this fixture:
 *  - The real menu-context.tsx also re-exports the Menubar family's context
 *    under `MenuBarContext*` aliases (deprecated pass-throughs). Dropped —
 *    Menubar is a sibling family, out of scope here.
 *  - The real `createStoreContext()` helper (from `@ariakit/react-utils`,
 *    not installed in this repo) composes the Menu context with the
 *    Composite and Hovercard contexts it's nested inside of. Stubbed below
 *    with plain `React.createContext`, which preserves the
 *    scoped-vs-unscoped distinction (`useMenuContext` vs
 *    `useMenuScopedContext`) that MenuItem actually depends on, without the
 *    cross-package composition machinery.
 *  - `MenuStoreOptions` here keeps only its own fields (`values`,
 *    `defaultValues`, `setValues`, `combobox`, `parent`, `menubar`). The real
 *    interface also extends `CompositeStoreOptions` and
 *    `HovercardStoreOptions` from sibling primitive packages (`orientation`,
 *    `placement`, `virtualFocus`, `timeout`, ...) that are shared across many
 *    non-Menu Ariakit families (Select, Combobox, Popover) and out of scope
 *    for this fixture.
 *  - `useMenuStore`'s real implementation composes Composite + Hovercard
 *    store slices via `@ariakit/react-store`. Stubbed to a plain
 *    `useState`-backed values bag — the exported *types* are what this
 *    fixture exercises, not the state machine.
 *  - `Props<T, O>` and `PickRequired<T, K>` stand in for the real generic
 *    helpers of the same name in `@ariakit/react-utils`/`@ariakit/utils`
 *    (not installed), the same way `fixtures/ark-ui/types.ts` stands in for
 *    `@zag-js/*`. Every component in this family is polymorphic over its
 *    rendered tag in real Ariakit; that generic-tag pattern itself is already
 *    covered by `fixtures/headlessui`, so here `Props<T, O>` is fixed to a
 *    single concrete DOM element per component instead of staying generic
 *    over `T`.
 */
import type { ComponentPropsWithoutRef, ElementType } from "react";
import { createContext, useContext, useState } from "react";

export type Props<T extends ElementType, O = {}> = O &
  Omit<ComponentPropsWithoutRef<T>, keyof O>;

export type PickRequired<T, K extends keyof T> = T & Required<Pick<T, K>>;

// ---- packages/ariakit-react-components/src/menu/menu-store.ts (trimmed) ----

export type MenuStoreValues = Record<string, string | string[] | boolean>;

export interface MenuStoreState<T extends MenuStoreValues = MenuStoreValues> {
  /** The values of checkbox and radio menu items wrapped in this menu. */
  values: T;
}

export interface MenuStoreFunctions<
  T extends MenuStoreValues = MenuStoreValues,
> {
  setValues: (values: T | ((prevValues: T) => T)) => void;
}

export interface MenuStoreOptions<
  T extends MenuStoreValues = MenuStoreValues,
> {
  /**
   * A callback that gets called when the `values` state changes.
   *
   * Live examples:
   * - [MenuItemCheckbox](https://ariakit.com/examples/menu-item-checkbox)
   * - [Submenu with
   *   Combobox](https://ariakit.com/examples/menu-nested-combobox)
   */
  setValues?: (values: MenuStoreState<T>["values"]) => void;
  /**
   * A reference to a [combobox
   * store](https://ariakit.com/reference/use-combobox-store). It's
   * automatically set when composing [Menu with
   * Combobox](https://ariakit.com/examples/menu-combobox).
   */
  combobox?: MenuStore | null;
  /**
   * A reference to a parent menu store. It's automatically set when nesting
   * menus in the React tree. You should manually set this if menus aren't
   * nested in the React tree.
   *
   * Live examples:
   * - [Menubar](https://ariakit.com/components/menubar)
   * - [Submenu](https://ariakit.com/examples/menu-nested)
   */
  parent?: MenuStore | null;
  /**
   * A reference to a [menubar
   * store](https://ariakit.com/reference/use-menubar-store). It's
   * automatically set when rendering menus inside a
   * [`Menubar`](https://ariakit.com/reference/menubar) in the React tree.
   */
  menubar?: MenuStore | null;
}

export interface MenuStoreProps<T extends MenuStoreValues = MenuStoreValues>
  extends MenuStoreOptions<T> {
  values?: T;
  defaultValues?: T;
}

export interface MenuStore<T extends MenuStoreValues = MenuStoreValues>
  extends MenuStoreFunctions<T>,
    MenuStoreState<T> {
  parent?: MenuStore | null;
  menubar?: MenuStore | null;
  combobox?: MenuStore | null;
}

export function useMenuStore(props: MenuStoreProps = {}): MenuStore {
  const [values, setValues] = useState(props.values ?? props.defaultValues ?? {});
  return {
    values,
    setValues,
    parent: props.parent ?? null,
    menubar: props.menubar ?? null,
    combobox: props.combobox ?? null,
  };
}

// ---- packages/ariakit-react-components/src/menu/menu-context.tsx (trimmed) ----

const MenuReactContext = createContext<MenuStore | undefined>(undefined);
const MenuScopedReactContext = createContext<MenuStore | undefined>(undefined);

/**
 * Returns the menu store from the nearest menu container.
 */
export function useMenuContext() {
  return useContext(MenuReactContext);
}

export function useMenuScopedContext(scoped?: boolean) {
  return useContext(scoped ? MenuScopedReactContext : MenuReactContext);
}

export function useMenuProviderContext() {
  return useContext(MenuReactContext);
}

export const MenuContextProvider = MenuReactContext.Provider;

export const MenuScopedContextProvider = MenuScopedReactContext.Provider;

/**
 * Whether the enclosing menu list is currently hidden (e.g. a closed menu
 * rendered without `unmountOnHide`). `MenuItem` uses it to skip registering
 * items that aren't shown yet.
 */
export const MenuListHiddenContext = createContext(false);
