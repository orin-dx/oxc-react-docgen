/**
 * Headless UI — @headlessui/react Listbox (simplified fixture)
 *
 * Adapted from tailwindlabs/headlessui (MIT license), the real source files:
 *   https://github.com/tailwindlabs/headlessui/blob/main/packages/%40headlessui-react/src/components/listbox/listbox.tsx
 *   https://github.com/tailwindlabs/headlessui/blob/main/packages/%40headlessui-react/src/types.ts (./types.ts)
 *   https://github.com/tailwindlabs/headlessui/blob/main/packages/%40headlessui-react/src/utils/render.ts (./render.tsx)
 *
 * Headless UI is the classic `as="button"`-style polymorphic prop convention
 * (older than Base UI's `render`-prop and Radix's `asChild`, both already
 * covered in fixtures/base-ui and fixtures/radix): every component's props
 * extend `Props<TTag, RenderPropArg, PropsWeControl, Overrides>` from
 * ./types.ts, which resolves `TTag`'s own element props, strips the ones
 * Headless UI controls (`as`, `children`, `refName`, `className`), and layers
 * a component-specific `Overrides` object on top. The real, un-simplified
 * `Props<...>` helper is preserved verbatim in ./types.ts.
 *
 * Of the ~10 components in the real 1067-line file (Listbox, ListboxButton,
 * ListboxOptions, ListboxOption, ListboxLabel, ListboxSelectedOption, ...),
 * this fixture keeps three that best show the pattern at increasing
 * complexity:
 *   - `Listbox` (root): 3 type params (`TTag`, `TType`, `TActualType`),
 *     `Fragment` as its default tag (renders nothing on its own), and a large
 *     `Overrides` object (`value`, `defaultValue`, `onChange`, `by`, `disabled`,
 *     `multiple`, ...).
 *   - `ListboxButton`: 1 type param (`TTag = 'button'`), the simplest and most
 *     "classic" instance of the pattern — a real DOM default tag plus a small
 *     `Overrides` object (`autoFocus`, `disabled`).
 *   - `ListboxOption`: 2 type params, where `TType` is inferred from
 *     `Parameters<typeof ListboxRoot>[0]['value']` (a real forward reference
 *     to a `let`-bound value further down this same file, kept verbatim —
 *     see the comment above `OptionFn` below) and whose `Overrides` includes
 *     a *required* generic prop (`value: TType`), unlike the other two
 *     components' all-optional overrides.
 * `ListboxOptions`, `ListboxLabel`, and `ListboxSelectedOption` are omitted
 * for brevity; they don't add a materially different shape of the `as`-prop
 * pattern once you've seen these three.
 *
 * Stubbed out: `@react-aria/focus`/`@react-aria/interactions` (focus ring,
 * hover), the internal listbox state machine (`useListboxMachine`,
 * `ActionTypes`, `ListboxStates`, `useSlice`), floating-ui positioning
 * (`FloatingProvider`, `useFloatingPanel`, anchor props), portal rendering,
 * form-field synchronization, scroll locking / inert-others, keyboard
 * type-ahead search, and the dozen or so single-purpose hooks
 * (`useControllable`, `useByComparator`, `useSyncRefs`, `useId`, ...) — none
 * of those packages/modules are installed in this repo, and none of them
 * affect the prop-type shape this fixture exists to exercise. Below, a single
 * plain `useState`-backed context replaces the real machine, and `useRender`
 * (./render.tsx) is a minimal `createElement` call instead of the real
 * Fragment-passthrough/data-attribute-merging renderer.
 */
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  Fragment,
  type ElementType,
  type Ref,
} from 'react'
import type { Props } from './types'
import { forwardRefWithAs, useRender, type HasDisplayName, type RefProp } from './render'

interface ListboxDataContextValue {
  value: unknown
  disabled: boolean
  invalid: boolean
  multiple: boolean
  open: boolean
  isSelected(value: unknown): boolean
  onChange(value: unknown): void
  toggle(): void
}

let ListboxDataContext = createContext<ListboxDataContextValue | null>(null)
ListboxDataContext.displayName = 'ListboxDataContext'

function useListboxData(component: string): ListboxDataContextValue {
  let context = useContext(ListboxDataContext)
  if (context === null) {
    let err = new Error(`<${component} /> is missing a parent <Listbox /> component.`)
    if (Error.captureStackTrace) Error.captureStackTrace(err, useListboxData)
    throw err
  }
  return context
}

