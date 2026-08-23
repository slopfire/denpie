import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import {
  FlowCardInfoSchema,
  ReviewActionValue,
  type FlowCardInfo,
} from "../generated/denpie_pb";
import {
  continueFailure,
  continueRetryDecision,
  continueSuccess,
  flowSlotKey,
  refillPollFound,
  refillPollMiss,
  reviewFailure,
  reviewSuccess,
  retryDecision,
  slotIdentity,
  startContinue,
  startReview,
  type ContinueAttempt,
  type ReviewAttempt,
  type ReviewSlot,
  type ReviewSuccessFields,
} from "./flow-review-state";

function card(id: bigint, pinned = false): FlowCardInfo {
  return create(FlowCardInfoSchema, {
    id,
    title: `card-${id}`,
    topicName: "rust",
    tipcardType: "repeatable_tip",
    status: "active",
    createdAt: "2026-01-01T00:00:00Z",
    pinned,
  });
}

function activeRepeatableCard(id: bigint): FlowCardInfo {
  return create(FlowCardInfoSchema, {
    id,
    title: `card-${id}`,
    topicName: "rust",
    tipcardType: "repeatable_tip",
    status: "active",
  });
}

function idleSlots(...cards: FlowCardInfo[]): ReviewSlot[] {
  return cards.map((flowCard): ReviewSlot => ({
    kind: "idle",
    card: flowCard,
  }));
}

const SUCCESS: ReviewSuccessFields = {
  nextCard: undefined,
  dailyComplete: false,
  pendingCount: 0,
  refillScheduled: false,
};

const ATTEMPT: ReviewAttempt = {
  grade: 3,
  action: ReviewActionValue.ACKNOWLEDGE,
  idempotencyKey: "key-a",
};

describe("startReview", () => {
  test("marks exactly the requested slot reviewing with key and generation", () => {
    const slots = startReview(idleSlots(card(1n), card(2n)), 2n, ATTEMPT);
    expect(slots[0]).toEqual({ kind: "idle", card: card(1n) });
    expect(slots[1]).toMatchObject({
      kind: "reviewing",
      generation: 1,
      attempt: ATTEMPT,
    });
    // Card identity is preserved as the generated bigint.
    if (slots[1].kind === "reviewing") {
      expect(slots[1].card.id).toBe(2n);
    }
  });

  test("cannot start a review on a non-idle slot", () => {
    const started = startReview(idleSlots(card(1n)), 1n, ATTEMPT);
    const again = startReview(started, 1n, {
      ...ATTEMPT,
      idempotencyKey: "key-b",
    });
    expect(again).toBe(started);
  });

  test("bumps the generation when re-reviewing an errored slot", () => {
    const started = startReview(idleSlots(card(1n)), 1n, ATTEMPT);
    const failed = reviewFailure(started, 1n, 1, {
      mutationOutcomeIndeterminate: true,
      message: "lost",
    });
    const retried = startReview(failed, 1n, ATTEMPT);
    if (retried[0].kind === "reviewing") {
      expect(retried[0].generation).toBe(2);
      expect(retried[0].attempt).toEqual(ATTEMPT);
    } else {
      throw new Error("expected reviewing slot");
    }
  });
});

