/**
 * Ariakit — @ariakit/react MenuProvider (near-verbatim fixture)
 *
 * Adapted from ariakit/ariakit (MIT license), the real source file:
 *   https://github.com/ariakit/ariakit/blob/main/packages/ariakit-react-components/src/menu/menu-provider.tsx
 *
 * The real file (42 lines) is reproduced near-verbatim below — this is the
 * root of the compound family: it creates the shared menu store and provides
 * it to every descendant via ./menu-context.ts's `MenuContextProvider`. The
 * only changes are the import paths (real code splits store types into
 * ./menu-store.ts; this fixture folds them into ./menu-context.ts, see that
 * file's header) and dropping the `@ariakit/utils` import for `PickRequired`
 * in favor of the local stand-in of the same name.
 */
import type { ReactElement, ReactNode } from "react";
import {
  MenuContextProvider,
  useMenuStore,
  type MenuStoreProps,
  type MenuStoreValues,
  type PickRequired,
} from "./menu-context.ts";

type Values = MenuStoreValues;

/**
 * Provides a menu store to [Menu](https://ariakit.com/components/menu)
 * components.
 * @see https://ariakit.com/components/menu
 * @example
 * ```jsx
 * <MenuProvider placement="top">
 *   <MenuButton>Edit</MenuButton>
 *   <Menu>
 *     <MenuItem>Undo</MenuItem>
 *     <MenuItem>Redo</MenuItem>
 *   </Menu>
 * </MenuProvider>
 * ```
 */

export function MenuProvider<T extends Values = Values>(
  props: PickRequired<MenuProviderProps<T>, "values" | "defaultValues">,
): ReactElement;

export function MenuProvider(props?: MenuProviderProps): ReactElement;

export function MenuProvider(props: MenuProviderProps = {}) {
  const store = useMenuStore(props);
  return (
    <MenuContextProvider value={store}>{props.children}</MenuContextProvider>
  );
}

export interface MenuProviderProps<T extends Values = Values>
  extends MenuStoreProps<T> {
  children?: ReactNode;
}
