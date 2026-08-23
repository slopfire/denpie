// Pure silent-refresh merge for the Flow grid: a background page-1 refetch is
// integrated into the currently rendered slots without leaving ready state.
// No fetch, no React — the component owns when refreshes run and whether any
// mutation is in flight; this module owns what the merged slot list is.

import type { FlowCardInfo } from "../generated/denpie_pb";
import type { FlowCursor } from "./flow-state";
import type { ReviewSlot } from "./flow-review-state";

/** Inputs of one silent refresh integration. */
export interface FlowRefreshMergeInput {
    /** The slots currently on screen (ready/loading-more/load-error). */
    currentSlots: readonly ReviewSlot[];
    /** Cards from the freshly fetched page 1, in fresh server order. */
    freshPageCards: readonly FlowCardInfo[];
    /** Cursor carried by the fresh page. */
    cursor: FlowCursor;
    /**
     * True while any optimistic mutation owns a card (pin, delete, add, or a
     * pending review/continue slot): the merge is skipped entirely so an
     * authoritative in-flight result cannot be clobbered by stale list data.
     */
    mutationsInFlight: boolean;
}

/** Result of one silent refresh integration. */
export interface FlowRefreshMergeResult {
    /** Merged slots; reference-equal to `currentSlots` when nothing changed. */
    slots: readonly ReviewSlot[];
    /** Fresh cursor; meaningful only when the merge was not skipped. */
    cursor: FlowCursor;
}

function slotCardId(slot: ReviewSlot): bigint {
    return slot.kind === "idle" ||
        slot.kind === "reviewing" ||
        slot.kind === "error"
        ? slot.card.id
        : slot.reviewedCardId;
}

/**
 * Merge a freshly fetched page 1 into the rendered slots:
 *
 * - Fresh cards win: every card present in the fresh page renders at its
 *   fresh position with fresh fields (a new idle slot replaces the old one).
 * - Loaded-beyond-page-1 cards and live placeholders (completed,
 *   awaitingRefill, …) absent from the fresh page survive, appended after
 *   the fresh ones in their prior relative order.
 * - Deduped by bigint ID; duplicate IDs inside the fresh page keep the first.
 * - An empty fresh page keeps current slots verbatim (same array reference):
 *   a quiet empty read must never clear the grid.
 * - While `mutationsInFlight`, the merge is skipped entirely (same
 *   references) so no in-flight mutation result can be clobbered.
 */
export function mergeFlowRefresh(
    input: FlowRefreshMergeInput,
): FlowRefreshMergeResult {
    const { currentSlots, freshPageCards, cursor, mutationsInFlight } = input;
    if (mutationsInFlight || freshPageCards.length === 0) {
        return { slots: currentSlots, cursor };
    }
    // Prior relative order of everything not re-served by the fresh page.
    const priorOrder: ReviewSlot[] = [];
    const freshIds = new Set<bigint>();
    for (const card of freshPageCards) {
        if (!freshIds.has(card.id)) freshIds.add(card.id);
    }
    for (const slot of currentSlots) {
        if (!freshIds.has(slotCardId(slot))) priorOrder.push(slot);
    }
    const nextSlots: ReviewSlot[] = [];
    for (const card of freshPageCards) {
        if (freshIds.has(card.id)) {
            freshIds.delete(card.id); // emit each fresh ID exactly once
            nextSlots.push({ kind: "idle", card });
        }
    }
    nextSlots.push(...priorOrder);
    if (
        nextSlots.length === currentSlots.length &&
        nextSlots.every((slot, index) => slot === currentSlots[index])
    ) {
        return { slots: currentSlots, cursor };
    }
    return { slots: nextSlots, cursor };
}
