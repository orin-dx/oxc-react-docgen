/**
 * TanStack Table — @tanstack/table-core generic column-def types (simplified fixture)
 *
 * Adapted from TanStack/table (MIT license), tag `v8.21.3` (the last v8
 * release — the repo's default branch is now `beta`, a v9 restructure;
 * verified via `gh api repos/TanStack/table/branches` + `.../tags` that this
 * file layout only exists on the v8 line, not on `beta`):
 *   https://github.com/TanStack/table/blob/v8.21.3/packages/table-core/src/types.ts
 *   https://github.com/TanStack/table/blob/v8.21.3/packages/table-core/src/core/table.ts
 *   https://github.com/TanStack/table/blob/v8.21.3/packages/table-core/src/core/headers.ts
 *   https://github.com/TanStack/table/blob/v8.21.3/packages/table-core/src/core/cell.ts
 *   https://github.com/TanStack/table/blob/v8.21.3/packages/table-core/src/utils.ts
 *
 * The real `types.ts` composes `ColumnDef<TData, TValue>` (and `Table`,
 * `Column`, `Row`, `Cell`, `Header`) from ~13 feature-plugin interfaces —
 * `VisibilityColumnDef`, `ColumnPinningColumnDef`, `ColumnFiltersColumnDef`,
 * `GlobalFilterColumnDef`, `SortingColumnDef`, `GroupingColumnDef`,
 * `ColumnSizingColumnDef`, row models, pagination, row selection, etc. —
 * each `extends`-ed on across ~15 files under `features/`. None of that
 * plugin surface affects the `<TData, TValue>` generic shape under test
 * here, so every feature mixin is dropped; each interface below keeps only
 * its own real members. `ColumnDef<TData, TValue>` itself — the 3-way
 * discriminated union (`DisplayColumnDef | GroupColumnDef |
 * AccessorColumnDef`) — and the whole `AccessorFn` / `ColumnDefTemplate` /
 * `HeaderContext` / `CellContext` chain it depends on are kept verbatim.
 *
 * `Table<TData>` is reduced from the real `CoreInstance<TData>` (itself one
 * of 14 instance interfaces the real `Table` extends) down to the members
 * `createColumn` (./column.ts) and `useReactTable`/`flexRender`
 * (./data-table.tsx) actually call. `createTable()` is a trivial stand-in
 * for the real ~527-line `core/table.ts`, which wires together every
 * feature's `getDefaultOptions`/`getInitialState`/row-model derivation —
 * machinery that doesn't touch the generic parameterization. `memo` /
 * `getMemoOptions` (real source: `utils.ts`) are folded in here rather than
 * kept as a 4th file, simplified to recompute-on-every-call instead of
 * diffing dependencies.
 */

// The real repo's `types.ts` imports `CoreColumn` from `./core/column`, and
// `core/column.ts` imports `Column`/`Table`/etc. back from `../types` — a
// genuine mutual dependency. Preserved here across our two files.
import { CoreColumn, createColumn } from './column'

export type RowData = unknown | object | any[]

export type Updater<T> = T | ((old: T) => T)
export type Getter<TValue> = () => TValue

export type PartialKeys<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>
export type RequiredKeys<T, K extends keyof T> = Omit<T, K> & Required<Pick<T, K>>

export type UnionToIntersection<T> = (
  T extends any ? (x: T) => any : never
) extends (x: infer R) => any
  ? R
  : never

export type AccessorFn<TData extends RowData, TValue = unknown> = (
  originalRow: TData,
  index: number,
) => TValue

export type ColumnDefTemplate<TProps extends object> =
  | string
  | ((props: TProps) => any)

export type StringOrTemplateHeader<TData, TValue> =
  | string
  | ColumnDefTemplate<HeaderContext<TData, TValue>>

export interface StringHeaderIdentifier {
  header: string
  id?: string
}

export interface IdIdentifier<TData extends RowData, TValue> {
  id: string
  header?: StringOrTemplateHeader<TData, TValue>
}

type ColumnIdentifiers<TData extends RowData, TValue> =
  | IdIdentifier<TData, TValue>
  | StringHeaderIdentifier

