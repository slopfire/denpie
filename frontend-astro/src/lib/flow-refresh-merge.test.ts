import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { mergeFlowRefresh, type FlowRefreshMergeResult } from "./flow-refresh-merge";
import { FlowCardInfoSchema, type FlowCardInfo } from "../generated/denpie_pb";
import type { FlowCursor } from "./flow-state";
import type { ReviewSlot } from "./flow-review-state";

function card(id: bigint, title = `card-${id}`): FlowCardInfo {
    return create(FlowCardInfoSchema, { id, title });
}

function idle(id: bigint, title?: string): ReviewSlot {
    return { kind: "idle", card: card(id, title) };
}

function completed(reviewedCardId: bigint): ReviewSlot {
    return {
        kind: "completed",
        reviewedCardId,
        topicName: "topic",
        title: `done-${reviewedCardId}`,
        createdAt: "2026-01-01T00:00:00Z",
        tipcardType: "casual_tip",
        pinned: false,
    };
}

function awaitingRefill(reviewedCardId: bigint): ReviewSlot {
    return {
        kind: "awaitingRefill",
        reviewedCardId,
        refillToken: 1,
        refillAttempts: 0,
        topicName: "topic",
        title: `refill-${reviewedCardId}`,
        createdAt: "2026-01-02T00:00:00Z",
        tipcardType: "repeatable_tip",
        pinned: false,
    };
}

const END: FlowCursor = { kind: "end" };

function merge(
    currentSlots: ReviewSlot[],
    freshPageCards: FlowCardInfo[],
    mutationsInFlight = false,
): FlowRefreshMergeResult {
    return mergeFlowRefresh({
        currentSlots,
        freshPageCards,
        cursor: END,
        mutationsInFlight,
    });
}

describe("mergeFlowRefresh", () => {
    test("fresh page wins: replaces matching cards at fresh positions with fresh fields", () => {
        const current = [idle(1n, "old"), idle(2n)];
        const fresh = [card(2n, "new"), card(3n)];
        const result = merge(current, fresh);
        expect(result.slots.map((slot) => slotCardIdOf(slot))).toEqual([
            2n, 3n, 1n,
        ]);
        expect(result.slots[0]).toEqual({
            kind: "idle",
            card: card(2n, "new"),
        });
        expect(result.cursor).toBe(END);
    });

    test("deduplicates by bigint id inside the fresh page (first occurrence wins)", () => {
        const current = [idle(1n)];
        const fresh = [card(1n, "first"), card(1n, "second")];
        const result = merge(current, fresh);
        expect(result.slots.map((slot) => slotCardIdOf(slot))).toEqual([1n]);
        expect(result.slots[0].kind === "idle" && result.slots[0].card.title).toBe(
            "first",
        );
    });

    test("preserves beyond-page-1 cards and placeholders after fresh ones in prior relative order", () => {
        const current = [
            idle(1n),
            idle(2n),
            completed(9n),
            idle(3n),
            awaitingRefill(8n),
            idle(4n),
        ];
        const fresh = [card(3n), card(1n)];
        const result = merge(current, fresh);
        expect(result.slots.map((slot) => slotCardIdOf(slot))).toEqual([
            3n, 1n, 2n, 9n, 8n, 4n,
        ]);
        // Placeholders keep their exact slot objects (live state survives).
        expect(result.slots[3]).toBe(current[2]);
        expect(result.slots[4]).toBe(current[4]);
    });

    test("empty fresh page keeps current slots (same reference), not clear-all", () => {
        const current = [idle(1n), completed(2n)];
        const result = merge(current, []);
        expect(result.slots).toBe(current);
        expect(result.slots).toHaveLength(2);
    });

    test("skips the merge entirely while a pin/delete/add mutation is in flight", () => {
        const current = [idle(1n)];
        const result = merge(current, [card(2n)], true);
        expect(result.slots).toBe(current);
        expect(result.slots.map((slot) => slotCardIdOf(slot))).toEqual([1n]);
    });

    test("does not mutate the input slots array", () => {
        const current = [idle(1n), idle(2n)];
        const snapshot = [...current];
        merge(current, [card(3n)]);
        expect(current).toEqual(snapshot);
    });
});

function slotCardIdOf(slot: ReviewSlot): bigint {
    return slot.kind === "idle" ||
        slot.kind === "reviewing" ||
        slot.kind === "error"
        ? slot.card.id
        : slot.reviewedCardId;
}
