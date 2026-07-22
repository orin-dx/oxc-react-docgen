import * as React from "react";

/**
 * Blueprint — @blueprintjs/table Table (simplified fixture)
 *
 * Adapted from palantir/blueprint (Apache License 2.0), the real source files:
 *   https://github.com/palantir/blueprint/blob/develop/packages/table/src/table.tsx (~1639 lines)
 *   https://github.com/palantir/blueprint/blob/develop/packages/table/src/tableProps.ts (TableProps)
 *
 * This fixture exists to stress-test scale and compound-component complexity
 * on a genuinely large, real props interface — not a narrow syntax gap. The
 * real `table.tsx` pulls in `@blueprintjs/core`, `react-innertext`, and ~20
 * sibling modules (Grid, TableQuadrantStack, ColumnHeader/RowHeader,
 * GuideLayer/RegionLayer, LocatorImpl, TableHotkeys, resize/reorder/select
 * interaction handlers, tableUtils, tableState, …), none of which are
 * installed in this repo. Those are stubbed out or dropped; the `Table`
 * class body — grid construction, the quadrant/scroll/selection/hotkeys
 * pipeline, ~50 private render/event-handler methods spanning
 * table.tsx:190-1639 — is trimmed to a single stub `render()`, since none of
 * that affects the prop-type contract under test.
 *
 * IMPORTANT ADAPTATION: upstream `Table` is
 * `class Table extends AbstractComponent<TableProps, TableState, TableSnapshot>`.
 * This repo's extractor (crates/core/src/extractor/) has no class-component
 * support at all — no `visit_class_declaration` exists in
 * `extractor/visit.rs`, and this is an intentional, documented non-goal (see
 * MIGRATING.md:16 and docs/rdt-coverage.md: "class components ... out of
 * scope (modern React only)"), not a bug. A literal class-based port of this
 * fixture would therefore always extract zero props, for every run, forever
 * — regardless of interface scale — which would make this fixture measure
 * "do we support classes" (already known: no) instead of "do we handle a
 * huge, real, compound props interface" (the actual point of this fixture).
 * To keep the stress test meaningful, `Table`/`Column` are represented as
 * function components here. `TableProps`/`ColumnProps` — the interfaces
 * actually under test — are 100% unchanged by this; only the outer
 * class-vs-function wrapper shape differs from upstream.
 *
 * What's preserved verbatim (including JSDoc) from upstream:
 *   - `TableProps`, in full, from tableProps.ts
 *   - `TablePropsDefaults` / `TablePropsWithDefaults`
 *   - the real `Table.defaultProps` values, expressed as destructured
 *     parameter defaults to match this fixture's function-component form
 *     (upstream's version is a class). Static `X.defaultProps = {...}`
 *     assignments on function components are a separate, real pattern this
 *     tool now also supports — see extractor/interface.rs's
 *     try_scan_default_props and the MUI-shaped coverage in rdt-coverage.md.
 *   - `ColumnProps` (see ./Column.tsx) and the supporting types it and
 *     `TableProps` depend on (see ./blueprintTypes.ts for what's real vs.
 *     trimmed, and why)
 */
import type { ColumnProps } from "./Column";
import {
    type ColumnIndices,
    type ColumnWidths,
    type ContextMenuRenderer,
    DISPLAYNAME_PREFIX,
    type FocusedCellCoordinates,
    type FocusedRegion,
    FocusMode,
    type IndexedResizeCallback,
    type Props,
    type Region,
    RegionCardinality,
    RenderMode,
    type RowHeaderRenderer,
    type RowHeights,
    type RowIndices,
    type SelectedRegionTransform,
    renderDefaultRowHeader,
    SelectionModes,
    type StyledRegionGroup,
    type TableLoadingOption,
} from "./blueprintTypes";

/** @deprecated Use `TableProps` instead */
export type Table2Props = TableProps;

export interface TableProps extends Props, Partial<RowHeights>, Partial<ColumnWidths> {
    /**
     * This dependency list may be used to trigger a re-render of all cells when one of its elements changes
     * (compared using shallow equality checks). This is done by invalidating the grid, which forces
     * TableQuadrantStack to re-render.
     */
    cellRendererDependencies?: React.DependencyList;

    /**
     * The children of a `Table` component, which must be React elements
     * that use `ColumnProps`.
     */
    children?: React.ReactElement<ColumnProps> | Array<React.ReactElement<ColumnProps>>;

