/**
 * Ark UI / Zag.js — local stand-ins for cross-package plumbing (simplified fixture)
 *
 * `Select.tsx` (adapted from `chakra-ui/ark`'s `select-root.tsx`) pulls its prop
 * shapes from `@zag-js/select`, `@zag-js/collection`, `@zag-js/interact-outside`,
 * `@zag-js/types`, and this package's own `factory.ts`. None of those packages
 * are installed in this repo, so this file inlines the real type definitions
 * (trimmed of members this fixture doesn't exercise, e.g. `ListCollection`'s
 * ~30 grouping/filtering methods) so `Select.tsx` type-checks standalone while
 * keeping the real generic + polymorphic prop shapes intact.
 *
 * Sources (MIT license):
 *   chakra-ui/ark — packages/react/src/types.ts
 *   chakra-ui/ark — packages/react/src/components/factory.ts
 *   chakra-ui/zag — packages/utilities/collection/src/types.ts
 *   chakra-ui/zag — packages/utilities/interact-outside/src/index.ts
 *   chakra-ui/zag — packages/types/src/index.ts
 *   chakra-ui/zag — packages/machines/select/src/select.types.ts
 *   chakra-ui/ark — packages/react/src/components/presence/use-presence.ts
 */
import type { ComponentPropsWithoutRef, JSX } from "react"

// ---- packages/react/src/types.ts (ark, verbatim) ----
export type Assign<T, U> = Omit<T, keyof U> & U
export type Optional<T, K extends keyof T> = Pick<Partial<T>, K> & Omit<T, K>

// ---- packages/react/src/components/factory.ts (ark, verbatim types) ----
export interface PolymorphicProps {
  /**
   * Use the provided child element as the default rendered element, combining their props and behavior.
   */
  asChild?: boolean | undefined
}
export type HTMLProps<T extends keyof JSX.IntrinsicElements> = ComponentPropsWithoutRef<T>

// ---- packages/utilities/collection/src/types.ts (zag, verbatim) ----
export type CollectionItem = any

export interface CollectionMethods<T extends CollectionItem = CollectionItem> {
  /** The value of the item */
  itemToValue: (item: T) => string
  /** The label of the item */
  itemToString: (item: T) => string
  /** Whether the item is disabled */
  isItemDisabled: (item: T) => boolean
}

export interface CollectionOptions<T extends CollectionItem = CollectionItem> extends Partial<CollectionMethods<T>> {
  /** The options of the select */
  items: Iterable<T> | Readonly<Iterable<T>>
  /** Function to group items */
  groupBy?: ((item: T, index: number) => string) | undefined
  /** Function to sort items */
  groupSort?: ((a: string, b: string) => number) | string[] | "asc" | "desc" | undefined
}

/**
 * Simplified stand-in for the real `ListCollection` class in `@zag-js/collection`,
 * which implements ~30 methods for grouping, filtering, and indexing. Only the
 * shape needed to exercise `SelectProps<T>['collection']` is kept.
 */
export declare class ListCollection<T extends CollectionItem = CollectionItem> {
  constructor(options: CollectionOptions<T>)
  readonly size: number
  items: T[]
  first(): T | undefined
  last(): T | undefined
  at(index: number): T | undefined
  itemToValue(item: T): string
  itemToString(item: T): string
}

// ---- packages/utilities/interact-outside/src/index.ts (zag, verbatim shape) ----
export interface PointerDownOutsideEvent extends CustomEvent<{ originalEvent: PointerEvent }> {}
export interface FocusOutsideEvent extends CustomEvent<{ originalEvent: FocusEvent }> {}
export interface InteractOutsideEvent extends CustomEvent<{ originalEvent: PointerEvent | FocusEvent }> {}

export interface InteractOutsideHandlers {
  /** Function called when the pointer is pressed down outside the component */
  onPointerDownOutside?: ((event: PointerDownOutsideEvent) => void) | undefined
  /** Function called when the focus is moved outside the component */
  onFocusOutside?: ((event: FocusOutsideEvent) => void) | undefined
  /** Function called when an interaction happens outside the component */
  onInteractOutside?: ((event: InteractOutsideEvent) => void) | undefined
}

// ---- packages/types/src/index.ts (zag, verbatim) ----
export interface DirectionProperty {
  /**
   * The document's text/writing direction.
   * @default "ltr"
   */
  dir?: "ltr" | "rtl" | undefined
}

export interface CommonProperties {
  /** The unique identifier of the machine. */
  id: string
  /** A root node to correctly resolve document in custom environments. E.x.: Iframes, Electron. */
  getRootNode?: (() => ShadowRoot | Document | Node) | undefined
}

// ---- packages/machines/select/src/select.types.ts (zag, trimmed) ----
// The prop surface itself is kept verbatim (it's the interesting part); the
// callback-detail types and `PositioningOptions` are simplified since their
// internals don't affect prop-type extraction.
export interface IntlTranslations {
  clearTriggerLabel?: string | undefined
}

