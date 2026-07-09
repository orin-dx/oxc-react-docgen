import * as React from "react";

/**
 * Fluent UI React v9 — @fluentui/react-button Button (simplified fixture)
 *
 * Adapted from microsoft/fluentui (MIT license):
 *   - packages/react-components/react-button/library/src/components/Button/Button.tsx
 *   - packages/react-components/react-button/library/src/components/Button/Button.types.ts
 *   - packages/react-components/react-utilities/src/compose/types.ts
 *       (Slot, ComponentProps, ComponentState, ForwardRefComponent, DistributiveOmit)
 *   - packages/react-components/react-aria/library/src/button/types.ts (ARIAButtonSlotProps)
 *
 * Fluent's real Button.tsx delegates to `useButton_unstable` / `useButtonStyles_unstable`
 * (sibling hook files) and imports `ForwardRefComponent` from `@fluentui/react-utilities`,
 * styling from `@griffel/react`, and a custom-style hook from `@fluentui/react-shared-contexts`
 * — none of those packages are installed in this repo. This fixture inlines simplified,
 * type-accurate versions of the "slots" plumbing (`Slot`, `ComponentProps`, `ComponentState`,
 * `ForwardRefComponent`, `DistributiveOmit`, `ARIAButtonSlotProps`) so the real
 * `ButtonSlots`/`ButtonProps`/`ButtonState` shapes — and a real forwardRef component built on
 * top of them — can be exercised without pulling in the rest of the Fluent UI monorepo. The
 * `use Button_unstable` / `renderButton_unstable` bodies below are minimal stand-ins for
 * Fluent's real (much larger) implementations, kept only so the component is runnable.
 */

// ---------------------------------------------------------------------------
// Slot / compose plumbing (simplified stub of @fluentui/react-utilities)
// Real source: packages/react-components/react-utilities/src/compose/types.ts
// ---------------------------------------------------------------------------

/** Matches any component's Slots type, e.g. ButtonSlots. */
type SlotPropsRecord = Record<string, object | null | undefined>;

/**
 * The props type for a slot. `Type` is either an intrinsic element tag (e.g. 'button'), in
 * which case the slot also accepts an `as` override into `AlternateAs`, or an explicit props
 * object (e.g. a component's own Props type), in which case it is used as-is. Simplified from
 * the real `Slot<Type, AlternateAs>`, which also supports shorthand children and per-slot
 * render functions.
 */
type Slot<
  Type extends keyof React.JSX.IntrinsicElements | object,
  AlternateAs extends keyof React.JSX.IntrinsicElements = never,
> = Type extends keyof React.JSX.IntrinsicElements
  ?
      | ({ as?: Type } & React.ComponentPropsWithRef<Type>)
      | (AlternateAs extends unknown ? { as: AlternateAs } & React.ComponentPropsWithRef<AlternateAs> : never)
      | null
  : Type;

/** Removes null/undefined from the slot type, extracting just the slot's Props object. */
type ExtractSlotProps<S> = Exclude<S, null | undefined>;

/**
 * Defines the Props type for a component given its slots and the definition of which one is
 * the primary slot (defaults to 'root'). The primary slot's props are spread onto the
 * component's props directly; other slots stay nested by name.
 */
type ComponentProps<Slots extends SlotPropsRecord, Primary extends keyof Slots = "root"> = Omit<
  Slots,
  Primary & "root"
> &
  ExtractSlotProps<Slots[Primary]>;

/** Defines the resolved State object of a component given its slots. */
type ComponentState<Slots extends SlotPropsRecord> = {
  /** The base element type rendered for each slot. */
  components: { [Key in keyof Slots]-?: React.ElementType };
} & {
  [Key in keyof Slots]: ExtractSlotProps<Slots[Key]>;
};

/** Distributes Omit over a union instead of collapsing it first. */
type DistributiveOmit<T, K extends keyof any> = T extends unknown ? Omit<T, K> : T;

/**
 * Return type for `React.forwardRef`. The real version infers the ref element type from the
 * props (via a marker event handler); simplified here to a plain HTMLElement ref.
 */
type ForwardRefComponent<Props> = React.ForwardRefExoticComponent<Props & React.RefAttributes<HTMLElement>>;

// ---------------------------------------------------------------------------
// ARIA button slot props (simplified stub of @fluentui/react-aria)
// Real source: packages/react-components/react-aria/library/src/button/types.ts
// ---------------------------------------------------------------------------

/**
 * Native button/anchor props plus the `disabled`/`disabledFocusable` handling that
 * `useARIAButtonProps` normalizes across `<button>` and `<a>` (e.g. `<a>` doesn't support the
 * native `disabled` attribute, so Fluent maps it to `aria-disabled` at runtime instead).
 */