// ---

let DEFAULT_LISTBOX_TAG = Fragment
type ListboxRenderPropArg<T> = {
  open: boolean
  disabled: boolean
  invalid: boolean
  value: T
}

export type ListboxProps<
  TTag extends ElementType = typeof DEFAULT_LISTBOX_TAG,
  TType = string,
  TActualType = TType,
> = Props<
  TTag,
  ListboxRenderPropArg<TType>,
  'value' | 'defaultValue' | 'onChange' | 'by' | 'disabled' | 'horizontal' | 'name' | 'multiple',
  {
    value?: TType
    defaultValue?: TType
    onChange?: (value: TType) => void
    by?: keyof TActualType | ((a: TActualType, z: TActualType) => boolean)
    disabled?: boolean
    invalid?: boolean
    horizontal?: boolean
    form?: string
    name?: string
    multiple?: boolean

    __demoMode?: boolean
  }
>

function ListboxFn<
  TTag extends ElementType = typeof DEFAULT_LISTBOX_TAG,
  TType = string,
  TActualType = TType extends (infer U)[] ? U : TType,
>(props: ListboxProps<TTag, TType, TActualType>, ref: Ref<HTMLElement>) {
  let {
    value: controlledValue,
    defaultValue,
    form,
    name,
    onChange: controlledOnChange,
    by,
    invalid = false,
    disabled = false,
    horizontal = false,
    multiple = false,
    __demoMode = false,
    ...theirProps
  } = props

  // Real Listbox wires `form`/`name` into a hidden <FormFields> so the value
  // participates in native form submission, and `by`/`horizontal`/`__demoMode`
  // into the state machine's comparator and orientation handling. None of
  // that affects the prop *shape*, so they're inert here.
  void form
  void name
  void by
  void horizontal
  void __demoMode

  let [open, setOpen] = useState(false)
  let [uncontrolledValue, setUncontrolledValue] = useState<TType | TType[] | undefined>(
    defaultValue ?? ((multiple ? [] : undefined) as TType | TType[] | undefined),
  )
  let value = controlledValue ?? uncontrolledValue

  let onChange = useCallback(
    (nextValue: unknown) => {
      controlledOnChange?.(nextValue as TType)
      setUncontrolledValue(nextValue as TType)
      setOpen(false)
    },
    [controlledOnChange],
  )

  let isSelected = useCallback(
    (compareValue: unknown) =>
      multiple ? Array.isArray(value) && value.includes(compareValue as TType) : value === compareValue,
    [value, multiple],
  )

  let data = useMemo<ListboxDataContextValue>(
    () => ({
      value,
      disabled,
      invalid,
      multiple,
      open,
      isSelected,
      onChange,
      toggle: () => setOpen((o) => !o),
    }),
    [value, disabled, invalid, multiple, open, isSelected, onChange],
  )

  let slot: ListboxRenderPropArg<TType> = { open, disabled, invalid, value: value as TType }
  let ourProps = { ref }
  let render = useRender()

  return (
    <ListboxDataContext.Provider value={data}>
      {render({ ourProps, theirProps, slot, defaultTag: DEFAULT_LISTBOX_TAG, name: 'Listbox' })}
    </ListboxDataContext.Provider>
  )
}

// ---

let DEFAULT_BUTTON_TAG = 'button' as const
type ButtonRenderPropArg = {
  disabled: boolean
  invalid: boolean
  hover: boolean
  focus: boolean
  autofocus: boolean
  open: boolean
  active: boolean
  value: any
}
type ButtonPropsWeControl =
  | 'aria-controls'
  | 'aria-expanded'
  | 'aria-haspopup'
  | 'aria-labelledby'
  | 'disabled'

export type ListboxButtonProps<TTag extends ElementType = typeof DEFAULT_BUTTON_TAG> = Props<
  TTag,
  ButtonRenderPropArg,
  ButtonPropsWeControl,
  {
    autoFocus?: boolean
    disabled?: boolean
  }
>

