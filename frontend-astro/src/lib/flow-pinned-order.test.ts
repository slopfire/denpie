import { describe, expect, test } from "bun:test";
import {
    PINNED_CARD_ORDER_STORAGE_KEY,
    movePinnedCard,
    normalizeCardOrder,
    parsePinnedCardId,
    parsePinnedCardOrder,
    replacePinnedCard,
    serializePinnedCardOrder,
    setPinnedMembership,
    transferPinnedCards,
} from "./flow-pinned-order";

const BIG = 9_223_372_036_854_775_807n; // i64::MAX
const ABOVE_SAFE = 9_007_199_254_740_993n;

describe("parsePinnedCardOrder", () => {
    test("accepts a raw numeric JSON array", () => {
        expect(parsePinnedCardOrder("[3,1,2]")).toEqual([3n, 1n, 2n]);
        expect(parsePinnedCardOrder("[]")).toEqual([]);
        expect(parsePinnedCardOrder("[ 3 , 1 ]")).toEqual([3n, 1n]);
        // Exact decimal identity beyond Number.MAX_SAFE_INTEGER.
        expect(parsePinnedCardOrder(`[${ABOVE_SAFE}]`)).toEqual([ABOVE_SAFE]);
        expect(parsePinnedCardOrder(`[${BIG}]`)).toEqual([BIG]);
    });

    test("rejects malformed input cleanly", () => {
        for (const raw of [
            null,
            "",
            "not json",
            "[1,]",
            "[1.5]",
            '["1"]',
            "[-1]",
            "[01]",
            "[9223372036854775808]",
            "null",
            "[null]",
            "1,2",
        ]) {
            expect(parsePinnedCardOrder(raw)).toBeNull();
        }
    });

    test("rejects non-positive and duplicate values", () => {
        expect(parsePinnedCardOrder("[0]")).toBeNull();
        expect(parsePinnedCardOrder("[2,2]")).toBeNull();
        expect(parsePinnedCardOrder(`[2,${BIG},2]`)).toBeNull();
    });
});

describe("parsePinnedCardId", () => {
    test("accepts exact positive i64 IDs and rejects untrusted drag text", () => {
        expect(parsePinnedCardId(String(ABOVE_SAFE))).toBe(ABOVE_SAFE);
        expect(parsePinnedCardId(String(BIG))).toBe(BIG);
        expect(parsePinnedCardId("0")).toBeNull();
        expect(parsePinnedCardId("01")).toBeNull();
        expect(parsePinnedCardId("-1")).toBeNull();
        expect(parsePinnedCardId("9223372036854775808")).toBeNull();
        expect(parsePinnedCardId("not-an-id")).toBeNull();
    });
});

describe("serializePinnedCardOrder", () => {
    test("writes a raw JSON integer array readable by both frontends", () => {
        expect(serializePinnedCardOrder([3n, 1n])).toBe("[3,1]");
        expect(serializePinnedCardOrder([])).toBe("[]");
        const serialized = serializePinnedCardOrder([ABOVE_SAFE, BIG]);
        expect(serialized).toBe(`[${ABOVE_SAFE},${BIG}]`);
        // Round-trips exactly through this parser.
        expect(parsePinnedCardOrder(serialized)).toEqual([ABOVE_SAFE, BIG]);
    });
});

describe("normalizeCardOrder", () => {
    test("discards unpinned IDs and retains saved relative order", () => {
        expect(normalizeCardOrder([4n, 2n, 9n], [2n, 3n, 4n])).toEqual([
            4n,
            2n,
            3n,
        ]);
    });

    test("appends newly pinned IDs in current source order", () => {
        expect(normalizeCardOrder([], [3n, 1n, 2n])).toEqual([3n, 1n, 2n]);
        expect(normalizeCardOrder([2n], [2n, 5n, 1n])).toEqual([2n, 5n, 1n]);
    });

    test("handles huge IDs exactly", () => {
        expect(normalizeCardOrder([ABOVE_SAFE], [BIG, ABOVE_SAFE])).toEqual([
            ABOVE_SAFE,
            BIG,
        ]);
    });
});

describe("movePinnedCard", () => {
    const order = [1n, 2n, 3n];

    test("moves forward: remove source then insert at target index", () => {
        expect(movePinnedCard(order, 1n, 3n)).toEqual([2n, 3n, 1n]);
    });

    test("moves backward with the same remove-then-insert semantics", () => {
        expect(movePinnedCard(order, 3n, 1n)).toEqual([3n, 1n, 2n]);
        expect(movePinnedCard(order, 2n, 1n)).toEqual([2n, 1n, 3n]);
    });

    test("unknown, equal IDs are no-ops returning the same reference", () => {
        expect(movePinnedCard(order, 9n, 1n)).toBe(order);
        expect(movePinnedCard(order, 1n, 9n)).toBe(order);
        expect(movePinnedCard(order, 2n, 2n)).toBe(order);
    });

    test("never mutates the input", () => {
        movePinnedCard(order, 1n, 3n);
        expect(order).toEqual([1n, 2n, 3n]);
    });
});

describe("replacePinnedCard", () => {
    test("replaces at the exact saved-order position", () => {
        expect(replacePinnedCard([4n, 2n, 7n], 2n, 5n)).toEqual([4n, 5n, 7n]);
    });

    test("huge-ID replacement keeps exact decimal identity", () => {
        expect(replacePinnedCard([ABOVE_SAFE], ABOVE_SAFE, BIG)).toEqual([BIG]);
    });

    test("unknown old or already-present new IDs are no-ops", () => {
        const order = [1n, 2n];
        expect(replacePinnedCard(order, 9n, 3n)).toBe(order);
        expect(replacePinnedCard(order, 1n, 2n)).toBe(order);
    });
});

describe("transferPinnedCards", () => {
    test("moves multiple IDs atomically while preserving each saved position", () => {
        expect(
            transferPinnedCards(
                [9n, 2n, 7n],
                [
                    { from: 9n, to: 11n },
                    { from: 7n, to: 13n },
                ],
            ),
        ).toEqual({ kind: "applied", order: [11n, 2n, 13n] });
    });

    test("reports a collision without partially transferring any position", () => {
        expect(
            transferPinnedCards(
                [9n, 2n, 7n],
                [
                    { from: 9n, to: 11n },
                    { from: 7n, to: 2n },
                ],
            ),
        ).toEqual({ kind: "collision" });
    });
});

describe("setPinnedMembership", () => {
    test("pinning appends; re-pinning is a same-reference no-op", () => {
        expect(setPinnedMembership([1n], 2n, true)).toEqual([1n, 2n]);
        const order = [1n, 2n];
        expect(setPinnedMembership(order, 2n, true)).toBe(order);
    });

    test("unpinning removes; removing absent is a same-reference no-op", () => {
        expect(setPinnedMembership([1n, 2n], 1n, false)).toEqual([2n]);
        const order = [1n];
        expect(setPinnedMembership(order, 9n, false)).toBe(order);
    });
});
