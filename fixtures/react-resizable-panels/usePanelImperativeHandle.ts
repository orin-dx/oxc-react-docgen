import { useImperativeHandle, useLayoutEffect, useRef, type Ref } from "react";
import type { PanelImperativeHandle } from "./types";

/**
 * usePanelImperativeHandle — bvaughn/react-resizable-panels (simplified fixture)
 *
 * Adapted from the real source (MIT license):
 *   https://github.com/bvaughn/react-resizable-panels/blob/main/lib/components/panel/usePanelImperativeHandle.ts
 *
 * Kept verbatim: the `useImperativeHandle(panelRef, () => imperativePanelRef.current, [])`
 * call — a hand-authored method-object ref (not a DOM node) is exactly what this
 * fixture exists to exercise.
 *
 * Stubbed: `getImperativePanelMethods` (real impl reaches into live panel-group
 * state — resize context, sibling panel sizes — to build working
 * collapse/expand/getSize/isCollapsed/resize implementations) is replaced with
 * inline no-op stand-ins, since none of that affects the ref's *type* shape,
 * only its runtime behavior. `useIsomorphicLayoutEffect` (an SSR-safe alias) is
 * replaced with `useLayoutEffect` directly.
 */

const NOOP_FUNCTION = () => {};

export function usePanelImperativeHandle(
  panelId: string,
  panelRef: Ref<PanelImperativeHandle> | undefined
) {
  const imperativePanelRef = useRef<PanelImperativeHandle>({
    collapse: NOOP_FUNCTION,
    expand: NOOP_FUNCTION,
    getSize: () => ({
      asPercentage: 0,
      inPixels: 0
    }),
    isCollapsed: () => false,
    resize: NOOP_FUNCTION
  });

  useImperativeHandle(panelRef, () => imperativePanelRef.current, []);

  // Real implementation re-wires these methods against live panel-group state
  // on every render via `getImperativePanelMethods({ groupId, panelId })`;
  // stubbed here since panel-group internals aren't installed in this fixture.
  useLayoutEffect(() => {
    void panelId;
  });
}
