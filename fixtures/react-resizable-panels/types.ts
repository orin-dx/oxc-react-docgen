import type { CSSProperties, HTMLAttributes, Ref } from "react";

/**
 * react-resizable-panels — bvaughn/react-resizable-panels (simplified fixture)
 *
 * Adapted from the real source (MIT license):
 *   https://github.com/bvaughn/react-resizable-panels/blob/main/lib/components/panel/types.ts
 *
 * Kept verbatim: `PanelImperativeHandle` (the hand-authored method-object type
 * exposed via `useImperativeHandle` — not a DOM node) and the `panelRef` /
 * `elementRef` prop shape on `PanelProps`. Trimmed: `RegisteredPanel`,
 * `PanelConstraints`, `PanelConstraintProps`, `SizeUnit` — internal
 * panel-group registration bookkeeping unrelated to the public props/ref
 * surface this fixture exercises.
 */

export type PanelSize = {
  asPercentage: number;
  inPixels: number;
};

export type GroupResizeBehavior =
  | "preserve-relative-size"
  | "preserve-pixel-size";

/**
 * Imperative Panel API
 *
 * ℹ️ The `usePanelRef` and `usePanelCallbackRef` hooks are exported for convenience use in TypeScript projects.
 */
export interface PanelImperativeHandle {
  /**
   * Collapse the Panel to it's `collapsedSize`.
   *
   * ⚠️ This method will do nothing if the Panel is not `collapsible` or if it is already collapsed.
   */
  collapse: () => void;

  /**
   * Expand a collapsed Panel to its most recent size.
   *
   * ⚠️ This method will do nothing if the Panel is not currently collapsed.
   */
  expand: () => void;

  /**
   * Get the current size of the Panel in pixels as well as a percentage of the parent group (0..100).
   *
   * @return Panel size (in pixels and as a percentage of the parent group)
   */
  getSize: () => {
    asPercentage: number;
    inPixels: number;
  };

  /**
   * The Panel is currently collapsed.
   */
  isCollapsed: () => boolean;

  /**
   * Update the Panel's size.
   *
   * Size can be in the following formats:
   * - Percentage of the parent Group (0..100)
   * - Pixels
   * - Relative font units (em, rem)
   * - Viewport relative units (vh, vw)
   *
   * ℹ️ Numeric values are assumed to be pixels.
   * Strings without explicit units are assumed to be percentages (0%..100%).
   *
   * @param size New panel size
   * @return Applied size (after validation)
   */
  resize: (size: number | string) => void;
}

type BasePanelAttributes = Omit<HTMLAttributes<HTMLDivElement>, "onResize">;

export type PanelProps = BasePanelAttributes & {
  /**
   * CSS class name.
   *
   * ⚠️ Class is applied to nested `HTMLDivElement` to avoid styles that interfere with Flex layout.
   */
  className?: string | undefined;

  /**
   * Panel size when collapsed; defaults to 0%.
   */
  collapsedSize?: number | string | undefined;

  /**
   * This panel can be collapsed.
   *
   * ℹ️ A collapsible panel will collapse when it's size is less than of the specified `minSize`
   */
  collapsible?: boolean | undefined;

  /**
   * Default size of Panel within its parent group; default is auto-assigned based on the total number of Panels.
   */
  defaultSize?: number | string | undefined;

  /**
   * When disabled, a panel cannot be resized either directly or indirectly (by resizing another panel).
   */
  disabled?: boolean | undefined;

  /**
   * Ref attached to the root `HTMLDivElement`.
   */
  elementRef?: Ref<HTMLDivElement | null> | undefined;

  /**
   * How should this Panel behave if the parent Group is resized?
   * Defaults to `preserve-relative-size`.
   */
  groupResizeBehavior?: GroupResizeBehavior | undefined;

  /**
   * Uniquely identifies this panel within the parent group.
   * Falls back to `useId` when not provided.
   */
  id?: string | number | undefined;

  /**
   * Maximum size of Panel within its parent group; defaults to `"100%"`.
   */
  maxSize?: number | string | undefined;

  /**
   * Minimum size of Panel within its parent group; defaults to 0%.
   */
  minSize?: number | string | undefined;

  /**
   * Called when panel sizes change.
   *
   * @param panelSize Panel size (both as a percentage of the parent Group and in pixels)
   * @param id Panel id (if one was provided as a prop)
   * @param prevPanelSize Previous panel size (will be undefined on mount)
   */
  onResize?:
    | ((
        panelSize: PanelSize,
        id: string | number | undefined,
        prevPanelSize: PanelSize | undefined
      ) => void)
    | undefined;

  /**
   * Exposes the following imperative API:
   * - `collapse(): void`
   * - `expand(): void`
   * - `getSize(): number`
   * - `isCollapsed(): boolean`
   * - `resize(size: number): void`
   *
   * ℹ️ The `usePanelRef` and `usePanelCallbackRef` hooks are exported for convenience use in TypeScript projects.
   */
  panelRef?: Ref<PanelImperativeHandle | null> | undefined;

  /**
   * CSS properties.
   *
   * ⚠️ Style is applied to nested `HTMLDivElement` to avoid styles that interfere with Flex layout.
   */
  style?: CSSProperties | undefined;
};

export type OnPanelResize = PanelProps["onResize"];