describe("reviewSuccess", () => {
  test("nextCard replaces the reviewed card at the same index", () => {
    const original = card(2n, true);
    const slots = startReview(idleSlots(original), 2n, ATTEMPT);
    const next = card(9n, false);
    const result = reviewSuccess(
      slots,
      2n,
      1,
      { ...SUCCESS, nextCard: next, pendingCount: 5 },
    );
    const slot = result[0];
    if (slot.kind !== "idle") throw new Error("expected idle replacement");
    expect(slot.card.id).toBe(9n);
    // Pinned state transfers from the reviewed card; pendingCount comes from
    // the response.
    expect(slot.card.pinned).toBe(true);
    expect(slot.card.pendingCount).toBe(5n);
  });

  test("no nextCard + dailyComplete -> completed placeholder in the same slot", () => {
    const slots = startReview(idleSlots(card(1n)), 1n, ATTEMPT);
    const result = reviewSuccess(slots, 1n, 1, {
      ...SUCCESS,
      dailyComplete: true,
      refillScheduled: true,
    });
    expect(result[0]).toEqual({
      kind: "completed",
      reviewedCardId: 1n,
      topicName: "rust",
      title: "card-1",
      createdAt: "2026-01-01T00:00:00Z",
      tipcardType: "repeatable_tip",
      pinned: false,
    });
  });

  test("no nextCard, not complete, refill scheduled -> awaitingRefill", () => {
    const slots = startReview(idleSlots(card(1n)), 1n, ATTEMPT);
    const result = reviewSuccess(slots, 1n, 1, {
      ...SUCCESS,
      refillScheduled: true,
    });
    expect(result[0]).toMatchObject({
      kind: "awaitingRefill",
      reviewedCardId: 1n,
      topicName: "rust",
      title: "card-1",
      createdAt: "2026-01-01T00:00:00Z",
      pinned: false,
    });
  });

  test("no nextCard, not complete, no refill -> completed", () => {
    const slots = startReview(idleSlots(card(1n)), 1n, ATTEMPT);
    const result = reviewSuccess(slots, 1n, 1, SUCCESS);
    expect(result[0]).toMatchObject({
      kind: "completed",
      reviewedCardId: 1n,
      topicName: "rust",
      title: "card-1",
      createdAt: "2026-01-01T00:00:00Z",
      pinned: false,
    });
  });

  test("stale success generation is ignored (same array reference)", () => {
    const slots = startReview(idleSlots(card(1n)), 1n, ATTEMPT);
    const stale = reviewSuccess(
      slots,
      1n,
      99,
      { ...SUCCESS, nextCard: card(9n) },
    );
    expect(stale).toBe(slots);
  });

  test("success leaves other slots untouched by reference", () => {
    const untouched = card(1n);
    const reviewed = card(2n);
    const slots = startReview(idleSlots(untouched, reviewed), 2n, ATTEMPT);
    const result = reviewSuccess(
      slots,
      2n,
      1,
      { ...SUCCESS, nextCard: card(9n) },
    );
    // Slot 0 keeps its exact object identity and shape.
    expect(result[0]).toBe(slots[0]);
    if (
      slots[0].kind === "idle" &&
      result[0].kind === "idle"
    ) {
      expect(result[0].card).toBe(untouched);
    }
    // The reviewed card object itself is not mutated either.
    expect(reviewed.pinned).toBe(false);
    expect(reviewed.pendingCount).toBe(0n);
    expect(result).toHaveLength(2);
  });
});

describe("reviewFailure", () => {
  test("failure affects only that slot and records indeterminacy", () => {
    const slots = startReview(idleSlots(card(1n), card(2n)), 2n, ATTEMPT);
    const failed = reviewFailure(slots, 2n, 1, {
      mutationOutcomeIndeterminate: true,
      message: "connection reset",
    });
    expect(failed[0]).toBe(slots[0]);
    const slot = failed[1];
    if (slot.kind !== "error") throw new Error("expected error slot");
    expect(slot.card.id).toBe(2n);
    expect(slot.attempt).toEqual(ATTEMPT);
    expect(slot.mutationOutcomeIndeterminate).toBe(true);
    expect(slot.message).toBe("connection reset");
  });

  test("stale failure generation is ignored (same array reference)", () => {
    const slots = startReview(idleSlots(card(1n)), 1n, ATTEMPT);
    expect(
      reviewFailure(slots, 1n, 42, {
        mutationOutcomeIndeterminate: false,
        message: "boom",
      }),
    ).toBe(slots);
  });
});

