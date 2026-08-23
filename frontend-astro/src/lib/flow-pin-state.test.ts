import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { FlowCardInfoSchema } from "../generated/denpie_pb";
import { slotsFromCards, type ReviewSlot } from "./flow-review-actions";
import {
  applyPinFailure,
  applyPinSuccess,
  EMPTY_PIN_STATE,
  pinCardState,
  pinRetryDecision,
  startPin,
  type PinAttempt,
  type PinCardState,
  type PinState,
} from "./flow-pin-state";

function card(id: bigint, pinned = false) {
  return create(FlowCardInfoSchema, {
    id,
    title: `card-${id}`,
    topicName: "t",
    fullContent: "body",
    tipcardType: "repeatable_tip",
    status: "active",
    pinned,
  });
}

function slots(...cards: ReturnType<typeof card>[]): ReviewSlot[] {
  return slotsFromCards(cards);
}

function attempt(
  overrides: Partial<PinAttempt> = {},
): PinAttempt {
  return {
    cardId: 7n,
    targetPinned: true,
    idempotencyKey: "key-1",
    generation: 1,
    ...overrides,
  };
}

function requireStarted(
  base: ReviewSlot[],
  state: PinState,
  pinAttempt: PinAttempt,
): Map<string, PinCardState> {
  const started = startPin(base, state, pinAttempt);
  if (started === undefined) throw new TypeError("expected pin to start");
  return started;
}

describe("startPin", () => {
  test("starts only the exact live idle card and leaves others untouched", () => {
    const base = slots(card(7n), card(9n));
    const next = startPin(base, EMPTY_PIN_STATE, attempt());
    if (next === undefined) throw new TypeError("expected pin to start");
    expect(pinCardState(next, 7n)).toEqual({
      kind: "saving",
      attempt: attempt(),
    });
    expect(pinCardState(next, 9n).kind).toBe("idle");
    // Other entries are absent, not idle objects.
    expect(next.size).toBe(1);
  });

  test("double-start guard: a second start while saving returns undefined", () => {
    const base = slots(card(7n));
    const first = requireStarted(base, EMPTY_PIN_STATE, attempt());
    const second = startPin(base, first, attempt({ idempotencyKey: "key-2" }));
    expect(second).toBeUndefined();
  });

  test("pin is not actionable without a live idle slot for that ID", () => {
    expect(startPin([], EMPTY_PIN_STATE, attempt())).toBeUndefined();
    // A consumed placeholder occupies the slot; no live card remains.
    const consumed: ReviewSlot[] = [
      {
        kind: "completed",
        reviewedCardId: 7n,
        topicName: "t",
        tipcardType: "repeatable_tip",
        pinned: true,
      },
    ];
    expect(startPin(consumed, EMPTY_PIN_STATE, attempt())).toBeUndefined();
  });
});

describe("applyPinSuccess", () => {
  test("commits to exactly the still-live card and clears its pin state", () => {
    const base = slots(card(7n), card(9n));
    const started = requireStarted(base, EMPTY_PIN_STATE, attempt());
    const committed = applyPinSuccess(base, started, attempt());
    expect(committed.slots[0].kind).toBe("idle");
    if (committed.slots[0].kind !== "idle") throw new TypeError("unreachable");
    expect(committed.slots[0].card.pinned).toBe(true);
    expect(committed.state.size).toBe(0);
  });

  test("preserves every other slot and object reference", () => {
    const base = slots(card(7n), card(9n));
    const started = requireStarted(base, EMPTY_PIN_STATE, attempt());
    const committed = applyPinSuccess(base, started, attempt());
    expect(committed.slots[1]).toBe(base[1]);
    expect(committed.slots[0]).not.toBe(base[0]);
  });

  test("stale generation / unknown attempt is a full no-op", () => {
    const base = slots(card(7n));
    const result = applyPinSuccess(
      base,
      EMPTY_PIN_STATE,
      attempt(),
    );
    expect(result.slots).toBe(base);
    expect(result.slots[0]).toBe(base[0]);
    expect(result.state).toBe(EMPTY_PIN_STATE);
    if (result.slots[0].kind !== "idle") throw new TypeError("unreachable");
    expect(result.slots[0].card.pinned).toBe(false);
  });

  test("a replaced/consumed card cannot commit", () => {
    const base = slots(card(7n));
    const started = requireStarted(base, EMPTY_PIN_STATE, attempt());
    // The card is consumed between start and success: the live slot is gone.
    const consumed: ReviewSlot[] = [
      {
        kind: "completed",
        reviewedCardId: 7n,
        topicName: "t",
        tipcardType: "repeatable_tip",
        pinned: false,
      },
    ];
    const committed = applyPinSuccess(consumed, started, attempt());
    expect(committed.slots).toBe(consumed);
    expect(committed.slots[0].kind).toBe("completed");
    expect(committed.state.size).toBe(0);
  });
});

