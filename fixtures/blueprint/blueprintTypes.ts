import * as React from "react";

/**
 * palantir/blueprint — local stand-ins for cross-package plumbing (simplified fixture)
 *
 * `Table.tsx` and `Column.tsx` (adapted from packages/table/src/table.tsx and
 * packages/table/src/column.tsx) pull supporting types from `@blueprintjs/core`,
 * `@blueprintjs/icons`, and a dozen-plus sibling modules inside
 * packages/table/src/ (common/, headers/, interactions/, regions.ts,
 * cell/cell.tsx). Neither `@blueprintjs/core` nor `@blueprintjs/icons` is
 * installed in this repo, and vendoring the full sibling module graph would
 * mean pulling in most of the table package. This file inlines real type
 * definitions (verbatim, including JSDoc) for everything that feeds directly
 * into `TableProps`/`ColumnProps`, and trims the types that are two-plus hops
 * removed from the props surface under test — `CellProps`, `HeaderCellProps`,
 * `ColumnHeaderCellProps`, `RowHeaderCellProps` — down to small stand-ins
 * that keep the real field names but drop their own nested dependencies
 * (icons, popovers).
 *
 * Note: the real `Table`/`Column` are ES6 class components; see the header
 * comment in Table.tsx for why this fixture represents them as function
 * components instead (no `AbstractComponent`/`TableState` stand-ins needed
 * as a result).
 *
 * Sources (Apache License 2.0), all under github.com/palantir/blueprint/blob/develop/:
 *   packages/core/src/common/props.ts                    (Props)
 *   packages/table/src/common/cellTypes.ts                (FocusMode, Focused*)
 *   packages/table/src/common/grid.ts                     (RowIndices, ColumnIndices)
 *   packages/table/src/common/renderMode.ts               (RenderMode)
 *   packages/table/src/headers/columnHeader.tsx           (ColumnWidths)
 *   packages/table/src/headers/rowHeader.tsx              (RowHeights)
 *   packages/table/src/headers/columnHeaderCell.tsx       (ColumnNameProps)
 *   packages/table/src/interactions/resizable.tsx         (IndexedResizeCallback)
 *   packages/table/src/interactions/selectable.tsx        (SelectedRegionTransform)
 *   packages/table/src/interactions/menus/menuContext.ts  (ContextMenuRenderer, MenuContext)
 *   packages/table/src/regions.ts                         (Region and friends)
 */

// ---- @blueprintjs/core stand-ins ----

/** Real (packages/core/src/common/props.ts) — verbatim; it really is this small. */
export interface Props {
    /** A space-delimited list of class names to pass along to a child element. */
    className?: string;
}

export const DISPLAYNAME_PREFIX = "Blueprint6";

// ---- packages/table/src/common/cellTypes.ts (verbatim) ----

export interface CellCoordinates {
    col: number;
    row: number;
}

export interface FocusedCellCoordinates extends CellCoordinates {
    focusSelectionIndex: number;
}

export enum FocusMode {
    CELL = "cell",
    ROW = "row",
}

export interface FocusedCell extends FocusedCellCoordinates {
    type: FocusMode.CELL;
}

export interface FocusedRow {
    type: FocusMode.ROW;
    row: number;
    focusSelectionIndex: number;
}

export type FocusedRegion = FocusedCell | FocusedRow;

// ---- packages/table/src/common/grid.ts (verbatim, relevant subset) ----

export interface RowIndices {
    rowIndexStart: number;
    rowIndexEnd: number;
}

export interface ColumnIndices {
    columnIndexStart: number;
    columnIndexEnd: number;
}

// ---- packages/table/src/common/renderMode.ts (verbatim) ----

export enum RenderMode {
    /**
     * Renders cells in batches across multiple animation frames. This improves
     * performance by spreading out work to keep a high FPS and avoid blocking
     * the UI, but it also introduces a noticeable scan-line rendering artifact
     * as successive batches of cells finish rendering.
     */
    BATCH = "batch",

    /**
     * Renders all cells synchronously on initial mount, then renders cells in
     * batches on successive updates (e.g. during scrolling). This helps to
     * remove visual rendering artifacts when the table is first rendered,
     * without slowing scrolling performance to a crawl.
     */
    BATCH_ON_UPDATE = "batch-on-update",

    /**
     * Disables the batch-rendering behavior, rendering all cells synchronously
     * at once. This may result in degraded performance on large tables and/or
     * on tables with complex cells.
     */
    NONE = "none",
}

// ---- packages/table/src/headers/columnHeader.tsx / rowHeader.tsx (verbatim) ----

export interface ColumnWidths {
    minColumnWidth: number;
    maxColumnWidth: number;
    defaultColumnWidth: number;
}

export interface RowHeights {
    minRowHeight: number;
    maxRowHeight: number;
    defaultRowHeight: number;
}

// ---- packages/table/src/interactions/resizable.tsx / selectable.tsx (verbatim) ----

export type IndexedResizeCallback = (index: number, size: number) => void;

export type SelectedRegionTransform = (
    region: Region,
    event: MouseEvent | KeyboardEvent,
    coords?: { top: number; left: number },
) => Region;

// ---- packages/table/src/regions.ts (verbatim, relevant subset) ----

export enum RegionCardinality {
    CELLS = "cells",
    FULL_ROWS = "full-rows",
    FULL_COLUMNS = "full-columns",
    FULL_TABLE = "full-table",
}

/**
 * A convenience object for subsets of `RegionCardinality` that are commonly
 * used as the `selectionMode` prop of the `<Table>`.
 */
