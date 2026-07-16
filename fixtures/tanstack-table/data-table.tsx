/**
 * TanStack Table — @tanstack/react-table adapter + a generic `DataTable`
 * usage wrapper (simplified fixture)
 *
 * `Renderable`/`flexRender`/`isReactComponent`/`isClassComponent`/
 * `isExoticComponent`/`useReactTable` below are adapted, near-verbatim, from
 * TanStack/table (MIT license), tag `v8.21.3`:
 *   https://github.com/TanStack/table/blob/v8.21.3/packages/react-table/src/index.tsx
 * (real file is 94 lines; only the `@tanstack/table-core` import/re-export
 * is repointed at the local `./types`).
 *
 * Neither `@tanstack/table-core` nor `@tanstack/react-table` actually ships
 * a React *component* — both packages are hooks/functions only
 * (`useReactTable`, `flexRender`). So the `DataTable<TData, TValue>`
 * component below — this fixture's actual extraction target — isn't lifted
 * from either package; its JSX body is adapted from the real, fetched
 * `examples/react/basic/src/main.tsx` (same repo/tag, MIT license):
 *   https://github.com/TanStack/table/blob/v8.21.3/examples/react/basic/src/main.tsx
 * restructured from that example's `App()` (which fixes its data to a
 * single non-generic `Person` type and renders a `<tfoot>`, both dropped
 * here) into a reusable generic component so `TData`/`TValue` show up as
 * real component props — `columns: ColumnDef<TData, TValue>[]` — flowing in
 * from `useReactTable`/`ColumnDef` (./types.ts) through `createColumn`
 * (./column.ts) and back out through `flexRender`'s own `TProps` generic.
 */
import * as React from 'react'
export * from './types'

import {
  TableOptions,
  TableOptionsResolved,
  RowData,
  ColumnDef,
  createTable,
} from './types'

export type Renderable<TProps> = React.ReactNode | React.ComponentType<TProps>

//

/**
 * If rendering headers, cells, or footers with custom markup, use flexRender instead of `cell.getValue()` or `cell.renderValue()`.
 */
export function flexRender<TProps extends object>(
  Comp: Renderable<TProps>,
  props: TProps
): React.ReactNode | React.JSX.Element {
  return !Comp ? null : isReactComponent<TProps>(Comp) ? (
    <Comp {...props} />
  ) : (
    Comp
  )
}

function isReactComponent<TProps>(
  component: unknown
): component is React.ComponentType<TProps> {
  return (
    isClassComponent(component) ||
    typeof component === 'function' ||
    isExoticComponent(component)
  )
}

function isClassComponent(component: any) {
  return (
    typeof component === 'function' &&
    (() => {
      const proto = Object.getPrototypeOf(component)
      return proto.prototype && proto.prototype.isReactComponent
    })()
  )
}

function isExoticComponent(component: any) {
  return (
    typeof component === 'object' &&
    typeof component.$$typeof === 'symbol' &&
    ['react.memo', 'react.forward_ref'].includes(component.$$typeof.description)
  )
}

export function useReactTable<TData extends RowData>(
  options: TableOptions<TData>
) {
  // Compose in the generic options to the user options
  const resolvedOptions: TableOptionsResolved<TData> = {
    state: {}, // Dummy state
    onStateChange: () => {}, // noop
    renderFallbackValue: null,
    ...options,
  }

  // Create a new table and store it in state
  const [tableRef] = React.useState(() => ({
    current: createTable<TData>(resolvedOptions),
  }))

  // By default, manage table state here using the table's initial state
  const [state, setState] = React.useState(() => tableRef.current.initialState)

  // Compose the default state above with any user state. This will allow the user
  // to only control a subset of the state if desired.
  tableRef.current.setOptions(prev => ({
    ...prev,
    ...options,
    state: {
      ...state,
      ...options.state,
    },
    // Similarly, we'll maintain both our internal state and any user-provided
    // state.
    onStateChange: updater => {
      setState(updater)
      options.onStateChange?.(updater)
    },
  }))

  return tableRef.current
}

//

export interface DataTableProps<TData extends RowData, TValue> {
  columns: ColumnDef<TData, TValue>[]
  data: TData[]
}

/**
 * The standard TanStack Table React usage pattern (see file header) —
 * `useReactTable<TData>` + `flexRender` composed into a reusable component.
 * `TData`/`TValue` here are real component props, not internal-only type
 * params: this is the fixture's target for prop/generic extraction.
 */
export function DataTable<TData extends RowData, TValue>({
  columns,
  data,
}: DataTableProps<TData, TValue>) {
  const table = useReactTable({
    data,
    columns,
  })

  return (
    <table>
      <thead>
        {table.getHeaderGroups().map(headerGroup => (
          <tr key={headerGroup.id}>
            {headerGroup.headers.map(header => (
              <th key={header.id}>
                {header.isPlaceholder
                  ? null
                  : flexRender(
                      header.column.columnDef.header,
                      header.getContext()
                    )}
              </th>
            ))}
          </tr>
        ))}
      </thead>
      <tbody>
        {table.getRowModel().rows.map(row => (
          <tr key={row.id}>
            {row.getVisibleCells().map(cell => (
              <td key={cell.id}>
                {flexRender(cell.column.columnDef.cell, cell.getContext())}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  )
}