function ButtonFn<TTag extends ElementType = typeof DEFAULT_BUTTON_TAG>(
  props: ListboxButtonProps<TTag>,
  ref: Ref<HTMLButtonElement>,
) {
  let data = useListboxData('Listbox.Button')
  let {
    id = 'headlessui-listbox-button',
    disabled = data.disabled || false,
    autoFocus = false,
    ...theirProps
  } = props

  let ourProps = {
    ref,
    id,
    type: 'button' as const,
    'aria-haspopup': 'listbox' as const,
    'aria-expanded': data.open,
    disabled: disabled || undefined,
    autoFocus,
    onClick: () => {
      if (!disabled) data.toggle()
    },
  }

  let slot: ButtonRenderPropArg = {
    open: data.open,
    active: data.open,
    disabled,
    invalid: data.invalid,
    value: data.value,
    hover: false,
    focus: false,
    autofocus: autoFocus,
  }

  let render = useRender()

  return render({
    ourProps,
    theirProps,
    slot,
    defaultTag: DEFAULT_BUTTON_TAG,
    name: 'Listbox.Button',
  })
}

// ---

let DEFAULT_OPTION_TAG = 'div' as const
type OptionRenderPropArg = {
  /** @deprecated use `focus` instead */
  active: boolean
  focus: boolean
  selected: boolean
  disabled: boolean

  selectedOption: boolean
}
type OptionPropsWeControl = 'aria-disabled' | 'aria-selected' | 'role' | 'tabIndex'

export type ListboxOptionProps<
  TTag extends ElementType = typeof DEFAULT_OPTION_TAG,
  TType = string,
> = Props<
  TTag,
  OptionRenderPropArg,
  OptionPropsWeControl,
  {
    disabled?: boolean
    value: TType
  }
>

function OptionFn<
  TTag extends ElementType = typeof DEFAULT_OPTION_TAG,
  // TODO: One day we will be able to infer this type from the generic in Listbox itself.
  // But today is not that day..
  TType = Parameters<typeof ListboxRoot>[0]['value'],
>(props: ListboxOptionProps<TTag, TType>, ref: Ref<HTMLElement>) {
  let { id = 'headlessui-listbox-option', disabled = false, value, ...theirProps } = props
  let data = useListboxData('Listbox.Option')
  let selected = data.isSelected(value)

  let ourProps = {
    id,
    ref,
    role: 'option' as const,
    tabIndex: disabled === true ? undefined : -1,
    'aria-disabled': disabled === true ? true : undefined,
    'aria-selected': selected,
    onClick: () => {
      if (disabled) return
      data.onChange(value)
    },
  }

  let slot: OptionRenderPropArg = {
    active: false,
    focus: false,
    selected,
    disabled,
    selectedOption: false,
  }

  let render = useRender()

  return render({
    ourProps,
    theirProps,
    slot,
    defaultTag: DEFAULT_OPTION_TAG,
    name: 'Listbox.Option',
  })
}

// ---

export interface _internal_ComponentListbox extends HasDisplayName {
  <
    TTag extends ElementType = typeof DEFAULT_LISTBOX_TAG,
    TType = string,
    TActualType = TType extends (infer U)[] ? U : TType,
  >(
    props: ListboxProps<TTag, TType, TActualType> & RefProp<typeof ListboxFn>,
  ): React.JSX.Element
}

export interface _internal_ComponentListboxButton extends HasDisplayName {
  <TTag extends ElementType = typeof DEFAULT_BUTTON_TAG>(
    props: ListboxButtonProps<TTag> & RefProp<typeof ButtonFn>,
  ): React.JSX.Element
}

export interface _internal_ComponentListboxOption extends HasDisplayName {
  <TTag extends ElementType = typeof DEFAULT_OPTION_TAG, TType = Parameters<typeof ListboxRoot>[0]['value']>(
    props: ListboxOptionProps<TTag, TType> & RefProp<typeof OptionFn>,
  ): React.JSX.Element
}

let ListboxRoot = forwardRefWithAs(ListboxFn) as _internal_ComponentListbox
export let ListboxButton = forwardRefWithAs(ButtonFn) as _internal_ComponentListboxButton
export let ListboxOption = forwardRefWithAs(OptionFn) as _internal_ComponentListboxOption

export let Listbox = Object.assign(ListboxRoot, {
  /** @deprecated use `<ListboxButton>` instead of `<Listbox.Button>` */
  Button: ListboxButton,
  /** @deprecated use `<ListboxOption>` instead of `<Listbox.Option>` */
  Option: ListboxOption,
})