describe("retryDecision", () => {
  function errored(indeterminate: boolean): ReviewSlot {
    const slots = startReview(idleSlots(card(1n)), 1n, {
      ...ATTEMPT,
      idempotencyKey: "key-orig",
    });
    return reviewFailure(slots, 1n, 1, {
      mutationOutcomeIndeterminate: indeterminate,
      message: "x",
    })[0];
  }

  test("indeterminate error reuses the exact same key", () => {
    const decision = retryDecision(errored(true));
    expect(decision).toEqual({
      kind: "reuseAttempt",
      attempt: {
        grade: 3,
        action: ReviewActionValue.ACKNOWLEDGE,
        idempotencyKey: "key-orig",
      },
      generation: 1,
    });
  });

  test("determinate error requires a fresh caller-provided key", () => {
    const decision = retryDecision(errored(false));
    expect(decision).toEqual({
      kind: "needsNewKey",
      grade: 3,
      action: ReviewActionValue.ACKNOWLEDGE,
      generation: 1,
    });
  });

  test("non-error slots are unrepresentable as retries", () => {
    expect(() =>
      retryDecision({
        kind: "completed",
        reviewedCardId: 1n,
        topicName: "topic",
        title: "topic title",
        createdAt: "2026-01-01T00:00:00Z",
        tipcardType: "repeatable_tip",
        pinned: false,
      }),
    ).toThrow(TypeError);
  });
});

const CONTINUE_ATTEMPT: ContinueAttempt = {
  topicName: "rust",
  idempotencyKey: "ck-1",
};

function continuing(indeterminate: boolean): ReviewSlot[] {
  const slots = [completedSlotBase, completedSlot(2n, true)];
  const started = startContinue(slots, 2n, CONTINUE_ATTEMPT);
  return continueFailure(started, 2n, 1, {
    mutationOutcomeIndeterminate: indeterminate,
    message: "lost",
  });
}

/** A completed slot: the only normal start state for Continue. */
const completedSlotBase: ReviewSlot = {
  kind: "completed",
  reviewedCardId: 1n,
  topicName: "rust",
  title: "card-1",
  createdAt: "2026-01-01T00:00:00Z",
  tipcardType: "repeatable_tip",
  pinned: true,
};
function completedSlot(id: bigint, pinned = false): ReviewSlot {
  return {
    kind: "completed",
    reviewedCardId: id,
    topicName: "rust",
    title: `card-${id}`,
    createdAt: "2026-01-01T00:00:00Z",
    tipcardType: "repeatable_tip",
    pinned,
  };
}

function awaitingSlots(id: bigint, pinned = false): ReviewSlot[] {
  return [
    idleSlots(card(0n))[0],
    {
      kind: "awaitingRefill",
      reviewedCardId: id,
      topicName: "rust",
      title: `card-${id}`,
      createdAt: "2026-01-01T00:00:00Z",
      tipcardType: "repeatable_tip",
      pinned,
      refillToken: 1,
      refillAttempts: 0,
    },
  ];
}

describe("refill polling", () => {
  test("review success schedules token from the review generation, attempt 0", () => {
    const slots = startReview(idleSlots(card(1n)), 1n, ATTEMPT);
    const result = reviewSuccess(slots, 1n, 1, { ...SUCCESS, refillScheduled: true });
    expect(result[0]).toMatchObject({ kind: "awaitingRefill", refillToken: 1, refillAttempts: 0 });
  });

  test("refillPollFound replaces the same index with an idle next card and transfers the pin", () => {
    const untouched = card(0n);
    const slots = awaitingSlots(2n, true);
    const result = refillPollFound(slots, 2n, 1, activeRepeatableCard(9n));
    expect(result).toHaveLength(2);
    expect(result[0]).toBe(slots[0]);
    const slot = result[1];
    if (slot.kind !== "idle") throw new Error("expected idle replacement");
    expect(slot.card.id).toBe(9n);
    expect(slot.card.pinned).toBe(true);
  });

  test("stale refill token returns the exact input array", () => {
    const slots = awaitingSlots(2n);
    expect(refillPollFound(slots, 2n, 99, card(9n))).toBe(slots);
    expect(refillPollMiss(slots, 2n, 99, 3)).toBe(slots);
  });

  test("refillPollFound rejects cards outside the exact refill scope", () => {
    const slots = awaitingSlots(2n);
    const wrongId = activeRepeatableCard(2n);
    const wrongTopic = { ...activeRepeatableCard(9n), topicName: "zig" };
    const wrongType = { ...activeRepeatableCard(9n), tipcardType: "casual_tip" };
    const wrongStatus = { ...activeRepeatableCard(9n), status: "pending" };

    expect(refillPollFound(slots, 2n, 1, wrongId)).toBe(slots);
    expect(refillPollFound(slots, 2n, 1, wrongTopic)).toBe(slots);
    expect(refillPollFound(slots, 2n, 1, wrongType)).toBe(slots);
    expect(refillPollFound(slots, 2n, 1, wrongStatus)).toBe(slots);
  });

  test("refillPollMiss increments attempts until maxAttempts then completes that slot", () => {
    let slots = awaitingSlots(2n);
    slots = refillPollMiss(slots, 2n, 1, 2);
    if (slots[1].kind !== "awaitingRefill") throw new Error("expected awaitingRefill");
    expect(slots[1].refillAttempts).toBe(1);
    expect(slots[1].kind).toBe("awaitingRefill");
    slots = refillPollMiss(slots, 2n, 1, 2);
    expect(slots[1]).toMatchObject({ kind: "completed", reviewedCardId: 2n });
    // Bounded: further misses are no-ops.
    const done = refillPollMiss(slots, 2n, 1, 2);
    expect(done[1].kind).toBe("completed");
  });

  test("miss leaves other slots untouched by reference", () => {
    const slots = awaitingSlots(2n);
    const result = refillPollMiss(slots, 2n, 1, 5);
    expect(result[0]).toBe(slots[0]);
    expect(result).toHaveLength(2);
  });
});