    /**
     * A sparse number array with a length equal to the number of columns. Any
     * non-null value will be used to set the width of the column at the same
     * index. Note that if you want to update these values when the user
     * drag-resizes a column, you may define a callback for `onColumnWidthChanged`.
     */
    columnWidths?: Array<number | null | undefined>;

    /**
     * An optional callback for displaying a context menu when right-clicking
     * on the table body. The callback is supplied with an array of
     * `Region`s. If the mouse click was on a selection, the array will
     * contain all selected regions. Otherwise it will have one `Region` that
     * represents the clicked cell.
     */
    bodyContextMenuRenderer?: ContextMenuRenderer;

    /**
     * Whether the body context menu is enabled.
     *
     * @default true if bodyContextMenuRenderer is defined
     */
    enableBodyContextMenu?: boolean;

    /**
     * If `true`, adds an interaction bar on top of all column header cells, and
     * moves interaction triggers into it.
     *
     * @default false
     */
    enableColumnInteractionBar?: boolean;

    /**
     * If `false`, disables reordering of columns.
     *
     * @default false
     */
    enableColumnReordering?: boolean;

    /**
     * If `false`, disables resizing of columns.
     *
     * @default true
     */
    enableColumnResizing?: boolean;

    /**
     * If `true`, there will be a single "focused" cell at all times,
     * which can be used to interact with the table as though it is a
     * spreadsheet. When false, no such cell will exist.
     *
     * @deprecated When using `Table2`, use the `focusMode` prop instead.
     *
     * @default false
     */
    enableFocusedCell?: boolean;

    /**
     * If `true`, empty space in the table container will be filled with empty
     * cells instead of a blank background.
     *
     * @default false
     */
    enableGhostCells?: boolean;

    /**
     * If `false`, only a single region of a single column/row/cell may be
     * selected at one time. Using `ctrl` or `meta` key will have no effect,
     * and a mouse drag will select the current column/row/cell only.
     *
     * @default true
     */
    enableMultipleSelection?: boolean;

    /**
     * If `false`, hides the row headers and settings menu.
     *
     * @default true
     */
    enableRowHeader?: boolean;

    /**
     * If `false`, disables reordering of rows.
     *
     * @default false
     */
    enableRowReordering?: boolean;

    /**
     * If `false`, disables resizing of rows.
     *
     * @default true
     */
    enableRowResizing?: boolean;

    /**
     * If defined, will set the focused cell state. This changes
     * the focused cell to controlled mode, meaning you are in charge of
     * setting the focus in response to events in the `onFocusedCell` callback.
     *
     * @deprecated When using `Table2`, use the `focusedRegion` prop instead
     */
    focusedCell?: FocusedCellCoordinates;

    /**
     * If defined, will set the focused region state. This changes the focused
     * region to controlled mode, meaning yo uare in charge of setting the focus
     * in response to events in the `onFocusedRegion` callback. The shape of
     * the region is defined by the `focusMode` prop.
     *
     * This API is only supported on `Table2`. When using `Table`, use
     * `focusedCell` and `onFocusedCell instead.
     */
    focusedRegion?: FocusedRegion;

    /**
     * If this is defined, there will be a single focused cell or row
     * at all times which can be used to interact with the table as
     * though it is a spread sheet. The type of allowed focus area
     * is given by the value. If undefined is passed, then this focus
     * state will be disabled.
     *
     * This API is only supported on `Table2`. When using `Table`, use
     * `enableFocusedCell` instead.
     *
     * @default undefined
     */
    focusMode?: FocusMode;

    /**
     * If `true`, selection state changes will cause the component to re-render.
     * If `false`, selection state is ignored when deciding to re-render.
     *
     * @default false
     */
    forceRerenderOnSelectionChange?: boolean;

    /**
     * If defined, this callback will be invoked for each cell when the user
     * attempts to copy a selection via `mod+c`. The returned data will be copied
     * to the clipboard and need not match the display value of the `<Cell>`.
     * The data will be invisibly added as `textContent` into the DOM before
     * copying. If not defined, a default implementation will be used that
     * turns the rendered cell elements into strings using 'react-innertext'.
     *
     * @param row the row index coordinate of the cell to get data for
     * @param col the col index coordinate of the cell to get data for
     * @param cellRenderer the cell renderer for this row, col coordinate in the table
     */
    getCellClipboardData?: (row: number, col: number, celRenderer: import("./blueprintTypes").CellRenderer) => any;