describe("applyPinFailure", () => {
  test("records a persistent per-card error without touching slots", () => {
    const base = slots(card(7n), card(9n));
    const started = requireStarted(base, EMPTY_PIN_STATE, attempt());
    const failed = applyPinFailure(base, started, attempt(), {
      mutationOutcomeIndeterminate: true,
      message: "network lost",
    });
    expect(failed.slots).toBe(base);
    expect(failed.slots[0]).toBe(base[0]);
    expect(failed.state.get("7")).toMatchObject({
      kind: "error",
      mutationOutcomeIndeterminate: true,
      message: "network lost",
      attempt: attempt(),
    });
  });

  test("stale failure does not overwrite a newer attempt", () => {
    const base = slots(card(7n));
    const newer = attempt({ generation: 2, idempotencyKey: "key-2" });
    const started = requireStarted(base, EMPTY_PIN_STATE, newer);
    const staleFailed = applyPinFailure(base, started, attempt(), {
      mutationOutcomeIndeterminate: false,
      message: "stale",
    });
    expect(staleFailed.state.get("7")).toEqual({
      kind: "saving",
      attempt: newer,
    });
  });
});

describe("pinRetryDecision", () => {
  function errorState(
    indeterminate: boolean,
  ): Extract<PinCardState, { kind: "error" }> {
    return {
      kind: "error",
      attempt: attempt({ targetPinned: false }),
      mutationOutcomeIndeterminate: indeterminate,
      message: "boom",
    };
  }

  test("indeterminate outcome reuses the exact attempt including key and generation", () => {
    const decision = pinRetryDecision(errorState(true));
    expect(decision.kind).toBe("reuseAttempt");
    if (decision.kind !== "reuseAttempt") throw new TypeError("unreachable");
    expect(decision.attempt).toEqual(attempt({ targetPinned: false }));
  });

  test("determinate outcome keeps cardId/targetPinned but requires a fresh key and generation", () => {
    const decision = pinRetryDecision(errorState(false));
    expect(decision.kind).toBe("needsNewKey");
    if (decision.kind !== "needsNewKey") throw new TypeError("unreachable");
    expect(decision.cardId).toBe(7n);
    expect(decision.targetPinned).toBe(false);
    expect(decision.generation).toBe(2);
  });

  test("a determinate retry can start again with a fresh key after an error", () => {
    const base = slots(card(7n));
    const started = requireStarted(base, EMPTY_PIN_STATE, attempt());
    const errored = applyPinFailure(base, started, attempt(), {
      mutationOutcomeIndeterminate: false,
      message: "rejected",
    });
    const state = errored.state.get("7");
    if (state?.kind !== "error") throw new TypeError("expected pin error");
    const decision = pinRetryDecision(state);
    if (decision.kind !== "needsNewKey") throw new TypeError("unreachable");
    const retryAttempt = attempt({
      cardId: decision.cardId,
      targetPinned: decision.targetPinned,
      idempotencyKey: "fresh-key",
      generation: decision.generation,
    });
    const restarted = startPin(base, errored.state, retryAttempt);
    if (restarted === undefined) throw new TypeError("expected retry to start");
    expect(pinCardState(restarted, 7n)).toEqual({
      kind: "saving",
      attempt: retryAttempt,
    });
  });
});
