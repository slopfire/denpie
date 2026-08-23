// Pure Transmission topic-pick partitioning. Pinned slots are deliberately
// not handled here. The caller passes only the unpinned slots it wants to
// organize. Extra repeatable slots collapse behind the selected topic card
// instead of reappearing in Other cards.

import { slotMetadata } from "./flow-organization";
import { slotIdentity, type ReviewSlot } from "./flow-review-state";

export const TRANSMISSION_MAX_PICKS = 9;
export const TRANSMISSION_MAX_PICKS_PER_TOPIC = 3;

type LiveReviewSlot = Extract<
    ReviewSlot,
    { kind: "idle" | "reviewing" | "error" }
>;

function isLiveSlot(slot: ReviewSlot): slot is LiveReviewSlot {
    return (
        slot.kind === "idle" ||
        slot.kind === "reviewing" ||
        slot.kind === "error"
    );
}

function slotTipcardType(slot: ReviewSlot): string {
    return isLiveSlot(slot) ? slot.card.tipcardType : slot.tipcardType;
}

function isActiveLiveSlot(slot: ReviewSlot): slot is LiveReviewSlot {
    return isLiveSlot(slot) && slot.card.status === "active";
}

function isRepeatableSlot(slot: ReviewSlot): boolean {
    return slotTipcardType(slot) === "repeatable_tip";
}

function activeRepeatableTopics(slots: readonly ReviewSlot[]): Set<string> {
    const topics = new Set<string>();
    for (const slot of slots) {
        if (!isActiveLiveSlot(slot) || !isRepeatableSlot(slot)) continue;
        topics.add(slotMetadata(slot).topicName);
    }
    return topics;
}

function isEligibleSlot(
    slot: ReviewSlot,
    activeTopics: ReadonlySet<string>,
): boolean {
    if (isActiveLiveSlot(slot)) return true;
    return (
        isRepeatableSlot(slot) &&
        !isLiveSlot(slot) &&
        !activeTopics.has(slotMetadata(slot).topicName)
    );
}

function pickIndexes(slots: readonly ReviewSlot[]): number[] {
    const activeTopics = activeRepeatableTopics(slots);
    const topicCount = new Set<string>();
    for (const slot of slots) {
        if (!isEligibleSlot(slot, activeTopics)) continue;
        topicCount.add(slotMetadata(slot).topicName);
    }
    if (topicCount.size === 0) return [];

    // Integer division: a single topic still receives at most three picks,
    // while four or more topics adapt to two or one pick each so the total
    // stays at nine.
    const perTopicLimit = Math.min(
        TRANSMISSION_MAX_PICKS_PER_TOPIC,
        Math.max(1, Math.floor(TRANSMISSION_MAX_PICKS / topicCount.size)),
    );
    const topicCounts = new Map<string, number>();
    // Keep the selected slot identity with the topic. This matters for React
    // placeholders because their identity is the reviewed card's bigint ID,
    // not a synthetic array position.
    const repeatableSelections = new Map<string, string>();
    const indexes: number[] = [];

    for (const [index, slot] of slots.entries()) {
        if (!isEligibleSlot(slot, activeTopics)) continue;
        const metadata = slotMetadata(slot);
        const topic = metadata.topicName;
        const repeatable = isRepeatableSlot(slot);
        if (repeatable && repeatableSelections.get(topic) !== undefined) {
            continue;
        }

        const count = topicCounts.get(topic) ?? 0;
        const cardLimit = repeatable ? 1 : perTopicLimit;
        if (count >= cardLimit) continue;

        indexes.push(index);
        topicCounts.set(topic, count + 1);
        if (repeatable) repeatableSelections.set(topic, slotIdentity(slot));
        if (indexes.length === TRANSMISSION_MAX_PICKS) break;
    }

    return indexes;
}

/** Select up to nine eligible slots in source order, balanced by topic. */
export function selectTopicPicks(slots: readonly ReviewSlot[]): ReviewSlot[] {
    return pickIndexes(slots).map((index) => slots[index]);
}

export interface TopicPickPartition {
    picks: ReviewSlot[];
    remaining: ReviewSlot[];
}

/**
 * Partition the source slots without mutation. `remaining` is the source-order
 * complement for non-repeatable cards. Unselected repeatable slots stay hidden
 * behind their one topic pick.
 */
export function splitTopicPicks(
    slots: readonly ReviewSlot[],
): TopicPickPartition {
    const pickIndexSet = new Set(pickIndexes(slots));
    const picks: ReviewSlot[] = [];
    const remaining: ReviewSlot[] = [];
    for (const [index, slot] of slots.entries()) {
        if (pickIndexSet.has(index)) {
            picks.push(slot);
        } else if (!isRepeatableSlot(slot)) {
            remaining.push(slot);
        }
    }
    return { picks, remaining };
}