export type ElementIds = Partial<{
  root: string
  content: string
  control: string
  trigger: string
  clearTrigger: string
  label: string
  hiddenSelect: string
  positioner: string
  item: (id: string | number) => string
  itemGroup: (id: string | number) => string
  itemGroupLabel: (id: string | number) => string
}>

export interface ValueChangeDetails<T extends CollectionItem = CollectionItem> {
  value: string[]
  items: T[]
}

export interface HighlightChangeDetails<T extends CollectionItem = CollectionItem> {
  highlightedValue: string | null
  highlightedItem: T | null
  highlightedIndex: number
}

export interface OpenChangeDetails {
  open: boolean
  value: string[]
}

export interface ScrollToIndexDetails {
  index: number
  immediate?: boolean | undefined
  getElement: () => HTMLElement | null
}

export interface SelectionDetails {
  value: string
}

/** Simplified stand-in for `@zag-js/popper`'s `PositioningOptions`. */
export interface PositioningOptions {
  placement?: string | undefined
  offset?: { mainAxis?: number; crossAxis?: number } | undefined
}

export interface SelectProps<T extends CollectionItem = CollectionItem>
  extends DirectionProperty,
    CommonProperties,
    InteractOutsideHandlers {
  /** Specifies the localized strings that identifies the accessibility elements and their states */
  translations?: IntlTranslations | undefined
  /** The item collection */
  collection: ListCollection<T>
  /** The ids of the elements in the select. Useful for composition. */
  ids?: ElementIds | undefined
  /** The `name` attribute of the underlying select. */
  name?: string | undefined
  /** The associate form of the underlying select. */
  form?: string | undefined
  /** The autocomplete attribute for the hidden select. Enables browser autofill (e.g. "address-level1" for state). */
  autoComplete?: string | undefined
  /** Whether the select is disabled */
  disabled?: boolean | undefined
  /** Whether the select is invalid */
  invalid?: boolean | undefined
  /** Whether the select is read-only */
  readOnly?: boolean | undefined
  /** Whether the select is required */
  required?: boolean | undefined
  /**
   * Whether the select should close after an item is selected
   * @default true
   */
  closeOnSelect?: boolean | undefined
  /** Function called when an item is selected */
  onSelect?: ((details: SelectionDetails) => void) | undefined
  /** The callback fired when the highlighted item changes. */
  onHighlightChange?: ((details: HighlightChangeDetails<T>) => void) | undefined
  /** The callback fired when the selected item changes. */
  onValueChange?: ((details: ValueChangeDetails<T>) => void) | undefined
  /** Function called when the popup is opened */
  onOpenChange?: ((details: OpenChangeDetails) => void) | undefined
  /** The positioning options of the menu. */
  positioning?: PositioningOptions | undefined
  /** The controlled keys of the selected items */
  value?: string[] | undefined
  /**
   * The initial default value of the select when rendered.
   * Use when you don't need to control the value of the select.
   */
  defaultValue?: string[] | undefined
  /** The controlled key of the highlighted item */
  highlightedValue?: string | null | undefined
  /**
   * The initial value of the highlighted item when opened.
   * Use when you don't need to control the highlighted value of the select.
   */
  defaultHighlightedValue?: string | null | undefined
  /**
   * Whether to loop the keyboard navigation through the options
   * @default false
   */
  loopFocus?: boolean | undefined
  /** Whether to allow multiple selection */
  multiple?: boolean | undefined
  /** Whether the select menu is open */
  open?: boolean | undefined
  /** Whether the select's open state is controlled by the user */
  defaultOpen?: boolean | undefined
  /** Function to scroll to a specific index */
  scrollToIndexFn?: ((details: ScrollToIndexDetails) => void) | undefined
  /**
   * Whether the select is a composed with other composite widgets like tabs or combobox
   * @default true
   */
  composite?: boolean | undefined
  /**
   * Whether the value can be cleared by clicking the selected item.
   *
   * **Note:** this is only applicable for single selection
   */
  deselectable?: boolean | undefined
}

// ---- packages/react/src/components/presence/use-presence.ts (ark, trimmed) ----
// Real type extends `@zag-js/presence`'s `Props` and `RenderStrategyProps`;
// both are small enough to inline directly here rather than stand in for.
export interface UsePresenceProps {
  /** Whether the element is present (mounted in the DOM) */
  present?: boolean | undefined
  /** Whether to synchronize the present change immediately or defer it */
  immediate?: boolean | undefined
  /** Function called when the exit animation completes */
  onExitComplete?: (() => void) | undefined
  /** Whether to mount the element on first present, and keep it mounted afterwards */
  lazyMount?: boolean | undefined
  /** Whether to unmount the element when not present */
  unmountOnExit?: boolean | undefined
  /**
   * Whether to allow the initial presence animation.
   * @default false
   */
  skipAnimationOnMount?: boolean | undefined
}
