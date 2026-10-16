import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { mergeFlowRefresh, type FlowRefreshMergeResult } from "./flow-refresh-merge";
import { FlowCardInfoSchema, type FlowCardInfo } from "../generated/denpie_pb";
import type { FlowCursor } from "./flow-state";
import type { ReviewSlot } from "./flow-review-state";

function card(
    id: bigint,
    title = `card-${id}`,
    extra: Partial<FlowCardInfo> = {},
): FlowCardInfo {
    return create(FlowCardInfoSchema, { id, title, ...extra });
}

function repeatable(id: bigint, topicName: string): FlowCardInfo {
    return card(id, `rep-${id}`, {
        topicName,
        tipcardType: "repeatable_tip",
        status: "active",
    });
}

function idle(id: bigint, title?: string): ReviewSlot {
    return { kind: "idle", card: card(id, title) };
}

function completed(
    reviewedCardId: bigint,
    topicName = "topic",
    tipcardType = "casual_tip",
): ReviewSlot {
    return {
        kind: "completed",
        reviewedCardId,
        topicName,
        title: `done-${reviewedCardId}`,
        createdAt: "2026-01-01T00:00:00Z",
        tipcardType,
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
const MORE: FlowCursor = { kind: "more", pageToken: "fresh-page-2" };
const LOADED_BEYOND: FlowCursor = { kind: "more", pageToken: "loaded-page-4" };

function merge(
    currentSlots: ReviewSlot[],
    freshPageCards: FlowCardInfo[],
    mutationsInFlight = false,
    cursor: FlowCursor = END,
    previousCursor?: FlowCursor,
): FlowRefreshMergeResult {
    return mergeFlowRefresh({
        currentSlots,
        freshPageCards,
        cursor,
        previousCursor,
        mutationsInFlight,
    });
}

function ids(result: FlowRefreshMergeResult): bigint[] {
    return result.slots.map((slot) => slotCardIdOf(slot));
}

describe("mergeFlowRefresh", () => {
    test("fresh page wins: updates matching cards in place with fresh fields and prepends new ones", () => {
        const current = [idle(1n, "old"), idle(2n)];
        const result = merge(current, [card(2n, "new"), card(3n), card(1n, "old")]);
        expect(ids(result)).toEqual([3n, 1n, 2n]);
        expect(result.slots[2]).toEqual({ kind: "idle", card: card(2n, "new") });
        // Structurally equal card: the very same slot object is reused.
        expect(result.slots[1]).toBe(current[0]);
        expect(result.cursor).toBe(END);
    });

    test("a quiet refresh returns the same array so nothing re-renders", () => {
        const current = [idle(1n), idle(2n)];
        const result = merge(current, [card(1n), card(2n)]);
        expect(result.slots).toBe(current);
    });

    test("deduplicates by bigint id inside the fresh page (first occurrence wins)", () => {
        const current = [idle(1n)];
        const result = merge(current, [card(1n, "first"), card(1n, "second")]);
        expect(ids(result)).toEqual([1n]);
        expect(result.slots[0].kind === "idle" && result.slots[0].card.title).toBe(
            "first",
        );
    });

    test("a failed-review slot keeps its error and retry state", () => {
        const failed: ReviewSlot = {
            kind: "error",
            card: card(1n),
            generation: 1,
            attempt: {} as never,
            mutationOutcomeIndeterminate: true,
            message: "boom",
        };
        const result = merge([failed], [card(1n, "server")]);
        expect(result.slots[0]).toBe(failed);
    });

    test("with a final fresh page, idle cards the server dropped disappear", () => {
        const current = [idle(1n), idle(2n), completed(9n), idle(3n)];
        const result = merge(current, [card(3n)]);
        expect(ids(result)).toEqual([9n, 3n]);
        // Placeholders keep their exact slot objects (live state survives).
        expect(result.slots[0]).toBe(current[2]);
    });

    test("an empty final fresh page clears dropped idle cards but keeps placeholders", () => {
        const current = [idle(1n), completed(2n), awaitingRefill(3n)];
        const result = merge(current, []);
        expect(ids(result)).toEqual([2n, 3n]);
        expect(result.slots[0]).toBe(current[1]);
    });

    test("an empty final fresh page can empty the grid", () => {
        expect(merge([idle(1n)], []).slots).toEqual([]);
    });

    test("with more pages, cards loaded beyond page 1 survive and keep their cursor", () => {
        const current = [idle(1n), idle(2n), idle(3n), idle(4n)];
        // Fresh page 1 holds 1 and 2; 3 and 4 were loaded from later pages.
        const result = merge(
            current,
            [card(1n), card(2n)],
            false,
            MORE,
            LOADED_BEYOND,
        );
        expect(ids(result)).toEqual([1n, 2n, 3n, 4n]);
        expect(result.cursor).toBe(LOADED_BEYOND);
    });

    test("with more pages, a card before the last shared card that vanished is dropped", () => {
        const current = [idle(1n), idle(2n), idle(3n), idle(4n)];
        // 2 vanished (deleted elsewhere); 3 is still the last shared card.
        const result = merge(
            current,
            [card(1n), card(3n)],
            false,
            MORE,
            LOADED_BEYOND,
        );
        expect(ids(result)).toEqual([1n, 3n, 4n]);
        expect(result.cursor).toBe(LOADED_BEYOND);
    });

    test("with more pages and nothing loaded beyond page 1, the fresh cursor wins", () => {
        const result = merge([idle(1n)], [card(1n), card(2n)], false, MORE, MORE);
        expect(result.cursor).toBe(MORE);
    });

    test("a fresh repeatable card replaces a completed placeholder of its topic in place", () => {
        const current = [idle(1n), completed(9n, "rust", "repeatable_tip"), idle(2n)];
        const result = merge(current, [
            card(1n),
            repeatable(10n, "rust"),
            card(2n),
        ]);
        expect(ids(result)).toEqual([1n, 10n, 2n]);
        expect(result.slots[1].kind).toBe("idle");
    });

    test("a refill placeholder is replaced by the new card, never duplicated", () => {
        const refill = {
            ...awaitingRefill(8n),
            topicName: "rust",
        } as ReviewSlot;
        const result = merge([refill], [repeatable(11n, "rust")]);
        expect(ids(result)).toEqual([11n]);
    });

    test("a placeholder of another topic is untouched by a fresh repeatable card", () => {
        const current = [completed(9n, "go", "repeatable_tip")];
        const result = merge(current, [repeatable(11n, "rust")]);
        expect(ids(result)).toEqual([11n, 9n]);
        expect(result.slots[1]).toBe(current[0]);
    });

    test("a repeatable topic that moved on drops its stale idle card even with more pages", () => {
        const stale: ReviewSlot = { kind: "idle", card: repeatable(5n, "rust") };
        const result = merge(
            [idle(1n), stale],
            [card(1n), repeatable(6n, "rust")],
            false,
            MORE,
            LOADED_BEYOND,
        );
        expect(ids(result)).toEqual([6n, 1n]);
    });

    test("skips the merge entirely while a pin/delete/add mutation is in flight", () => {
        const current = [idle(1n)];
        const result = merge(current, [card(2n)], true, END, LOADED_BEYOND);
        expect(result.slots).toBe(current);
        expect(result.cursor).toBe(LOADED_BEYOND);
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