describe("continue", () => {
  test("startContinue only starts from a completed slot; other states are no-ops", () => {
    const idle = idleSlots(card(2n));
    expect(startContinue(idle, 2n, CONTINUE_ATTEMPT)).toBe(idle);
    const slots = [idleSlots(card(0n))[0], completedSlot(2n)];
    const started = startContinue(slots, 2n, CONTINUE_ATTEMPT);
    expect(started[0]).toBe(slots[0]);
    expect(started[1]).toMatchObject({
      kind: "continuing",
      generation: 1,
      attempt: CONTINUE_ATTEMPT,
      reviewedCardId: 2n,
    });
  });

  test("startContinue rejects a mismatched topic and non-repeatable slot", () => {
    const completed = [completedSlot(2n)];
    expect(
      startContinue(completed, 2n, {
        topicName: "zig",
        idempotencyKey: "ck-wrong-topic",
      }),
    ).toBe(completed);

    const casual: ReviewSlot[] = [
      { ...completedSlot(2n), tipcardType: "casual_tip" },
    ];
    expect(startContinue(casual, 2n, CONTINUE_ATTEMPT)).toBe(casual);
  });

  test("continueSuccess replaces the same slot with idle, transfers the pin, preserves others", () => {
    const slots = [idleSlots(card(0n))[0], completedSlot(2n, true)];
    const started = startContinue(slots, 2n, CONTINUE_ATTEMPT);
    const result = continueSuccess(started, 2n, 1, activeRepeatableCard(9n), 4);
    expect(result[0]).toBe(slots[0]);
    const slot = result[1];
    if (slot.kind !== "idle") throw new Error("expected idle replacement");
    expect(slot.card.id).toBe(9n);
    expect(slot.card.pinned).toBe(true);
    expect(slot.card.pendingCount).toBe(4n);
  });

  test("continueSuccess rejects cards outside the exact Continue scope", () => {
    const started = startContinue([completedSlot(2n)], 2n, CONTINUE_ATTEMPT);
    const wrongTopic = { ...activeRepeatableCard(9n), topicName: "zig" };
    const wrongType = { ...activeRepeatableCard(9n), tipcardType: "casual_tip" };
    const wrongStatus = { ...activeRepeatableCard(9n), status: "pending" };

    expect(continueSuccess(started, 2n, 1, wrongTopic, 0)).toBe(started);
    expect(continueSuccess(started, 2n, 1, wrongType, 0)).toBe(started);
    expect(continueSuccess(started, 2n, 1, wrongStatus, 0)).toBe(started);
  });

  test("stale continue success or failure is a no-op (same array reference)", () => {
    const started = startContinue([completedSlot(2n)], 2n, CONTINUE_ATTEMPT);
    expect(continueSuccess(started, 2n, 42, card(9n), 0)).toBe(started);
    expect(
      continueFailure(started, 2n, 42, {
        mutationOutcomeIndeterminate: true,
        message: "x",
      }),
    ).toBe(started);
    // Unknown reviewed ID too.
    expect(continueSuccess(started, 7n, 1, card(9n), 0)).toBe(started);
  });

  test("continueFailure affects only the continuing slot", () => {
    const failed = continuing(true);
    expect(failed[0]).toBe(completedSlotBase);
    expect(failed[1]).toMatchObject({
      kind: "continueError",
      generation: 1,
      attempt: CONTINUE_ATTEMPT,
      mutationOutcomeIndeterminate: true,
      message: "lost",
      reviewedCardId: 2n,
    });
  });

  test("indeterminate retry reuses the exact prior ContinueAttempt", () => {
    const decision = continueRetryDecision(continuing(true)[1]);
    expect(decision).toEqual({
      kind: "reuseAttempt",
      attempt: CONTINUE_ATTEMPT,
      generation: 1,
    });
  });

  test("determinate retry requires a new key but retains the semantic topic", () => {
    const decision = continueRetryDecision(continuing(false)[1]);
    expect(decision).toEqual({ kind: "needsNewKey", topicName: "rust", generation: 1 });
  });

  test("generations increment across retries", () => {
    const errored = continuing(false);
    const retried = startContinue(errored, 2n, {
      topicName: "rust",
      idempotencyKey: "ck-2",
    });
    expect(retried[1].kind === "continuing" && retried[1].attempt.idempotencyKey === "ck-2").toBe(true);
    expect(retried[0]).toMatchObject({ kind: "completed", reviewedCardId: 1n });
    expect(retried[1]).toMatchObject({ kind: "continuing", generation: 2 });
    const failedAgain = continueFailure(retried, 2n, 2, {
      mutationOutcomeIndeterminate: true,
      message: "again",
    });
    const retriedTwice = startContinue(failedAgain, 2n, {
      topicName: "rust",
      idempotencyKey: "ck-3",
    });
    expect(retriedTwice[1]).toMatchObject({ kind: "continuing", generation: 3 });
  });
});