    /**
     * A list of `TableLoadingOption`. Set this prop to specify whether to
     * render the loading state for the column header, row header, and body
     * sections of the table.
     */
    loadingOptions?: TableLoadingOption[];

    /**
     * The number of columns to freeze to the left side of the table, counting
     * from the leftmost column.
     *
     * @default 0
     */
    numFrozenColumns?: number;

    /**
     * The number of rows to freeze to the top of the table, counting from the
     * topmost row.
     *
     * @default 0
     */
    numFrozenRows?: number;

    /**
     * The number of rows in the table.
     */
    numRows?: number;

    /**
     * If reordering is enabled, this callback will be invoked when the user finishes
     * drag-reordering one or more columns.
     */
    onColumnsReordered?: (oldIndex: number, newIndex: number, length: number) => void;

    /**
     * If resizing is enabled, this callback will be invoked when the user
     * finishes drag-resizing a column.
     */
    onColumnWidthChanged?: IndexedResizeCallback;

    /**
     * An optional callback invoked when all cells in view have completely rendered.
     * Will be invoked on initial mount and whenever cells update (e.g., on scroll).
     */
    onCompleteRender?: () => void;

    /**
     * If you want to do something after the copy or if you want to notify the
     * user if a copy fails, you may provide this optional callback.
     *
     * Due to browser limitations, the copy can fail. This usually occurs if
     * the selection is too large, like 20,000+ cells. The copy will also fail
     * if the browser does not support the copy method (see
     * `Clipboard.isCopySupported`).
     */
    onCopy?: (success: boolean) => void;

    /**
     * A callback called when the focus is changed in the table.
     *
     * @deprecated When using `Table2`, use the `onFocusedRegion` prop instead
     */
    onFocusedCell?: (focusedCell: FocusedCellCoordinates) => void;

    /**
     * A callback called when the focused region is changed in the table.
     *
     * This API is only supported for `Table2`. When using `Table`, use
     * `onFocusedCell` instead.
     */
    onFocusedRegion?: (focusedRegion: FocusedRegion) => void;

    /**
     * If resizing is enabled, this callback will be invoked when the user
     * finishes drag-resizing a row.
     */
    onRowHeightChanged?: IndexedResizeCallback;

    /**
     * If reordering is enabled, this callback will be invoked when the user finishes
     * drag-reordering one or more rows.
     */
    onRowsReordered?: (oldIndex: number, newIndex: number, length: number) => void;

    /**
     * A callback called when the selection is changed in the table.
     */
    onSelection?: (selectedRegions: Region[]) => void;

    /**
     * A callback called when the visible cell indices change in the table.
     */
    onVisibleCellsChange?: (rowIndices: RowIndices, columnIndices: ColumnIndices) => void;

    /**
     * Dictates how cells should be rendered. Supported modes are:
     * - `RenderMode.BATCH`: renders cells in batches to improve performance
     * - `RenderMode.BATCH_ON_UPDATE`: renders cells synchronously on mount and
     *   in batches on update
     * - `RenderMode.NONE`: renders cells synchronously all at once
     *
     * @default RenderMode.BATCH_ON_UPDATE
     */
    renderMode?: RenderMode;

    /**
     * Render each row's header cell.
     */
    rowHeaderCellRenderer?: RowHeaderRenderer;

    /**
     * A sparse number array with a length equal to the number of rows. Any
     * non-null value will be used to set the height of the row at the same
     * index. Note that if you want to update these values when the user
     * drag-resizes a row, you may define a callback for `onRowHeightChanged`.
     */
    rowHeights?: Array<number | null | undefined>;

    /**
     * If defined, will set the selected regions in the cells. If defined, this
     * changes table selection to controlled mode, meaning you in charge of
     * setting the selections in response to events in the `onSelection`
     * callback.
     *
     * Note that the `selectionModes` prop controls which types of events are
     * triggered to the `onSelection` callback, but does not restrict what
     * selection you can pass to the `selectedRegions` prop. Therefore you can,
     * for example, convert cell clicks into row selections.
     */
    selectedRegions?: Region[];

    /**
     * An optional transform function that will be applied to the located
     * `Region`.
     *
     * This allows you to, for example, convert cell `Region`s into row
     * `Region`s while maintaining the existing multi-select and meta-click
     * functionality.
     */
    selectedRegionTransform?: SelectedRegionTransform;

