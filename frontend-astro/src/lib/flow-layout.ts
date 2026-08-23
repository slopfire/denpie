// Pure Flow layout preference model. No DOM, no localStorage access:
// callers read raw storage text at the boundary.

/** The user-selectable Flow card layout. */
export type FlowLayout = "grid" | "list";

/** Canonical localStorage keys for Flow layout. */
export const FLOW_LAYOUT_STORAGE_KEY = "denpie-flow-layout";
export const FLOW_GRID_COLUMNS_STORAGE_KEY = "denpie-flow-grid-columns";

export type GridColumns = 1 | 2 | 3 | 4;

/**
 * Boundary parser for the stored layout: only `grid` and `list` are valid;
 * missing, malformed, and legacy values normalize to `grid`.
 */
export function parseFlowLayout(value: string | null): FlowLayout {
  return value === "list" ? "list" : "grid";
}

/** Clamp a parsed numeric column count to the union `1 | 2 | 3 | 4`. */
function clampGridColumns(columns: number): GridColumns {
  const bounded = Math.floor(Math.min(4, Math.max(1, columns)));
  // Integer union 1 | 2 | 3 | 4.
  if (bounded <= 1) return 1;
  if (bounded === 2) return 2;
  if (bounded === 3) return 3;
  return 4;
}

/**
 * Boundary parser for the stored grid ceiling. Storage is untrusted text:
 * missing or malformed values default to 4; numeric values clamp to 1
 * through 4. Negative text is malformed rather than a value that can be
 * clamped.
 */
export function parseGridColumns(value: string | null): GridColumns {
  if (value === null) return 4;
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return 4;
  return clampGridColumns(Number(trimmed));
}

/**
 * Responsive ceiling classes: 2 adds `md:grid-cols-2`, 3 adds
 * `xl:grid-cols-3`, 4 adds `2xl:grid-cols-4`.
 */
export function gridClassesForColumns(
  columns: GridColumns,
): string {
  switch (columns) {
    case 1:
      return "grid grid-cols-1 gap-6 items-start";
    case 2:
      return "grid grid-cols-1 md:grid-cols-2 gap-6 items-start";
    case 3:
      return "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-6 items-start";
    case 4:
      return "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-6 items-start";
  }
}

/** List mode: one column, full width, capped and centered. */
export const FLOW_LIST_CLASSES =
  "grid grid-cols-1 gap-6 items-start w-full max-w-4xl mx-auto";
