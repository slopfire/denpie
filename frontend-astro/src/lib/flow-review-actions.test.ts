// Bun tests for the pure Flow review action mapping and slot-list helpers.
import { describe, expect, test } from "bun:test";
import { ReviewActionValue, type FlowCardInfo } from "../generated/denpie_pb";
import { TransportError } from "./api-v1/transport";
import {
  appendIdleSlots,
  classifyReviewError,
  reviewActionsFor,
  slotsFromCards,
} from "./flow-review-actions";

function card(overrides: Partial<FlowCardInfo> = {}): FlowCardInfo {
  return {
    id: 1n,
    title: "T",
    topicName: "topic",
    fullContent: "body",
    compressedContent: "",
    images: [],
    pinned: false,
    repeatCount: 0,
    pendingCount: 0n,
    tipcardType: "custom_tip",
    status: "active",
    ...overrides,
  };
}

describe("reviewActionsFor", () => {
  test("casual_tip: dismiss then acknowledge", () => {
    const a = reviewActionsFor(card({ tipcardType: "casual_tip" }));
    expect(a.skipGroup).toBeUndefined();
    expect(a.primary).toEqual([
      { id: "dismiss", label: "Dismiss", grade: 3, action: ReviewActionValue.SKIP_NOT_INTERESTED },
      { id: "acknowledge", label: "Acknowledge", grade: 3, action: ReviewActionValue.ACKNOWLEDGE },
    ]);
  });

  test("manual_tip matches casual_tip mapping", () => {
    const a = reviewActionsFor(card({ tipcardType: "manual_tip" }));
    expect(a.primary[0]).toEqual({
      id: "dismiss",
      label: "Dismiss",
      grade: 3,
      action: ReviewActionValue.SKIP_NOT_INTERESTED,
    });
    expect(a.primary[1]?.action).toBe(ReviewActionValue.ACKNOWLEDGE);
  });

  test("repeatable_tip: again/learned plus grouped skip reasons", () => {
    const a = reviewActionsFor(card({ tipcardType: "repeatable_tip" }));
    expect(a.primary).toEqual([
      { id: "again", label: "Again", grade: 1, action: ReviewActionValue.AGAIN },
      { id: "learned", label: "Learned", grade: 5, action: ReviewActionValue.LEARNED },
    ]);
    expect(a.skipGroup).toEqual([
      { id: "known", label: "Known", grade: 5, action: ReviewActionValue.SKIP_KNOWN },
      { id: "not-interested", label: "Not interested", grade: 3, action: ReviewActionValue.SKIP_NOT_INTERESTED },
      { id: "too-difficult", label: "Too difficult", grade: 1, action: ReviewActionValue.SKIP_TOO_DIFFICULT },
    ]);
  });

  test("other active types grade without named actions", () => {
    for (const type of ["custom_tip", "", "unknown_future_type"]) {
      const a = reviewActionsFor(card({ tipcardType: type }));
      expect(a.skipGroup).toBeUndefined();
      expect(a.primary).toEqual([
        { id: "again", label: "Again", grade: 1, action: ReviewActionValue.UNSPECIFIED },
        { id: "good", label: "Good", grade: 3, action: ReviewActionValue.UNSPECIFIED },
        { id: "easy", label: "Easy", grade: 5, action: ReviewActionValue.UNSPECIFIED },
      ]);
    }
  });
});

describe("slotsFromCards / appendIdleSlots", () => {
  test("initial page becomes idle slots in order", () => {
    const slots = slotsFromCards([card({ id: 1n }), card({ id: 2n })]);
    expect(slots.map((s) => (s.kind === "idle" ? s.card.id : null))).toEqual([1n, 2n]);
  });

  test("load-more keeps existing slot states and appends new idle slots", () => {
    const initial = slotsFromCards([card({ id: 1n }), card({ id: 2n })]);
    const reviewing = initial.map((slot, index) =>
      index === 0
        ? {
            kind: "reviewing" as const,
            card: card({ id: 1n }),
            generation: 1,
            attempt: { grade: 3, action: ReviewActionValue.AGAIN, idempotencyKey: "k1" },
          }
        : slot,
    );
    const next = appendIdleSlots(reviewing, [
      card({ id: 2n }), // duplicate of an existing idle slot
      card({ id: 3n }),
    ]);
    expect(next).toHaveLength(3);
    expect(next[0]?.kind).toBe("reviewing");
    expect(next[2]).toEqual({ kind: "idle", card: card({ id: 3n }) });
  });

  test("load-more deduplicates reviewing, placeholder, and incoming ids", () => {
    const slots = [
      {
        kind: "reviewing" as const,
        card: card({ id: 1n }),
        generation: 1,
        attempt: {
          grade: 1,
          action: ReviewActionValue.AGAIN,
          idempotencyKey: "k1",
        },
      },
      {
        kind: "awaitingRefill" as const,
        reviewedCardId: 2n,
        topicName: "topic",
        tipcardType: "repeatable_tip",
        pinned: false,
      },
    ];
    const next = appendIdleSlots(slots, [
      card({ id: 1n }),
      card({ id: 2n }),
      card({ id: 3n }),
      card({ id: 3n }),
    ]);
    expect(next.slice(0, 2)).toEqual(slots);
    expect(next).toHaveLength(3);
    expect(next[2]).toEqual({ kind: "idle", card: card({ id: 3n }) });
  });
});

describe("classifyReviewError", () => {

  test("TransportError keeps its own indeterminate verdict", () => {
    const indeterminate = new TransportError({
      status: 504,
      message: "gateway timeout",
      retryable: true,
      mutationOutcomeIndeterminate: true,
      requestId: "r1",
    });
    expect(classifyReviewError(indeterminate)).toEqual({
      mutationOutcomeIndeterminate: true,
      message: "gateway timeout",
    });
    const determinate = new TransportError({
      status: 400,
      message: "bad request",
      retryable: false,
      mutationOutcomeIndeterminate: false,
      requestId: "r2",
    });
    expect(classifyReviewError(determinate).mutationOutcomeIndeterminate).toBe(false);
  });

  test("non-transport errors conservatively preserve the mutation attempt", () => {
    expect(classifyReviewError(new Error("boom"))).toEqual({
      mutationOutcomeIndeterminate: true,
      message: "boom",
    });
    expect(classifyReviewError("plain")).toEqual({
      mutationOutcomeIndeterminate: true,
      message: "plain",
    });
  });
});
