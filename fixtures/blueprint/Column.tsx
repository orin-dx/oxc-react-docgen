import * as React from "react";

/**
 * Blueprint — @blueprintjs/table Column (simplified fixture)
 *
 * Adapted from palantir/blueprint (Apache License 2.0), the real source file:
 *   https://github.com/palantir/blueprint/blob/develop/packages/table/src/column.tsx
 *
 * The real file is already tiny (~70 lines) — `Column` is a marker component;
 * `Table` reads `id`/`loadingOptions`/`cellRenderer`/`columnHeaderCellRenderer`
 * off of its React element children rather than ever mounting it.
 *
 * Upstream, `Column` is `class Column extends PureComponent<ColumnProps>` —
 * see the header comment in ./Table.tsx for why this fixture represents it
 * as a function component instead (this repo's extractor has no class
 * component support at all, by design, and testing that would confound the
 * scale/compound-API stress test this fixture exists for). `ColumnProps`
 * itself, including `ColumnNameProps` and its JSDoc, is preserved verbatim;
 * only the component wrapper shape and the `@blueprintjs/core` import
 * (`DISPLAYNAME_PREFIX`, `Props`) and real `emptyCellRenderer` (which
 * renders a real `<Cell />`, out of scope for this fixture) are adapted.
 */
import {
    type CellProps,
    type CellRenderer,
    type ColumnHeaderRenderer,
    type ColumnLoadingOption,
    type ColumnNameProps,
    DISPLAYNAME_PREFIX,
    type Props,
} from "./blueprintTypes";

export interface ColumnProps extends ColumnNameProps, Props {
    /**
     * A unique ID, similar to React's `key`. This is used, for example, to
     * maintain the width of a column between re-ordering and rendering. If no
     * IDs are provided, widths will be persisted across renders using a
     * column's index only. Columns widths can also be persisted outside the
     * `Table` component, then passed in with the `columnWidths` prop.
     */
    id?: string | number;

    /**
     * Set this prop to specify whether to render the loading state of the
     * column header and cells in this column. Column-level `loadingOptions`
     * override `Table`-level `loadingOptions`. For example, if you set
     * `loadingOptions=[ TableLoadingOption.CELLS ]` on `Table` and
     * `loadingOptions=[ ColumnLoadingOption.HEADER ]` on a `Column`, the cells
     * in that column will _not_ show their loading state.
     */
    loadingOptions?: ColumnLoadingOption[];

    /**
     * An instance of `CellRenderer`, a function that takes a row and column
     * index, and returns a `Cell` React element.
     */
    cellRenderer?: CellRenderer;

    /**
     * An instance of `ColumnHeaderRenderer`, a function that takes a column
     * index and returns a `ColumnHeaderCell` React element.
     */
    columnHeaderCellRenderer?: ColumnHeaderRenderer;
}

/** Trimmed stand-in for the real `emptyCellRenderer`, which renders a real `<Cell />` (out of scope here). */
function emptyCellRenderer(): React.ReactElement<CellProps> | undefined {
    return undefined;
}

/**
 * Column component.
 *
 * @see https://blueprintjs.com/docs/#table/api.column
 */
export function Column({ cellRenderer = emptyCellRenderer }: ColumnProps): React.ReactElement | null {
    // Trimmed: `Column` never renders anything itself — `Table` reads its
    // props straight off the element and only ever invokes `cellRenderer`.
    void cellRenderer;
    return null;
}

Column.displayName = `${DISPLAYNAME_PREFIX}.Column`;