type ARIAButtonSlotProps<AlternateAs extends "a" = "a"> = ExtractSlotProps<Slot<"button", AlternateAs>> & {
  disabled?: boolean;
  disabledFocusable?: boolean;
};

// ---------------------------------------------------------------------------
// Button.types.ts (real, unmodified prop definitions)
// Real source: packages/react-components/react-button/library/src/components/Button/Button.types.ts
// ---------------------------------------------------------------------------

export type ButtonSlots = {
  /** Root of the component that renders as either a `<button>` tag or an `<a>` tag. */
  root: NonNullable<Slot<ARIAButtonSlotProps<"a">>>;

  /** Icon that renders either before or after the `children` as specified by the `iconPosition` prop. */
  icon?: Slot<"span">;
};

/** A button supports different sizes. */
export type ButtonSize = "small" | "medium" | "large";

export type ButtonProps = ComponentProps<ButtonSlots> & {
  /**
   * A button can have its content and borders styled for greater emphasis or to be subtle.
   * - 'secondary' (default): Gives emphasis to the button in such a way that it indicates a secondary action.
   * - 'primary': Emphasizes the button as a primary action.
   * - 'outline': Removes background styling.
   * - 'subtle': Minimizes emphasis to blend into the background until hovered or focused.
   * - 'transparent': Removes background and border styling.
   *
   * @default 'secondary'
   */
  appearance?: "secondary" | "primary" | "outline" | "subtle" | "transparent";

  /**
   * When set, allows the button to be focusable even when it has been disabled. This is used in scenarios where it
   * is important to keep a consistent tab order for screen reader and keyboard users. The primary example of this
   * pattern is when the disabled button is in a menu or a commandbar and is seldom used for standalone buttons.
   *
   * @default false
   */
  disabledFocusable?: boolean;

  /**
   * A button can show that it cannot be interacted with.
   *
   * @default false
   */
  disabled?: boolean;

  /**
   * A button can format its icon to appear before or after its content.
   *
   * @default 'before'
   */
  iconPosition?: "before" | "after";

  /**
   * A button can be rounded, circular, or square.
   *
   * @default 'rounded'
   */
  shape?: "rounded" | "circular" | "square";

  /**
   * A button supports different sizes.
   *
   * @default 'medium'
   */
  size?: ButtonSize;
};

export type ButtonBaseProps = DistributiveOmit<ButtonProps, "appearance" | "size" | "shape">;

export type ButtonState = ComponentState<ButtonSlots> &
  Required<Pick<ButtonProps, "appearance" | "disabledFocusable" | "disabled" | "iconPosition" | "shape" | "size">> & {
    /**
     * A button can contain only an icon.
     *
     * @default false
     */
    iconOnly: boolean;
  };

// ---------------------------------------------------------------------------
// Button.tsx (real component structure; hook bodies are minimal stand-ins — see header)
// Real source: packages/react-components/react-button/library/src/components/Button/Button.tsx
// ---------------------------------------------------------------------------

function useButton_unstable(props: ButtonProps, ref: React.Ref<HTMLElement>): ButtonState {
  const {
    appearance = "secondary",
    disabled = false,
    disabledFocusable = false,
    icon,
    iconPosition = "before",
    shape = "rounded",
    size = "medium",
    ...rest
  } = props;

  return {
    components: { root: "button", icon: "span" },
    root: { ref, disabled, ...rest } as unknown as ComponentState<ButtonSlots>["root"],
    icon: icon as unknown as ComponentState<ButtonSlots>["icon"],
    appearance,
    disabled,
    disabledFocusable,
    iconPosition,
    shape,
    size,
    iconOnly: Boolean(icon) && !("children" in rest && rest.children),
  };
}

function renderButton_unstable(state: ButtonState): React.ReactElement {
  const { root: Root, icon: Icon } = state.components;
  const rootProps = state.root as Record<string, unknown>;
  const iconProps = state.icon as Record<string, unknown> | undefined;

  return (
    <Root {...rootProps}>
      {state.iconPosition !== "after" && state.icon && <Icon {...iconProps} />}
      {rootProps.children as React.ReactNode}
      {state.iconPosition === "after" && state.icon && <Icon {...iconProps} />}
    </Root>
  );
}

/**
 * Buttons give people a way to trigger an action.
 */
export const Button: ForwardRefComponent<ButtonProps> = React.forwardRef((props, ref) => {
  const state = useButton_unstable(props, ref);
  return renderButton_unstable(state);
}) as ForwardRefComponent<ButtonProps>;

Button.displayName = "Button";