export interface ColumnMeta<TData extends RowData, TValue> {}

export interface ColumnDefBase<TData extends RowData, TValue = unknown> {
  getUniqueValues?: AccessorFn<TData, unknown[]>
  footer?: ColumnDefTemplate<HeaderContext<TData, TValue>>
  cell?: ColumnDefTemplate<CellContext<TData, TValue>>
  meta?: ColumnMeta<TData, TValue>
}

export interface IdentifiedColumnDef<TData extends RowData, TValue = unknown>
  extends ColumnDefBase<TData, TValue> {
  id?: string
  header?: StringOrTemplateHeader<TData, TValue>
}

export type DisplayColumnDef<
  TData extends RowData,
  TValue = unknown,
> = ColumnDefBase<TData, TValue> & ColumnIdentifiers<TData, TValue>

interface GroupColumnDefBase<TData extends RowData, TValue = unknown>
  extends ColumnDefBase<TData, TValue> {
  columns?: ColumnDef<TData, any>[]
}

export type GroupColumnDef<
  TData extends RowData,
  TValue = unknown,
> = GroupColumnDefBase<TData, TValue> & ColumnIdentifiers<TData, TValue>

export interface AccessorFnColumnDefBase<
  TData extends RowData,
  TValue = unknown,
> extends ColumnDefBase<TData, TValue> {
  accessorFn: AccessorFn<TData, TValue>
}

export type AccessorFnColumnDef<
  TData extends RowData,
  TValue = unknown,
> = AccessorFnColumnDefBase<TData, TValue> & ColumnIdentifiers<TData, TValue>

export interface AccessorKeyColumnDefBase<
  TData extends RowData,
  TValue = unknown,
> extends ColumnDefBase<TData, TValue> {
  id?: string
  accessorKey: (string & {}) | keyof TData
}

export type AccessorKeyColumnDef<
  TData extends RowData,
  TValue = unknown,
> = AccessorKeyColumnDefBase<TData, TValue> &
  Partial<ColumnIdentifiers<TData, TValue>>

export type AccessorColumnDef<TData extends RowData, TValue = unknown> =
  | AccessorKeyColumnDef<TData, TValue>
  | AccessorFnColumnDef<TData, TValue>

/**
 * The real, load-bearing generic under test: a 3-way discriminated union
 * threading `<TData, TValue>` down from every column-definition shape.
 */
export type ColumnDef<TData extends RowData, TValue = unknown> =
  | DisplayColumnDef<TData, TValue>
  | GroupColumnDef<TData, TValue>
  | AccessorColumnDef<TData, TValue>

export type ColumnDefResolved<
  TData extends RowData,
  TValue = unknown,
> = Partial<UnionToIntersection<ColumnDef<TData, TValue>>> & {
  accessorKey?: string
}

export interface Column<TData extends RowData, TValue = unknown>
  extends CoreColumn<TData, TValue> {}

export interface Row<TData extends RowData> {
  id: string
  index: number
  original: TData
  getVisibleCells: () => Cell<TData, unknown>[]
}

export interface Cell<TData extends RowData, TValue> {
  id: string
  column: Column<TData, TValue>
  row: Row<TData>
  getValue: () => TValue
  getContext: () => CellContext<TData, TValue>
}

export interface Header<TData extends RowData, TValue> {
  id: string
  colSpan: number
  isPlaceholder: boolean
  column: Column<TData, TValue>
  getContext: () => HeaderContext<TData, TValue>
}

export interface HeaderGroup<TData extends RowData> {
  id: string
  headers: Header<TData, unknown>[]
}

export interface HeaderContext<TData, TValue> {
  column: Column<TData, TValue>
  header: Header<TData, TValue>
  table: Table<TData>
}

export interface CellContext<TData extends RowData, TValue> {
  cell: Cell<TData, TValue>
  column: Column<TData, TValue>
  getValue: Getter<TValue>
  renderValue: Getter<TValue | null>
  row: Row<TData>
  table: Table<TData>
}

export interface RowModel<TData extends RowData> {
  rows: Row<TData>[]
  flatRows: Row<TData>[]
  rowsById: Record<string, Row<TData>>
}