export const SelectionModes = {
    ALL: [
        RegionCardinality.FULL_TABLE,
        RegionCardinality.FULL_COLUMNS,
        RegionCardinality.FULL_ROWS,
        RegionCardinality.CELLS,
    ],
    COLUMNS_AND_CELLS: [RegionCardinality.FULL_COLUMNS, RegionCardinality.CELLS],
    COLUMNS_ONLY: [RegionCardinality.FULL_COLUMNS],
    NONE: [] as RegionCardinality[],
    ROWS_AND_CELLS: [RegionCardinality.FULL_ROWS, RegionCardinality.CELLS],
    ROWS_ONLY: [RegionCardinality.FULL_ROWS],
};

export enum ColumnLoadingOption {
    CELLS = "cells",
    HEADER = "column-header",
}

export enum TableLoadingOption {
    CELLS = "cells",
    COLUMN_HEADERS = "column-header",
    ROW_HEADERS = "row-header",
}

export interface StyledRegionGroup {
    className?: string;
    regions: Region[];
}

/** An _inclusive_ interval of ZERO-indexed cell indices. */
export type CellInterval = [number, number];

/** Small datastructure for storing cell coordinates [row, column]. */
export type CellCoordinate = [number, number];

/**
 * @see Regions.getRegionCardinality for more about the format of this object.
 */
export interface Region {
    /**
     * The first and last row indices in the region, inclusive and zero-indexed.
     * If `rows` is `null`, then all rows are understood to be included in the
     * region.
     */
    rows?: CellInterval | null;

    /**
     * The first and last column indices in the region, inclusive and
     * zero-indexed. If `cols` is `null`, then all columns are understood to be
     * included in the region.
     */
    cols?: CellInterval | null;
}

// ---- packages/table/src/interactions/menus/menuContext.ts (verbatim) ----

export type ContextMenuRenderer = (context: MenuContext) => React.JSX.Element;

export interface MenuContext {
    /**
     * Returns an array of `Region`s that represent the user-intended context
     * of this menu. If the mouse click was on a selection, the array will
     * contain all selected regions. Otherwise it will have one `Region` that
     * represents the clicked cell (the same `Region` from `getTarget`).
     */
    getRegions: () => Region[];

    /**
     * Returns the list of selected `Region` in the table, regardless of
     * where the users clicked to launch the context menu. For the user-
     * intended regions for this context, use `getRegions` instead.
     */
    getSelectedRegions: () => Region[];

    /**
     * Returns a region containing the single cell that was clicked to launch
     * this context menu.
     */
    getTarget: () => Region;

    /**
     * Returns an array containing all of the unique, potentially non-
     * contiguous, cells contained in all the regions from `getRegions`. The
     * cell coordinates are sorted by rows then columns.
     */
    getUniqueCells: () => CellCoordinate[];
}

// ---- packages/table/src/headers/columnHeaderCell.tsx (verbatim) ----

export interface ColumnNameProps {
    /**
     * The name displayed in the header of the column.
     */
    name?: string;

    /**
     * A callback to override the default name rendering behavior. The default
     * behavior is to simply use the `ColumnHeaderCell`s name prop.
     *
     * This render callback can be used, for example, to provide a
     * `EditableName` component for editing column names.
     *
     * If you define this callback, we recommend you also set
     * `<Table enableColumnInteractionBar={true}>` to avoid issues with menus or selection.
     *
     * The callback will also receive the column index if an `index` was originally
     * provided via props.
     */
    nameRenderer?: (name: string, index?: number) => React.ReactElement<Props>;
}

// ---- Trimmed stand-ins for internal cell/header prop shapes ----
//
// The real `CellProps` (cell/cell.tsx), `HeaderCellProps` (headers/headerCell.tsx),
// `ColumnHeaderCellProps`, and `RowHeaderCellProps` are two-plus hops removed
// from `TableProps`/`ColumnProps` — they only ever appear as the generic
// parameter of a renderer callback's return type — and pull in
// `@blueprintjs/icons` and Popover plumbing that's out of scope here. Trimmed
// to the handful of fields that matter for a plausible generic parameter.

export interface CellProps extends Props {
    loading?: boolean;
    style?: React.CSSProperties;
}

/**
 * An instance of `CellRenderer`, a function that takes a row and column
 * index, and returns a `Cell` React element.
 */
export type CellRenderer = (rowIndex: number, columnIndex: number) => React.ReactElement<CellProps> | undefined;

export interface HeaderCellProps extends Props {
    index?: number;
    isActive?: boolean;
    loading?: boolean;
    name?: string;
}

export interface ColumnHeaderCellProps extends HeaderCellProps, ColumnNameProps {
    enableColumnInteractionBar?: boolean;
    isColumnSelected?: boolean;
}

export type ColumnHeaderRenderer = (columnIndex: number) => React.ReactElement<ColumnHeaderCellProps> | null;

export interface RowHeaderCellProps extends HeaderCellProps {
    isRowSelected?: boolean;
}

export type RowHeaderRenderer = (rowIndex: number) => React.ReactElement<RowHeaderCellProps>;

/** Real default for `TableProps["rowHeaderCellRenderer"]`, trimmed of the real `RowHeaderCell` import. */
export function renderDefaultRowHeader(rowIndex: number): React.ReactElement<RowHeaderCellProps> {
    return React.createElement(
        "div",
        { "data-row-index": rowIndex },
        String(rowIndex + 1),
    ) as unknown as React.ReactElement<RowHeaderCellProps>;
}

