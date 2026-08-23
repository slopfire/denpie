import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { FlowCardInfoSchema } from "../generated/denpie_pb";
import {
    integrateCreatedCards,
    mergeReconciledCards,
} from "./flow-add-integration";
import { slotsFromCards } from "./flow-review-actions";
import type { ReviewSlot } from "./flow-review-state";

function card(
    id: bigint,
    options: {
        topicName?: string;
        tipcardType?: string;
        status?: string;
        pinned?: boolean;
        title?: string;
    } = {},
) {
    return create(FlowCardInfoSchema, {
        id,
        topicName: options.topicName ?? "Rust",
        title: options.title ?? `card-${id}`,
        fullContent: "body",
        tipcardType: options.tipcardType ?? "casual_tip",
        status: options.status ?? "active",
        pinned: options.pinned ?? false,
    });
}

function slots(...cards: ReturnType<typeof card>[]): ReviewSlot[] {
    return slotsFromCards(cards);
}

function reviewing(slot: ReviewSlot): ReviewSlot {
    if (slot.kind !== "idle") throw new TypeError("expected idle slot");
    return {
        kind: "reviewing",
        card: slot.card,
        generation: 1,
        attempt: { grade: 3, action: null, idempotencyKey: "review-key" },
    };
}

describe("integrateCreatedCards", () => {
    test("appends and batch-deduplicates active casual/manual cards", () => {
        const casual = card(1n, { tipcardType: "casual_tip" });
        const manual = card(2n, { tipcardType: "manual_tip" });
        const result = integrateCreatedCards({
            slots: [],
            cards: [casual, casual, manual],
            pinnedOrder: [],
            busyCardIds: [],
        });
        expect(result.slots).toEqual([
            { kind: "idle", card: casual },
            { kind: "idle", card: manual },
        ]);
        expect(result.needsReconciliation).toBe(false);
    });

    test("defers inactive and unsupported created details instead of rendering them", () => {
        const inactive = card(1n, { status: "archived" });
        const unsupported = card(2n, { tipcardType: "future_tip" });
        const result = integrateCreatedCards({
            slots: [],
            cards: [inactive, unsupported],
            pinnedOrder: [],
            busyCardIds: [],
        });
        expect(result.slots).toEqual([]);
        expect(result.deferredCardIds).toEqual([1n, 2n]);
        expect(result.needsReconciliation).toBe(true);
    });

    test("same-ID refreshes only an idle slot and preserves unrelated references", () => {
        const original = card(1n, { title: "old" });
        const unrelated = card(2n);
        const base = slots(original, unrelated);
        const refreshed = card(1n, { title: "new" });
        const result = integrateCreatedCards({
            slots: base,
            cards: [refreshed],
            pinnedOrder: [],
            busyCardIds: [],
        });
        expect(result.slots[0]).toEqual({ kind: "idle", card: refreshed });
        expect(result.slots[1]).toBe(base[1]);
    });

    test("defers same-ID details owned by review or a pin/delete mutation", () => {
        const reviewed = card(1n);
        const busy = card(2n);
        const base = [reviewing(slots(reviewed)[0]), slots(busy)[0]];
        const result = integrateCreatedCards({
            slots: base,
            cards: [
                card(1n, { title: "fresh review" }),
                card(2n, { title: "fresh busy" }),
            ],
            pinnedOrder: [],
            busyCardIds: [2n],
        });
        expect(result.slots).toEqual(base);
        expect(result.deferredCardIds).toEqual([1n, 2n]);
        expect(result.needsReconciliation).toBe(true);
    });

    test("replaces multiple safe repeatable topics and transfers pinned order atomically", () => {
        const first = card(1n, {
            topicName: "Rust",
            tipcardType: "repeatable_tip",
            pinned: true,
        });
        const second = card(2n, {
            topicName: "Zig",
            tipcardType: "repeatable_tip",
            pinned: true,
        });
        const base = slots(first, second, card(3n));
        const firstNew = card(11n, {
            topicName: "Rust",
            tipcardType: "repeatable_tip",
        });
        const secondNew = card(12n, {
            topicName: "Zig",
            tipcardType: "repeatable_tip",
        });
        const result = integrateCreatedCards({
            slots: base,
            cards: [firstNew, secondNew],
            pinnedOrder: [2n, 1n],
            busyCardIds: [],
        });
        expect(result.slots).toEqual([
            { kind: "idle", card: { ...firstNew, pinned: true } },
            { kind: "idle", card: { ...secondNew, pinned: true } },
            base[2],
        ]);
        expect(result.pinnedOrder).toEqual([12n, 11n]);
        expect(result.needsReconciliation).toBe(true);
    });

    test("never duplicates a repeatable topic when its existing slot is non-idle or busy", () => {
        const old = card(1n, {
            topicName: "Rust",
            tipcardType: "repeatable_tip",
            pinned: true,
        });
        const incoming = card(11n, {
            topicName: "Rust",
            tipcardType: "repeatable_tip",
        });
        const reviewed = [reviewing(slots(old)[0])];
        const reviewResult = integrateCreatedCards({
            slots: reviewed,
            cards: [incoming],
            pinnedOrder: [1n],
            busyCardIds: [],
        });
        expect(reviewResult.slots).toEqual(reviewed);
        expect(reviewResult.deferredCardIds).toEqual([11n]);

        const idle = slots(old);
        const busyResult = integrateCreatedCards({
            slots: idle,
            cards: [incoming],
            pinnedOrder: [1n],
            busyCardIds: [1n],
        });
        expect(busyResult.slots).toEqual(idle);
        expect(busyResult.deferredCardIds).toEqual([11n]);
    });

    test("keeps only one repeatable topic when the resolved batch itself collides", () => {
        const first = card(11n, {
            topicName: "Rust",
            tipcardType: "repeatable_tip",
        });
        const duplicate = card(12n, {
            topicName: "Rust",
            tipcardType: "repeatable_tip",
        });
        const result = integrateCreatedCards({
            slots: [],
            cards: [first, duplicate],
            pinnedOrder: [],
            busyCardIds: [],
        });
        expect(result.slots).toEqual([{ kind: "idle", card: first }]);
        expect(result.deferredCardIds).toEqual([12n]);
        expect(result.needsReconciliation).toBe(true);
    });

    test("defers a repeatable replacement whose saved-order destination collides", () => {
        const old = card(1n, {
            topicName: "Rust",
            tipcardType: "repeatable_tip",
            pinned: true,
        });
        const incoming = card(2n, {
            topicName: "Rust",
            tipcardType: "repeatable_tip",
        });
        const base = slots(old);
        const result = integrateCreatedCards({
            slots: base,
            cards: [incoming],
            pinnedOrder: [1n, 2n],
            busyCardIds: [],
        });
        expect(result.slots).toEqual(base);
        expect(result.pinnedOrder).toEqual([1n, 2n]);
        expect(result.deferredCardIds).toEqual([2n]);
    });
});

describe("mergeReconciledCards", () => {
    test("refreshes idle cards but never duplicates a non-idle same-ID slot", () => {
        const active = card(1n, { title: "old" });
        const reviewingCard = card(2n, { title: "reviewing" });
        const base = [slots(active)[0], reviewing(slots(reviewingCard)[0])];
        const refreshed = card(1n, { title: "new" });
        const result = mergeReconciledCards(base, [
            refreshed,
            card(2n, { title: "server" }),
            card(3n),
        ]);
        expect(result).toEqual([
            { kind: "idle", card: refreshed },
            base[1],
            { kind: "idle", card: card(3n) },
        ]);
    });
});