export interface TableFeature<TData extends RowData = any> {
  createColumn?: (column: Column<TData, unknown>, table: Table<TData>) => void
}

export interface CoreOptions<TData extends RowData> {
  data: TData[]
  columns: ColumnDef<TData, any>[]
  state: Partial<TableState>
  onStateChange: (updater: Updater<Partial<TableState>>) => void
  renderFallbackValue: unknown
  _features?: TableFeature[]
}

export interface TableOptionsResolved<TData extends RowData>
  extends CoreOptions<TData> {}

export interface TableOptions<TData extends RowData>
  extends PartialKeys<
    TableOptionsResolved<TData>,
    'state' | 'onStateChange' | 'renderFallbackValue'
  > {}

export interface TableState {}

export interface Table<TData extends RowData> {
  initialState: TableState
  options: RequiredKeys<TableOptionsResolved<TData>, 'state'>
  _features: readonly TableFeature[]
  _getDefaultColumnDef: () => Partial<ColumnDef<TData, unknown>>
  _getOrderColumnsFn: () => (
    columns: Column<TData, unknown>[],
  ) => Column<TData, unknown>[]
  getHeaderGroups: () => HeaderGroup<TData>[]
  getRowModel: () => RowModel<TData>
  setOptions: (updater: Updater<TableOptionsResolved<TData>>) => void
}

export function createTable<TData extends RowData>(
  options: TableOptionsResolved<TData>,
): Table<TData> {
  // Simplified stand-in for the real `createTable` in
  // packages/table-core/src/core/table.ts: the real version wires together
  // every registered feature's `getDefaultOptions`, `getInitialState`, and
  // `createColumn` hooks via `table._features`, and derives
  // `getAllColumns`/`getHeaderGroups`/`getRowModel` as memoized
  // recomputations over that plugin state. This stub only builds the leaf
  // columns + a single flat header group + an unsorted/unfiltered row model
  // needed to exercise `ColumnDef<TData, TValue>` end-to-end at runtime.
  const table = {
    initialState: {},
    _features: [],
    _getDefaultColumnDef: () => ({}) as Partial<ColumnDef<TData, unknown>>,
    _getOrderColumnsFn: () => (columns: Column<TData, unknown>[]) => columns,
  } as unknown as Table<TData>

  table.options = { ...options } as RequiredKeys<TableOptionsResolved<TData>, 'state'>

  const columns: Column<TData, unknown>[] = options.columns.map(columnDef =>
    createColumn(table, columnDef as ColumnDef<TData, unknown>, 0, undefined),
  )

  table.getHeaderGroups = () => [
    {
      id: 'header-group-0',
      headers: columns.map(column => ({
        id: column.id,
        colSpan: 1,
        isPlaceholder: false,
        column,
        getContext: () =>
          ({ column, header: undefined, table }) as unknown as HeaderContext<
            TData,
            unknown
          >,
      })),
    },
  ]

  table.getRowModel = () => {
    const rows: Row<TData>[] = options.data.map((original, index) => {
      const row: Row<TData> = {
        id: `${index}`,
        index,
        original,
        getVisibleCells: () =>
          columns.map(column => ({
            id: `${row.id}_${column.id}`,
            column,
            row,
            getValue: () => column.accessorFn?.(original, index),
            getContext: () =>
              ({} as unknown as CellContext<TData, unknown>),
          })),
      }
      return row
    })
    return { rows, flatRows: rows, rowsById: {} }
  }

  table.setOptions = updater => {
    table.options = {
      ...table.options,
      ...(typeof updater === 'function' ? updater(table.options) : updater),
    }
  }

  return table
}

type NoInfer<T> = [T][T extends any ? 0 : never]

export function memo<TDeps extends readonly any[], TResult>(
  getDeps: () => readonly [...TDeps],
  fn: (...args: NoInfer<[...TDeps]>) => TResult,
  _opts?: { key?: any },
): () => TResult {
  return () => fn(...getDeps())
}

export function getMemoOptions(
  _tableOptions: unknown,
  _debugLevel: string,
  key: string,
): { key: string } {
  return { key }
}
