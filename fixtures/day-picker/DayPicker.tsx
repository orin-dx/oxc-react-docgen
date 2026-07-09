import * as React from "react";
import type { DayPickerProps } from "./props";

/**
 * react-day-picker — `DayPicker` component (simplified fixture)
 *
 * Adapted from gpbl/react-day-picker (MIT license), the real source file:
 * https://github.com/gpbl/react-day-picker/blob/main/packages/react-day-picker/src/DayPicker.tsx
 * (fetched at ~832 lines, `main` branch, 2026-07)
 *
 * The real component is a plain function (no forwardRef, no memo) that pulls
 * in a dozen internal hooks (`useCalendar`, `useSelection`, `useFocus`,
 * `useAnimation`, ...) and helper modules to build the calendar grid, none
 * of which exist in this repo. This fixture keeps the real exported
 * signature — `export function DayPicker(initialProps: DayPickerProps)` —
 * and the real time-zone-normalization preamble, which narrows on
 * `props.mode` to convert `selected` per-branch. That's genuine upstream
 * code and it exercises consumer-side narrowing of the very union
 * `DayPickerProps` (see ./props.ts) exists to test. Everything after that
 * (month-grid construction, dropdown options, DOM rendering) is replaced
 * with a stub return — irrelevant to prop-type extraction.
 */
export function DayPicker(initialProps: DayPickerProps) {
  let props = initialProps;
  const timeZone = props.timeZone;

  if (timeZone) {
    props = {
      ...initialProps,
      timeZone,
    };
    if (props.mode === "single" && props.selected) {
      // Real upstream converts `props.selected` to the target time zone here.
    } else if (props.mode === "multiple" && props.selected) {
      props.selected = props.selected?.map((date) => date);
    } else if (props.mode === "range" && props.selected) {
      props.selected = {
        from: props.selected.from,
        to: props.selected.to,
      };
    }
  }

  return (
    <div
      id={props.id}
      className={props.className}
      style={props.style}
      dir={props.dir}
      role={props.role}
    />
  );
}
