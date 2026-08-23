import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { FlowCardInfoSchema } from "../generated/denpie_pb";
import {
  cursorFromPage,
  loadMoreSuccess,
  type FlowCursor,
} from "./flow-state";

function card(id: bigint) {
  return create(FlowCardInfoSchema, { id, title: `card-${id}` });
}

describe("cursorFromPage", () => {
  test("hasMore with a non-empty token is `more` carrying that token", () => {
    const cursor = cursorFromPage({ nextPageToken: "tok-1", hasMore: true });
    expect(cursor).toEqual({ kind: "more", pageToken: "tok-1" });
  });

  test("hasMore without a token fails closed", () => {
    expect(() =>
      cursorFromPage({ nextPageToken: "", hasMore: true }),
    ).toThrow(/more pages without a cursor/);
  });

  test("no hasMore and no token is `end`", () => {
    expect(cursorFromPage({ nextPageToken: "", hasMore: false })).toEqual({
      kind: "end",
    });
  });

  test("a token on a final page fails closed", () => {
    expect(() =>
      cursorFromPage({ nextPageToken: "stray", hasMore: false }),
    ).toThrow(/cursor for a final page/);
  });
});

describe("loadMoreSuccess", () => {
  test("merges the fetched page and derives the next cursor", () => {
    const page = {
      cards: [card(2n)],
      cursor: { kind: "more" as const, pageToken: "tok-2" },
    };
    const result = loadMoreSuccess([card(1n)], page);
    expect(result.cards.map((c) => c.id)).toEqual([1n, 2n]);
    const cursor: FlowCursor = result.cursor;
    expect(cursor.kind).toBe("more");
    if (cursor.kind === "more") expect(cursor.pageToken).toBe("tok-2");
  });

  test("a final page ends pagination", () => {
    const page = { cards: [card(3n)], cursor: { kind: "end" as const } };
    const result = loadMoreSuccess([card(1n), card(2n)], page);
    expect(result.cards.map((c) => c.id)).toEqual([1n, 2n, 3n]);
    expect(result.cursor).toEqual({ kind: "end" });
  });

  test("deduplicates overlapping IDs across pages", () => {
    const page = {
      cards: [card(2n), card(4n)],
      cursor: { kind: "end" as const },
    };
    const result = loadMoreSuccess([card(1n), card(2n)], page);
    expect(result.cards.map((c) => c.id)).toEqual([1n, 2n, 4n]);
  });
});