describe("slotIdentity", () => {
  test("total over the union: live cards key on card id, placeholders on reviewed id", () => {
    expect(slotIdentity({ kind: "idle", card: card(1n) })).toBe("1");
    const reviewing = startReview(idleSlots(card(1n)), 1n, ATTEMPT)[0];
    expect(slotIdentity(reviewing)).toBe("1");
    expect(slotIdentity(completedSlot(5n))).toBe("5");
    expect(slotIdentity(awaitingSlots(5n)[1])).toBe("5");
    expect(slotIdentity(continuing(true)[1])).toBe("2");
  });
});

describe("flowSlotKey", () => {
  test("keeps one topic key across repeatable replacement, Continue, and refill", () => {
    const original = card(1n);
    const replacement = card(33n);
    expect(flowSlotKey({ kind: "idle", card: original })).toBe(
      "repeatable:rust",
    );
    const reviewing = startReview(idleSlots(original), 1n, ATTEMPT)[0];
    expect(flowSlotKey(reviewing)).toBe("repeatable:rust");
    const next = reviewSuccess(
      startReview(idleSlots(original), 1n, ATTEMPT),
      1n,
      1,
      {
        ...SUCCESS,
        nextCard: replacement,
        pendingCount: 4,
      },
    );
    expect(flowSlotKey(next[0])).toBe("repeatable:rust");
    expect(flowSlotKey(completedSlot(5n))).toBe("repeatable:rust");
    expect(flowSlotKey(awaitingSlots(5n)[1])).toBe("repeatable:rust");
    expect(flowSlotKey(continuing(true)[1])).toBe("repeatable:rust");
  });

  test("keys other types on the generated card id", () => {
    const casual = create(FlowCardInfoSchema, {
      id: 9n,
      title: "casual",
      topicName: "rust",
      tipcardType: "casual_tip",
      status: "active",
    });
    expect(flowSlotKey({ kind: "idle", card: casual })).toBe("card:9");
    expect(
      flowSlotKey({
        kind: "completed",
        reviewedCardId: 9n,
        topicName: "rust",
        title: "casual",
        createdAt: "2026-01-01T00:00:00Z",
        tipcardType: "casual_tip",
        pinned: false,
      }),
    ).toBe("card:9");
  });
});
