/**
 * Minimal stand-ins for react-day-picker's `types/shared.ts` auxiliary types.
 *
 * Real source (MIT license):
 * https://github.com/gpbl/react-day-picker/blob/main/packages/react-day-picker/src/types/shared.ts
 *
 * None of these are the real day-picker shapes — most collapse to
 * `Record<string, X>` or a trimmed literal union. The fixture this file
 * supports (./props.ts, ../DayPicker.tsx) exists to exercise the 7-way
 * `mode`/`required` discriminated union on `DayPickerProps`, not these
 * support types, so they're simplified as far as possible while still being
 * real shapes (unions, generics, mapped-type-ish records) rather than `any`,
 * since a couple of them (`Matcher`, `DateRange`) are referenced directly
 * from the union branches under test and are kept verbatim.
 */
import type { CSSProperties, ComponentType, ReactNode } from "react";

/** Selection modes supported by DayPicker — the union's first discriminant. */
export type Mode = "single" | "multiple" | "range";

/** Simplified stand-in for the real `CustomComponents` (22 named slots). */
export type CustomComponents = Record<string, ComponentType<any>>;

/** Simplified stand-in for the real `Formatters` (7 named formatter fns). */
export type Formatters = Record<string, (...args: any[]) => ReactNode>;

/** Simplified stand-in for the real `Labels` (11 named aria-label fns). */
export type Labels = Record<string, (...args: any[]) => string>;

export type DateBefore = { before: Date };
export type DateAfter = { after: Date };
export type DateInterval = { before: Date; after: Date };
export type DayOfWeek = { dayOfWeek: number | number[] };

/** A value or predicate that matches specific days. Kept verbatim. */
export type Matcher =
  | boolean
  | ((date: Date) => boolean)
  | Date
  | Date[]
  | DateRange
  | DateBefore
  | DateAfter
  | DateInterval
  | DayOfWeek;

/** A range of dates. Kept verbatim — the type selected by the `range` branches. */
export type DateRange = { from: Date | undefined; to?: Date | undefined };

/** Kept verbatim (real type is already this small). */
export type DayEventHandler<EventType> = (
  date: Date,
  modifiers: Modifiers,
  e: EventType,
) => void;

/** Kept verbatim (real type is already this small). */
export type MonthChangeEventHandler = (month: Date) => void;

/** Simplified stand-in for the real mapped type keyed on UI/SelectionState/DayFlag/Animation enums. */
export type ClassNames = Record<string, string>;

/** Simplified stand-in for the real mapped type keyed on UI/SelectionState/DayFlag enums. */
export type Styles = Record<string, CSSProperties | undefined>;

/** Kept verbatim — the real source already defines this as `Record<string, boolean>`. */
export type Modifiers = Record<string, boolean>;

/** Kept verbatim (real type is already this small). */
export type ModifiersStyles = Record<string, CSSProperties>;

/** Kept verbatim (real type is already this small). */
export type ModifiersClassNames = Record<string, string>;

/** Simplified stand-in for the real 18-member numeral-system literal union. */
export type Numerals = "latn" | "arab" | "arabext" | "deva" | "beng";

/** Simplified stand-in for `DayPickerLocale` (real type is re-exported from date-fns). */
export type DayPickerLocale = Record<string, unknown>;
