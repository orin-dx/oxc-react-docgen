import {
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  type CSSProperties,
  type Ref
} from "react";
import type { PanelProps } from "./types";
import { usePanelImperativeHandle } from "./usePanelImperativeHandle";

/**
 * react-resizable-panels — bvaughn/react-resizable-panels Panel (simplified fixture)
 *
 * Adapted from the real source (MIT license):
 *   https://github.com/bvaughn/react-resizable-panels/blob/main/lib/components/panel/Panel.tsx
 *
 * NOTE: as of this writing, the real `Panel` is *not* `React.forwardRef` — it's
 * a plain function component that accepts a custom-named `panelRef` prop
 * (typed `Ref<PanelImperativeHandle | null>`, see ./types.ts) and wires it up
 * internally via `usePanelImperativeHandle` → `useImperativeHandle`. A second,
 * separate `elementRef` prop carries the DOM node ref. Both are ordinary named
 * props — neither is the special `ref` prop — and this fixture preserves that
 * structure verbatim, since it's the point under test.
 *
 * Stubbed out: `useGroupContext` (parent Group registration, layout
 * measurement, `orientation`/`getPanelStyles`), the `useSyncExternalStore`
 * subscription to group-computed flex styles, `useMergedRefs` /
 * `useStableCallback` / `useStableObject` (trivial inlined stand-ins below),
 * and the `registerPanel` / `updatePanelProps` group-registration effect —
 * none of that affects the props/ref shape this fixture exercises, only how
 * panel sizing is computed at runtime against sibling panels.
 */

function useMergedRefs<T>(...refs: Array<Ref<T> | null | undefined>): Ref<T> {
  return (value: T | null) => {
    for (const ref of refs) {
      if (typeof ref === "function") {
        ref(value);
      } else if (ref != null) {
        (ref as { current: T | null }).current = value;
      }
    }
  };
}

/**
 * A Panel wraps resizable content and can be configured with min/max size constraints and collapsible behavior.
 *
 * Panel elements always include the following attributes:
 *
 * ```html
 * <div data-panel data-testid="panel-id-prop" id="panel-id-prop">
 * ```
 *
 * ⚠️ Panel elements must be direct DOM children of their parent Group elements.
 */
export function Panel({
  children,
  className,
  collapsedSize = "0%",
  collapsible = false,
  defaultSize,
  disabled,
  elementRef: elementRefProp,
  groupResizeBehavior = "preserve-relative-size",
  id: idProp,
  maxSize = "100%",
  minSize = "0%",
  onResize,
  panelRef,
  style,
  ...rest
}: PanelProps) {
  const generatedId = useId();
  const id = idProp !== undefined ? String(idProp) : generatedId;

  const elementRef = useRef<HTMLDivElement | null>(null);
  const mergedRef = useMergedRefs(elementRef, elementRefProp);

  void onResize;
  void groupResizeBehavior;

  // Register Panel with parent Group (stubbed — no group context installed).
  useLayoutEffect(() => {
    void collapsedSize;
    void collapsible;
    void maxSize;
    void minSize;
    void disabled;
  }, [collapsedSize, collapsible, maxSize, minSize, disabled]);

  usePanelImperativeHandle(id, panelRef);

  // Real implementation subscribes to group-computed flex styles via
  // useSyncExternalStore; simplified here to a direct computation from
  // defaultSize since no group context is installed in this fixture.
  const panelStyles: CSSProperties = useMemo(() => {
    if (defaultSize !== undefined) {
      return {
        flexGrow: undefined,
        flexShrink: undefined,
        flexBasis: defaultSize
      };
    }
    return { flexGrow: 1 };
  }, [defaultSize]);

  return (
    <div
      {...rest}
      data-disabled={disabled || undefined}
      data-panel
      data-testid={id}
      id={id}
      ref={mergedRef}
      style={{
        display: "flex",
        flexBasis: 0,
        flexShrink: 1,
        overflow: "visible",

        ...panelStyles
      }}
    >
      <div
        className={className}
        style={{
          maxHeight: "100%",
          maxWidth: "100%",
          flexGrow: 1,
          overflow: "auto",

          ...style
        }}
      >
        {children}
      </div>
    </div>
  );
}

// See https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Function/displayName
Panel.displayName = "Panel";
