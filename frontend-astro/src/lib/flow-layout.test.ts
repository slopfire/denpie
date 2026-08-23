import { describe, expect, test } from "bun:test";
import {
  FLOW_GRID_COLUMNS_STORAGE_KEY,
  FLOW_LAYOUT_STORAGE_KEY,
  FLOW_LIST_CLASSES,
  gridClassesForColumns,
  parseFlowLayout,
  parseGridColumns,
  type GridColumns,
} from "./flow-layout";

describe("parseFlowLayout", () => {
  test("missing, malformed, and legacy values normalize to grid", () => {
    expect(parseFlowLayout(null)).toBe("grid");
    expect(parseFlowLayout("")).toBe("grid");
    expect(parseFlowLayout("title-evil")).toBe("grid");
    expect(parseFlowLayout("GRID")).toBe("grid");
    expect(parseFlowLayout("cards")).toBe("grid");
  });

  test("only the exact valid values are honored", () => {
    expect(parseFlowLayout("grid")).toBe("grid");
    expect(parseFlowLayout("list")).toBe("list");
  });
});

describe("parseGridColumns", () => {
  test("missing and malformed values default to 4", () => {
    expect(parseGridColumns(null)).toBe(4);
    expect(parseGridColumns("")).toBe(4);
    expect(parseGridColumns("many")).toBe(4);
    expect(parseGridColumns("2.5")).toBe(4);
    expect(parseGridColumns("[2]")).toBe(4);
  });

  test("numeric values clamp to 1 through 4", () => {
    const cases: Array<readonly [string, GridColumns]> = [
      ["0", 1],
      ["1", 1],
      ["2", 2],
      ["3", 3],
      ["4", 4],
      ["99", 4],
      [" 3 ", 3],
    ];
    for (const [raw, expected] of cases) {
      expect(parseGridColumns(raw)).toBe(expected);
    }
  });

  test("negative values are invalid usize text and default to 4", () => {
    expect(parseGridColumns("-3")).toBe(4);
  });

  test("storage keys are the canonical denpie-flow values", () => {
    expect(FLOW_LAYOUT_STORAGE_KEY).toBe("denpie-flow-layout");
    expect(FLOW_GRID_COLUMNS_STORAGE_KEY).toBe("denpie-flow-grid-columns");
  });
});

describe("gridClassesForColumns", () => {
  test("exact responsive ceiling classes per column count", () => {
    expect(gridClassesForColumns(1)).toBe(
      "grid grid-cols-1 gap-3 items-start",
    );
    expect(gridClassesForColumns(2)).toBe(
      "grid grid-cols-1 md:grid-cols-2 gap-3 items-start",
    );
    expect(gridClassesForColumns(3)).toBe(
      "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3 items-start",
    );
    expect(gridClassesForColumns(4)).toBe(
      "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 items-start",
    );
  });

  test("list mode is one centered capped column", () => {
    expect(FLOW_LIST_CLASSES).toBe(
      "grid grid-cols-1 gap-3 items-start w-full max-w-4xl mx-auto",
    );
  });
});
