// Pure silent-refresh merge for the Flow grid: a background page-1 refetch is
// reconciled with the currently rendered slots without leaving ready state.
// No fetch, no React — the component owns when refreshes run and whether any
// mutation is in flight; this module owns what the merged slot list is.

import { equals } from "@bufbuild/protobuf";
import { FlowCardInfoSchema, type FlowCardInfo } from "../generated/denpie_pb";
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
     * Cursor the rendered grid is paginated with, when it is known. Pages
     * loaded beyond page 1 stay on screen, so their cursor stays too: the
     * fresh page-1 cursor would send "Load more" back to page 2.
     */
    previousCursor?: FlowCursor;
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
    /** Cursor the merged grid paginates with. */
    cursor: FlowCursor;
}

type Placeholder = Extract<ReviewSlot, { kind: "completed" | "awaitingRefill" }>;

function slotCardId(slot: ReviewSlot): bigint {
    return slot.kind === "idle" ||
        slot.kind === "reviewing" ||
        slot.kind === "error"
        ? slot.card.id
        : slot.reviewedCardId;
}

function isRepeatableCard(card: FlowCardInfo): boolean {
    return card.tipcardType === "repeatable_tip";
}

/** A consumed repeatable card waiting for its topic's next card. */
function openPlaceholder(slot: ReviewSlot): slot is Placeholder {
    return (
        (slot.kind === "completed" || slot.kind === "awaitingRefill") &&
        slot.tipcardType === "repeatable_tip"
    );
}

/**
 * Reconcile a freshly fetched page 1 with the rendered slots. The server is
 * authoritative for every card it serves; local state wins only for work the
 * server cannot know about.
 *
 * - A fresh card replaces its slot in place with fresh fields. A slot whose
 *   card is structurally equal is reused, so a quiet refresh changes nothing
 *   and returns the same array. A failed-review slot keeps its error and
 *   retry. Cards not on screen yet are prepended in fresh order.
 * - A repeatable topic owns one slot. A fresh repeatable card takes over a
 *   consumed-card placeholder (completed / awaiting refill) of its topic in
 *   place, which is how newly loaded cards reach a finished topic.
 * - Idle cards the fresh page no longer serves are gone (deleted, reviewed
 *   elsewhere, topic removed) and are dropped when it is provable: the fresh
 *   page is the whole list, the card sorts before the last card both sides
 *   share, or its repeatable topic now serves a different card. Cards loaded
 *   beyond page 1 that are not provably gone survive.
 * - Placeholders and in-flight slots survive untouched.
 * - While `mutationsInFlight`, the merge is skipped (same references).
 */
export function mergeFlowRefresh(
    input: FlowRefreshMergeInput,
): FlowRefreshMergeResult {
    const {
        currentSlots,
        freshPageCards,
        cursor,
        previousCursor,
        mutationsInFlight,
    } = input;
    if (mutationsInFlight) {
        return { slots: currentSlots, cursor: previousCursor ?? cursor };
    }

    const fresh = new Map<bigint, FlowCardInfo>();
    const freshRepeatableByTopic = new Map<string, FlowCardInfo>();
    for (const card of freshPageCards) {
        if (fresh.has(card.id)) continue;
        fresh.set(card.id, card);
        if (isRepeatableCard(card) && !freshRepeatableByTopic.has(card.topicName)) {
            freshRepeatableByTopic.set(card.topicName, card);
        }
    }

    let lastSharedIndex = -1;
    const currentIds = new Set<bigint>();
    for (const [index, slot] of currentSlots.entries()) {
        const id = slotCardId(slot);
        currentIds.add(id);
        if (fresh.has(id)) lastSharedIndex = index;
    }

    const placed = new Set<bigint>();
    const merged: ReviewSlot[] = [];
    let keptBeyondFreshPage = false;
    for (const [index, slot] of currentSlots.entries()) {
        const id = slotCardId(slot);
        const freshCard = fresh.get(id);
        if (freshCard !== undefined) {
            placed.add(id);
            if (slot.kind === "error") {
                merged.push(slot);
            } else if (
                slot.kind === "idle" &&
                equals(FlowCardInfoSchema, slot.card, freshCard)
            ) {
                merged.push(slot);
            } else {
                merged.push({ kind: "idle", card: freshCard });
            }
            continue;
        }
        if (openPlaceholder(slot)) {
            const successor = freshRepeatableByTopic.get(slot.topicName);
            if (
                successor !== undefined &&
                !placed.has(successor.id) &&
                !currentIds.has(successor.id)
            ) {
                placed.add(successor.id);
                merged.push({ kind: "idle", card: successor });
                continue;
            }
        }
        if (slot.kind === "idle") {
            const topicSuccessor = isRepeatableCard(slot.card)
                ? freshRepeatableByTopic.get(slot.card.topicName)
                : undefined;
            // Absent from the fresh page, so any fresh same-topic card is a
            // different one: the topic has moved on.
            const gone =
                cursor.kind === "end" ||
                index < lastSharedIndex ||
                topicSuccessor !== undefined;
            if (gone) continue;
            keptBeyondFreshPage = true;
        }
        merged.push(slot);
    }

    const additions: ReviewSlot[] = [];
    for (const card of fresh.values()) {
        if (!placed.has(card.id)) additions.push({ kind: "idle", card });
    }
    const nextSlots = [...additions, ...merged];

    const nextCursor =
        keptBeyondFreshPage &&
        cursor.kind === "more" &&
        previousCursor !== undefined
            ? previousCursor
            : cursor;
    if (
        nextSlots.length === currentSlots.length &&
        nextSlots.every((slot, index) => slot === currentSlots[index])
    ) {
        return { slots: currentSlots, cursor: nextCursor };
    }
    return { slots: nextSlots, cursor: nextCursor };
}
