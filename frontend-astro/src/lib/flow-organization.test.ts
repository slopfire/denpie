import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import {
  FlowCardInfoSchema,
  type FlowCardInfo,
} from "../generated/denpie_pb";
import type { ReviewSlot } from "./flow-review-state";
import {
  FLOW_SORT_STORAGE_KEY,
  organizeFlowSlots,
  parseFlowSortMode,
  slotMetadata,
} from "./flow-organization";

function card(
  id: bigint,
  topicName: string,
  title: string,
  createdAt: string,
  pinned = false,
): FlowCardInfo {
  return create(FlowCardInfoSchema, {
    id,
    title,
    topicName,
    fullContent: `${title} body`,
    tipcardType: "repeatable_tip",
    status: "active",
    createdAt,
    pinned,
  });
}

function completedSlot(
  reviewedCardId: bigint,
  topicName: string,
  title: string,
  createdAt: string,
  pinned = false,
): Extract<ReviewSlot, { kind: "completed" }> {
  return {
    kind: "completed",
    reviewedCardId,
    topicName,
    title,
    createdAt,
    tipcardType: "repeatable_tip",
    pinned,
  };
}

describe("parseFlowSortMode", () => {
  test("accepts the two canonical values", () => {
    expect(parseFlowSortMode("topic")).toBe("topic");
    expect(parseFlowSortMode("date")).toBe("date");
  });

  test("normalizes missing and unknown values to topic", () => {
    expect(parseFlowSortMode(null)).toBe("topic");
    expect(parseFlowSortMode("")).toBe("topic");
    expect(parseFlowSortMode("TITLE")).toBe("topic");
    expect(parseFlowSortMode('{"evil":true}')).toBe("topic");
  });

  test("exposes the canonical storage key", () => {
    expect(FLOW_SORT_STORAGE_KEY).toBe("denpie-flow-sort");
  });
});

describe("organizeFlowSlots", () => {
  const slots: ReviewSlot[] = [
    { kind: "idle", card: card(3n, "beta", "apple", "2026-01-01T00:00:00Z") },
    { kind: "idle", card: card(9n, "alpha", "zulu", "2026-03-01T00:00:00Z") },
    completedSlot(1n, "gamma", "mango", "2026-02-01T00:00:00Z"),
    {
      kind: "awaitingRefill",
      reviewedCardId: 5n,
      topicName: "alpha",
      title: "aaa",
      createdAt: "2026-04-01T00:00:00Z",
      tipcardType: "repeatable_tip",
      pinned: true,
      refillToken: 2,
      refillAttempts: 0,
    },
  ];

  test("splits pinned slots out of the unpinned sort", () => {
    const organized = organizeFlowSlots(slots, "date");
    expect(organized.pinned.map(slotMetadata)).toEqual([
      {
        topicName: "alpha",
        title: "aaa",
        createdAt: "2026-04-01T00:00:00Z",
        id: 5n,
        pinned: true,
      },
    ]);
    // The awaiting-refill placeholder keeps its exact reference.
    expect(organized.pinned[0]).toBe(slots[3]);
    expect(organized.unpinned.map((slot) => slotIdentityOf(slot))).toEqual([
      "9",
      "1",
      "3",
    ]);
  });

  test("renders pinned slots in the exact saved bigint order", () => {
    const aboveSafe = 9_007_199_254_740_993n;
    const pinnedSlots: ReviewSlot[] = [
      { kind: "idle", card: card(1n, "one", "one", "2026-01-01", true) },
      {
        kind: "idle",
        card: card(aboveSafe, "big", "big", "2026-01-02", true),
      },
      { kind: "idle", card: card(3n, "three", "three", "2026-01-03", true) },
    ];
    expect(
      organizeFlowSlots(pinnedSlots, "topic", [1n, aboveSafe, 3n]).pinned.map(
        slotIdentityOf,
      ),
    ).toEqual(["1", String(aboveSafe), "3"]);
  });

  test("topic mode orders by lowercase topic then lowercase title then id", () => {
    const mixed: ReviewSlot[] = [
      { kind: "idle", card: card(7n, "Alpha", "same", "2026-01-05T00:00:00Z") },
      { kind: "idle", card: card(2n, "ALPHA", "SAME", "2026-01-04T00:00:00Z") },
      { kind: "idle", card: card(4n, "beta", "early", "2026-09-09T00:00:00Z") },
      { kind: "idle", card: card(6n, "alpha", "Aardvark", "2026-01-03T00:00:00Z") },
    ];
    expect(
      organizeFlowSlots(mixed, "topic").unpinned.map((slot) =>
        slotIdentityOf(slot),
      ),
    ).toEqual(["6", "2", "7", "4"]);
  });

  test("topic mode uses deterministic lexical ordering, not host locale", () => {
    const mixed: ReviewSlot[] = [
      { kind: "idle", card: card(1n, "ä", "later", "2026-01-01T00:00:00Z") },
      { kind: "idle", card: card(2n, "z", "earlier", "2026-01-01T00:00:00Z") },
    ];
    expect(
      organizeFlowSlots(mixed, "topic").unpinned.map(slotIdentityOf),
    ).toEqual(["2", "1"]);
  });

  test("date mode orders by createdAt descending then id descending", () => {
    const mixed: ReviewSlot[] = [
      { kind: "idle", card: card(8n, "t", "old", "2026-01-02T00:00:00Z") },
      { kind: "idle", card: card(11n, "t", "tie-low", "2026-06-01T00:00:00Z") },
      { kind: "idle", card: card(10n, "t", "tie-high", "2026-06-01T00:00:00Z") },
      { kind: "idle", card: card(12n, "t", "newest", "2026-07-01T00:00:00Z") },
    ];
    expect(
      organizeFlowSlots(mixed, "date").unpinned.map((slot) =>
        slotIdentityOf(slot),
      ),
    ).toEqual(["12", "11", "10", "8"]);
  });

  test("stable ties preserve source order", () => {
    const first = completedSlot(
      7n,
      "same",
      "same",
      "2026-01-01T00:00:00Z",
    );
    const second = completedSlot(
      7n,
      "same",
      "same",
      "2026-01-01T00:00:00Z",
    );
    const ties: ReviewSlot[] = [first, second];
    const before = [...ties];
    const organized = organizeFlowSlots(ties, "topic");
    // Sorting never mutates the input array or its slot objects.
    expect(ties).toEqual(before);
    expect(organized.unpinned).not.toBe(ties);
    expect(organized.unpinned).toEqual([first, second]);
    expect(organized.unpinned.at(0)).toBe(first);
    expect(organized.unpinned.at(1)).toBe(second);
    expect(organized.pinned).toHaveLength(0);
  });

  test("empty input yields empty sections", () => {
    expect(organizeFlowSlots([], "topic")).toEqual({
      pinned: [],
      unpinned: [],
    });
  });

  function slotIdentityOf(slot: ReviewSlot): string {
    return slotMetadata(slot).id.toString();
  }
});
