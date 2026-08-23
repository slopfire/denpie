// Pure integration of resolved add-card details into Flow slots. The caller
// owns fetches and React commits; this module only describes an atomic next
// slots/order result and whether a quiet authoritative reconciliation is due.

import type { FlowCardInfo } from "../generated/denpie_pb";
import { transferPinnedCards } from "./flow-pinned-order";
import { slotMetadata } from "./flow-organization";
import type { ReviewSlot } from "./flow-review-state";

export interface AddIntegrationInput {
    slots: readonly ReviewSlot[];
    cards: readonly FlowCardInfo[];
    pinnedOrder: readonly bigint[];
    /** IDs with a live pin or delete mutation; their detail must not be replaced. */
    busyCardIds: readonly bigint[];
}

export interface AddIntegrationResult {
    slots: ReviewSlot[];
    pinnedOrder: readonly bigint[];
    /** True when the quiet list fetch must authoritatively fill a deferred gap. */
    needsReconciliation: boolean;
    /** Detail IDs deliberately not integrated because a live slot owns them. */
    deferredCardIds: readonly bigint[];
}

function isIdle(
    slot: ReviewSlot,
): slot is Extract<ReviewSlot, { kind: "idle" }> {
    return slot.kind === "idle";
}

function isLiveCard(
    slot: ReviewSlot,
): slot is Extract<ReviewSlot, { card: FlowCardInfo }> {
    return (
        slot.kind === "idle" ||
        slot.kind === "reviewing" ||
        slot.kind === "error"
    );
}

function isActive(card: FlowCardInfo): boolean {
    return card.status === "active";
}

function isRepeatable(card: FlowCardInfo): boolean {
    return card.tipcardType === "repeatable_tip";
}

function isCasualOrManual(card: FlowCardInfo): boolean {
    return (
        card.tipcardType === "casual_tip" || card.tipcardType === "manual_tip"
    );
}

function sameSlotId(slot: ReviewSlot, cardId: bigint): boolean {
    return slotMetadata(slot).id === cardId;
}

function repeatableTopicIndex(
    slots: readonly ReviewSlot[],
    topicName: string,
): number {
    return slots.findIndex(
        (slot) =>
            isLiveCard(slot) &&
            isRepeatable(slot.card) &&
            slot.card.topicName === topicName,
    );
}

/**
 * Integrate a resolved add batch without disturbing work already owned by a
 * review, pin, or delete request. Casual/manual active cards append or refresh
 * exactly one idle same-ID slot. A repeatable card replaces one idle active
 * same-topic repeatable card, preserving that card's pin flag and transferring
 * every saved-order position in one all-or-nothing batch.
 */
export function integrateCreatedCards({
    slots,
    cards,
    pinnedOrder,
    busyCardIds,
}: AddIntegrationInput): AddIntegrationResult {
    const next = [...slots];
    const busyIds = new Set(busyCardIds);
    const seenIds = new Set<bigint>();
    const seenRepeatableTopics = new Set<string>();
    const transfers: { from: bigint; to: bigint }[] = [];
    const deferredCardIds: bigint[] = [];
    let needsReconciliation = false;

    function defer(cardId: bigint): void {
        if (!deferredCardIds.includes(cardId)) deferredCardIds.push(cardId);
        needsReconciliation = true;
    }

    for (const card of cards) {
        if (seenIds.has(card.id)) continue;
        seenIds.add(card.id);
        if (!isActive(card)) {
            defer(card.id);
            continue;
        }

        const sameIdIndex = next.findIndex((slot) => sameSlotId(slot, card.id));
        if (sameIdIndex !== -1) {
            const existing = next[sameIdIndex];
            if (isIdle(existing) && !busyIds.has(card.id)) {
                next[sameIdIndex] = { kind: "idle", card };
            } else {
                defer(card.id);
            }
            continue;
        }

        if (isCasualOrManual(card)) {
            next.push({ kind: "idle", card });
            continue;
        }

        if (!isRepeatable(card)) {
            defer(card.id);
            continue;
        }

        needsReconciliation = true;
        if (seenRepeatableTopics.has(card.topicName)) {
            defer(card.id);
            continue;
        }
        seenRepeatableTopics.add(card.topicName);

        const previousIndex = repeatableTopicIndex(next, card.topicName);
        if (previousIndex === -1) {
            next.push({ kind: "idle", card });
            continue;
        }
        const previous = next[previousIndex];
        if (!isIdle(previous) || !isActive(previous.card)) {
            defer(card.id);
            continue;
        }
        if (busyIds.has(previous.card.id) || busyIds.has(card.id)) {
            defer(card.id);
            continue;
        }

        const proposedTransfers = previous.card.pinned
            ? [...transfers, { from: previous.card.id, to: card.id }]
            : transfers;
        const transfer = transferPinnedCards(pinnedOrder, proposedTransfers);
        if (transfer.kind === "collision") {
            defer(card.id);
            continue;
        }
        transfers.length = 0;
        transfers.push(...proposedTransfers);
        next[previousIndex] = {
            kind: "idle",
            card: { ...card, pinned: previous.card.pinned },
        };
    }

    const pinnedTransfer = transferPinnedCards(pinnedOrder, transfers);
    // A collision was ruled out before each replacement. Keeping the original
    // order on an unexpected collision is safer than a partial transfer.
    const nextPinnedOrder =
        pinnedTransfer.kind === "applied" ? pinnedTransfer.order : pinnedOrder;
    return {
        slots: next,
        pinnedOrder: nextPinnedOrder,
        needsReconciliation,
        deferredCardIds,
    };
}

/**
 * Quiet authoritative reconciliation. It refreshes only idle same-ID slots;
 * all in-flight/placeholder references remain untouched and cannot be
 * duplicated by an incoming page.
 */
export function mergeReconciledCards(
    slots: readonly ReviewSlot[],
    cards: readonly FlowCardInfo[],
): ReviewSlot[] {
    const byId = new Map<bigint, FlowCardInfo>();
    for (const card of cards) byId.set(card.id, card);
    const next = slots.map((slot): ReviewSlot => {
        if (!isIdle(slot)) return slot;
        const reconciled = byId.get(slot.card.id);
        return reconciled === undefined
            ? slot
            : { kind: "idle", card: reconciled };
    });
    const representedIds = new Set(next.map((slot) => slotMetadata(slot).id));
    for (const card of cards) {
        if (representedIds.has(card.id)) continue;
        representedIds.add(card.id);
        next.push({ kind: "idle", card });
    }
    return next;
}
