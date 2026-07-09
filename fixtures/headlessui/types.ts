/**
 * Headless UI — @headlessui/react shared prop-polymorphism helper (verbatim fixture)
 *
 * Adapted from tailwindlabs/headlessui (MIT license), the real source file:
 * https://github.com/tailwindlabs/headlessui/blob/main/packages/%40headlessui-react/src/types.ts
 *
 * This is the type every Headless UI component builds its public props from —
 * the classic `as="button"`-style polymorphic prop convention (as opposed to
 * Base UI's `render`-prop or Radix's `asChild`). `Props<TTag, TSlot,
 * TOmittableProps, Overrides>` is kept byte-for-byte identical to upstream:
 * it strips the props Headless UI controls itself (`as`, `children`, `refName`,
 * `className`, plus whatever the caller lists in `TOmittableProps`) off of
 * `React.ComponentProps<TTag>`, re-adds `as`/`children`/`refName` typed against
 * `TTag`/`TSlot`, conditionally re-adds a slot-aware `className` override only
 * when `TTag` actually has one, and finally layers `Overrides` on top. See
 * ./Listbox.tsx for how each component instantiates this with its own tag
 * default, render-prop slot shape, and controlled-prop overrides.
 */

export type ReactTag = keyof React.JSX.IntrinsicElements | React.JSXElementConstructor<any>

export type Expand<T> = T extends infer O ? { [K in keyof O]: O[K] } : never

export type PropsOf<TTag extends ReactTag> = TTag extends React.ElementType
  ? Omit<React.ComponentProps<TTag>, 'ref'>
  : never

type PropsWeControl = 'as' | 'children' | 'refName' | 'className'

// Resolve the props of the component, but ensure to omit certain props that we control
type CleanProps<TTag extends ReactTag, TOmittableProps extends PropertyKey = never> = Omit<
  PropsOf<TTag>,
  TOmittableProps | PropsWeControl
>

// Add certain props that we control
type OurProps<TTag extends ReactTag, TSlot> = {
  as?: TTag
  children?: React.ReactNode | ((bag: TSlot) => React.ReactElement)
  refName?: string
}

type HasProperty<T extends object, K extends PropertyKey> = T extends never
  ? never
  : K extends keyof T
    ? true
    : never

// Conditionally override the `className`, to also allow for a function
// if and only if the PropsOf<TTag> already defines `className`.
// This will allow us to have a TS error on as={Fragment}
type ClassNameOverride<TTag extends ReactTag, TSlot = {}> =
  // Order is important here, because `never extends true` is `true`...
  true extends HasProperty<PropsOf<TTag>, 'className'>
    ? { className?: PropsOf<TTag>['className'] | ((bag: TSlot) => string) }
    : {}

// Provide clean TypeScript props, which exposes some of our custom APIs.
export type Props<
  TTag extends ReactTag,
  TSlot = {},
  TOmittableProps extends PropertyKey = never,
  Overrides = {},
> = CleanProps<TTag, TOmittableProps | keyof Overrides> &
  OurProps<TTag, TSlot> &
  ClassNameOverride<TTag, TSlot> &
  Overrides

export type EnsureArray<T> = T extends any[] ? T : Expand<T>[]
