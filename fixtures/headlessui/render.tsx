/**
 * Headless UI — @headlessui/react rendering plumbing (simplified fixture)
 *
 * Adapted from tailwindlabs/headlessui (MIT license), the real source file:
 * https://github.com/tailwindlabs/headlessui/blob/main/packages/%40headlessui-react/src/utils/render.ts
 *
 * Kept real: the `RenderFeatures` flag enum and `PropsForFeatures<T>` (which
 * conditionally adds `static`/`unmount` props depending on which flags a
 * component opts into — see `ListboxOptionsProps` in ./Listbox.tsx), plus the
 * `HasDisplayName`/`RefProp<T>` helper types and the `forwardRefWithAs` /
 * `mergeProps` signatures, all verbatim.
 *
 * Stubbed out: the real `render()`/`_render()` (~200 lines) merge `ourProps`
 * with `theirProps`, resolve the render-prop `children` function, handle
 * `as={Fragment}` passthrough via `cloneElement`, compute `data-*` state
 * attributes, and merge/guard event handlers when `disabled`. None of that
 * affects the *prop types* this fixture exists to exercise, so `useRender`
 * below collapses to a minimal `createElement(as ?? defaultTag, ...)` call.
 */
import * as React from 'react'

export enum RenderFeatures {
  /** No features at all */
  None = 0,

  /**
   * When used, this will allow us to use one of the render strategies.
   *
   * **The render strategies are:**
   *    - **Unmount**   _(Will unmount the component.)_
   *    - **Hidden**    _(Will hide the component using the [hidden] attribute.)_
   */
  RenderStrategy = 1,

  /**
   * When used, this will allow the user of our component to be in control. This can be used when
   * you want to transition based on some state.
   */
  Static = 2,
}

type UnionToIntersection<T> = (T extends any ? (x: T) => any : never) extends (x: infer R) => any
  ? R
  : never

type PropsForFeature<
  TPassedInFeatures extends RenderFeatures,
  TForFeature extends RenderFeatures,
  TProps,
> = TPassedInFeatures extends TForFeature ? TProps : {}

export type PropsForFeatures<T extends RenderFeatures> = UnionToIntersection<
  | PropsForFeature<T, RenderFeatures.Static, { static?: boolean }>
  | PropsForFeature<T, RenderFeatures.RenderStrategy, { unmount?: boolean }>
>

export type HasDisplayName = {
  displayName: string
}

export type RefProp<T extends Function> = T extends (props: any, ref: React.Ref<infer RefType>) => any
  ? { ref?: React.Ref<RefType> }
  : never

/**
 * This is a hack, but basically we want to keep the full 'API' of the component, but we do want to
 * wrap it in a forwardRef so that we _can_ passthrough the ref
 */
export function forwardRefWithAs<T extends { name: string; displayName?: string }>(
  component: T,
): T & { displayName: string } {
  return Object.assign(React.forwardRef(component as any) as any, {
    displayName: component.displayName ?? component.name,
  })
}

// TODO: add proper return type, but this is not exposed as public API so it's fine for now
export function mergeProps<T extends Record<string, unknown>[]>(...listOfProps: T) {
  return Object.assign({}, ...listOfProps)
}

// Minimal stand-in for the real `render()`: resolves `as` against the
// component's default tag and spreads `ourProps`/`theirProps` onto it. The
// real implementation also resolves a function-`children` render prop against
// `slot`, which is preserved here since it's part of the polymorphic surface.
export function useRender() {
  return React.useCallback(function render(args: {
    ourProps: Record<string, unknown>
    theirProps: Record<string, unknown> & { as?: React.ElementType; children?: unknown }
    slot?: unknown
    defaultTag: React.ElementType
    features?: RenderFeatures
    visible?: boolean
    name: string
  }) {
    const { ourProps, theirProps, slot, defaultTag, visible = true } = args
    if (!visible) return null
    const { as: Component = defaultTag, children, ...rest } = theirProps
    const resolvedChildren = typeof children === 'function' ? (children as any)(slot ?? {}) : children
    return React.createElement(Component, { ...rest, ...ourProps }, resolvedChildren)
  }, [])
}