    /**
     * A `SelectionModes` enum value indicating the selection mode. You may
     * equivalently provide an array of `RegionCardinality` enum values for
     * precise configuration.
     *
     * The `SelectionModes` enum values are:
     * - `ALL`
     * - `NONE`
     * - `COLUMNS_AND_CELLS`
     * - `COLUMNS_ONLY`
     * - `ROWS_AND_CELLS`
     * - `ROWS_ONLY`
     *
     * The `RegionCardinality` enum values are:
     * - `FULL_COLUMNS`
     * - `FULL_ROWS`
     * - `FULL_TABLE`
     * - `CELLS`
     *
     * @default SelectionModes.ALL
     */
    selectionModes?: RegionCardinality[];

    /**
     * Styled region groups are rendered as overlays above the table and are
     * marked with their own `className` for custom styling.
     */
    styledRegionGroups?: StyledRegionGroup[];

    /**
     * If `false`, hides the column headers.
     *
     * @default true
     */
    enableColumnHeader?: boolean;
}

export type TablePropsDefaults = Required<
    Pick<
        TableProps,
        | "defaultColumnWidth"
        | "defaultRowHeight"
        | "enableColumnInteractionBar"
        | "enableFocusedCell"
        | "enableGhostCells"
        | "enableMultipleSelection"
        | "enableRowHeader"
        | "forceRerenderOnSelectionChange"
        | "getCellClipboardData"
        | "loadingOptions"
        | "maxColumnWidth"
        | "maxRowHeight"
        | "minColumnWidth"
        | "minRowHeight"
        | "numFrozenColumns"
        | "numFrozenRows"
        | "numRows"
        | "renderMode"
        | "rowHeaderCellRenderer"
        | "selectionModes"
        | "enableColumnHeader"
    >
>;

export type TablePropsWithDefaults = Omit<TableProps, keyof TablePropsDefaults> & TablePropsDefaults;

/**
 * Table component.
 *
 * @see https://blueprintjs.com/docs/#table/table
 *
 * Real defaults (from upstream `Table.defaultProps: TablePropsDefaults`),
 * expressed here as destructured parameter defaults instead of a static
 * class field — see the adaptation note at the top of this file.
 */
export function Table({
    defaultColumnWidth = 150,
    defaultRowHeight = 20,
    enableColumnHeader = true,
    enableColumnInteractionBar = false,
    enableFocusedCell = false,
    enableGhostCells = false,
    enableMultipleSelection = true,
    enableRowHeader = true,
    forceRerenderOnSelectionChange = false,
    getCellClipboardData = (row, col, cellRenderer) => {
        // Real implementation pipes the rendered cell through `react-innertext`
        // (not installed in this repo); trimmed to a same-shape stand-in.
        return cellRenderer(row, col);
    },
    loadingOptions = [],
    maxColumnWidth = 9999,
    maxRowHeight = 9999,
    minColumnWidth = 50,
    minRowHeight = 20,
    numFrozenColumns = 0,
    numFrozenRows = 0,
    numRows = 0,
    renderMode = RenderMode.BATCH_ON_UPDATE,
    rowHeaderCellRenderer = renderDefaultRowHeader,
    selectionModes = SelectionModes.ALL,
    // Remaining props have no upstream default; passed straight through.
    cellRendererDependencies,
    children,
    columnWidths,
    bodyContextMenuRenderer,
    enableBodyContextMenu,
    enableColumnReordering,
    enableColumnResizing,
    enableRowReordering,
    enableRowResizing,
    focusedCell,
    focusedRegion,
    focusMode,
    onColumnsReordered,
    onColumnWidthChanged,
    onCompleteRender,
    onCopy,
    onFocusedCell,
    onFocusedRegion,
    onRowHeightChanged,
    onRowsReordered,
    onSelection,
    onVisibleCellsChange,
    rowHeights,
    selectedRegions,
    selectedRegionTransform,
    styledRegionGroups,
    className,
}: TableProps): React.ReactElement | null {
    // Trimmed: the real render pipeline (table.tsx:486-1126) builds a Grid,
    // wires up TableQuadrantStack, column/row headers, selection/resize/
    // reorder interactions, hotkeys, and scroll/overlay layers across
    // several hundred lines. None of that affects the prop-type contract
    // under test.
    return null;
}

Table.displayName = `${DISPLAYNAME_PREFIX}.Table`;
