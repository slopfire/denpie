// Pure Flow organization: pinned/unpinned section split plus the persisted
// topic/date sort preference. No fetch, no component code, no mutation of
// slot objects or the input array.

import type { FlowCardInfo } from "../generated/denpie_pb";
import type { ReviewSlot } from "./flow-review-state";

/** The user-selectable Flow card ordering. */
export type FlowSortMode = "topic" | "date";

/** Canonical localStorage key for Flow sort. */
export const FLOW_SORT_STORAGE_KEY = "denpie-flow-sort";

/**
 * Boundary parser for the stored preference: only `topic` and `date` are
 * valid; missing, unknown, or non-string values normalize to `topic`.
 */
export function parseFlowSortMode(value: string | null): FlowSortMode {
  return value === "date" ? "date" : "topic";
}

function isLiveCard(slot: ReviewSlot): slot is Extract<ReviewSlot, { kind: "idle" | "reviewing" | "error" }> {
  return (
    slot.kind === "idle" || slot.kind === "reviewing" || slot.kind === "error"
  );
}

type SlotMetadata = {
  topicName: string;
  title: string;
  createdAt: string;
  id: bigint;
  pinned: boolean;
};

/** Read-only view of a slot's sort/pin metadata across every variant. */
export function slotMetadata(slot: ReviewSlot): SlotMetadata {
  if (isLiveCard(slot)) {
    const card: FlowCardInfo = slot.card;
    return {
      topicName: card.topicName,
      title: card.title,
      createdAt: card.createdAt,
      id: card.id,
      pinned: card.pinned,
    };
  }
  return {
    topicName: slot.topicName,
    title: slot.title,
    createdAt: slot.createdAt,
    id: slot.reviewedCardId,
    pinned: slot.pinned,
  };
}

function compareBigints(a: bigint, b: bigint): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

function compareStrings(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/**
 * Topic mode: topicName lowercase ascending, then title lowercase ascending,
 * then bigint ID ascending.
 */
function compareTopic(
  a: SlotMetadata,
  b: SlotMetadata,
): number {
  const byTopic = compareStrings(
    a.topicName.toLowerCase(),
    b.topicName.toLowerCase(),
  );
  if (byTopic !== 0) return byTopic;
  const byTitle = compareStrings(
    a.title.toLowerCase(),
    b.title.toLowerCase(),
  );
  if (byTitle !== 0) return byTitle;
  return compareBigints(a.id, b.id);
}

/**
 * Date mode: createdAt descending (ISO timestamps compare
 * lexicographically), then bigint ID descending.
 */
function compareDate(a: SlotMetadata, b: SlotMetadata): number {
  const byDate = compareStrings(b.createdAt, a.createdAt);
  if (byDate !== 0) return byDate;
  return compareBigints(b.id, a.id);
}

/**
 * Split slots into the pinned section and the sorted unpinned list. Pinned
 * slots render in the normalized saved drag order (`savedPinnedOrder`, the
 * `denpie-pinned-card-order` bigint array; IDs absent from it keep current
 * source order after the saved ones); without it they keep source order.
 * The unpinned list sorts stably under `mode` (ties preserve source order).
 * Neither the input arrays nor any slot object is mutated.
 */
export function organizeFlowSlots(
  slots: readonly ReviewSlot[],
  mode: FlowSortMode,
  savedPinnedOrder?: readonly bigint[],
): { pinned: ReviewSlot[]; unpinned: ReviewSlot[] } {
  const pinned: ReviewSlot[] = [];
  const unpinned: ReviewSlot[] = [];
  for (const slot of slots) {
    (slotMetadata(slot).pinned ? pinned : unpinned).push(slot);
  }
  if (savedPinnedOrder !== undefined && savedPinnedOrder.length > 0) {
    const rank = new Map<bigint, number>();
    for (const [index, id] of savedPinnedOrder.entries()) {
      rank.set(id, index);
    }
    pinned.sort((a, b) => {
      const aRank = rank.get(slotMetadata(a).id);
      const bRank = rank.get(slotMetadata(b).id);
      // Unranked IDs keep source order behind the ranked ones.
      if (aRank === undefined && bRank === undefined) return 0;
      if (aRank === undefined) return 1;
      if (bRank === undefined) return -1;
      return aRank - bRank;
    });
  }
  const comparator = mode === "topic" ? compareTopic : compareDate;
  unpinned.sort((a, b) => comparator(slotMetadata(a), slotMetadata(b)));
  return { pinned, unpinned };
}
