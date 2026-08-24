import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { FlowCardInfoSchema } from "../generated/denpie_pb";
import { slotsFromCards, type ReviewSlot } from "./flow-review-actions";
import {
  applyDeleteFailure,
  applyDeleteSuccess,
  deleteCardState,
  deleteRetryDecision,
  EMPTY_DELETE_STATE,
  startDelete,
  type DeleteAttempt,
  type DeleteCardState,
  type DeleteState,
} from "./flow-delete-state";

function card(id: bigint, pinned = false) {
  return create(FlowCardInfoSchema, {
    id,
    title: `card-${id}`,
    // Keep delete-state fixtures in separate repeatable stacks. Topic-level
    // deduplication belongs to flow projection and is not under test here.
    topicName: `t-${id}`,
    fullContent: "body",
    tipcardType: "repeatable_tip",
    status: "active",
    pinned,
  });
}

function slots(...cards: ReturnType<typeof card>[]): ReviewSlot[] {
  return slotsFromCards(cards);
}

function attempt(overrides: Partial<DeleteAttempt> = {}): DeleteAttempt {
  return {
    cardId: 7n,
    idempotencyKey: "key-1",
    generation: 1,
    ...overrides,
  };
}

function requireStarted(
  base: ReviewSlot[],
  state: DeleteState,
  deleteAttempt: DeleteAttempt,
): Map<string, DeleteCardState> {
  const started = startDelete(base, state, deleteAttempt);
  if (started === undefined) throw new TypeError("expected delete to start");
  return started;
}

describe("startDelete", () => {
  test("starts only the exact live idle card and leaves other entries untouched", () => {
    const base = slots(card(7n), card(9n));
    const next = startDelete(base, EMPTY_DELETE_STATE, attempt());
    if (next === undefined) throw new TypeError("expected delete to start");
    expect(deleteCardState(next, 7n)).toEqual({
      kind: "deleting",
      attempt: attempt(),
    });
    expect(deleteCardState(next, 9n)).toEqual({ kind: "idle" });
  });

  test("rejects a non-idle or unknown slot", () => {
    const live = slots(card(7n));
    const reviewing = live.map((slot): ReviewSlot => {
      if (slot.kind !== "idle") return slot;
      return {
        kind: "reviewing",
        card: slot.card,
        generation: 1,
        attempt: { grade: 4, action: null, idempotencyKey: "k" },
      };
    });
    expect(startDelete(reviewing, EMPTY_DELETE_STATE, attempt())).toBeUndefined();
    expect(
      startDelete(slots(card(99n)), EMPTY_DELETE_STATE, attempt()),
    ).toBeUndefined();
  });

  test("a second click cannot launch another request", () => {
    const base = slots(card(7n));
    const first = requireStarted(base, EMPTY_DELETE_STATE, attempt());
    expect(startDelete(base, first, attempt({ idempotencyKey: "key-2" }))).toBeUndefined();
    expect(startDelete(base, first, attempt())).toBeUndefined();
  });
});

describe("deleteRetryDecision", () => {
  test("indeterminate failure reuses the exact attempt and key", () => {
    const decision = deleteRetryDecision({
      kind: "error",
      attempt: attempt({ idempotencyKey: "same-key" }),
      mutationOutcomeIndeterminate: true,
      message: "lost",
    });
    expect(decision).toEqual({
      kind: "reuseAttempt",
      attempt: attempt({ idempotencyKey: "same-key" }),
    });
  });

  test("determinate failure allocates a new key while preserving the card ID", () => {
    const decision = deleteRetryDecision({
      kind: "error",
      attempt: attempt({ cardId: 7n, idempotencyKey: "old", generation: 3 }),
      mutationOutcomeIndeterminate: false,
      message: "500",
    });
    expect(decision).toEqual({
      kind: "needsNewKey",
      cardId: 7n,
      generation: 4,
    });
  });
});

describe("applyDeleteSuccess", () => {
  test("removes exactly the matching slot and preserves every other reference", () => {
    const base = slots(card(7n), card(9n), card(11n));
    const state = requireStarted(base, EMPTY_DELETE_STATE, attempt());
    const result = applyDeleteSuccess(base, state, attempt(), [7n]);
    expect(result.slots.map((s) => s.kind === "idle" && s.card.id)).toEqual([
      9n,
      11n,
    ]);
    expect(result.slots[0]).toBe(base[1]);
    expect(result.slots[1]).toBe(base[2]);
    expect(result.state.has("7")).toBe(false);
  });

  test("drops the deleted ID from the saved pinned order without disturbing the rest", () => {
    const base = slots(card(7n), card(9n));
    const state = requireStarted(base, EMPTY_DELETE_STATE, attempt());
    const result = applyDeleteSuccess(base, state, attempt(), [9n, 7n, 3n]);
    expect(result.pinnedOrder).toEqual([9n, 3n]);
  });

  test("stale success after replacement removes no slot", () => {
    const replaced = slots(card(13n), card(9n));
    const state = requireStarted(
      slots(card(7n)),
      EMPTY_DELETE_STATE,
      attempt({ generation: 1 }),
    );
    // The newer retry owns generation 2; generation 1 is stale.
    const result = applyDeleteSuccess(
      replaced,
      requireStarted(
        replaced,
        EMPTY_DELETE_STATE,
        attempt({ cardId: 13n, generation: 2 }),
      ),
      attempt({ generation: 1 }),
      [],
    );
    expect(result.slots).toBe(replaced);
    // Unknown attempt commits nothing either.
    const unknown = applyDeleteSuccess(
      replaced,
      EMPTY_DELETE_STATE,
      attempt(),
      [],
    );
    expect(unknown.slots).toBe(replaced);
  });

  test("last-card deletion yields an empty slot list input for the empty transition", () => {
    const base = slots(card(7n));
    const state = requireStarted(base, EMPTY_DELETE_STATE, attempt());
    const result = applyDeleteSuccess(base, state, attempt(), []);
    expect(result.slots).toHaveLength(0);
  });

  test("failure keeps the card and records the persistent error", () => {
    const base = slots(card(7n), card(9n));
    const state = requireStarted(base, EMPTY_DELETE_STATE, attempt());
    const result = applyDeleteFailure(
      base,
      state,
      attempt(),
      [],
      {
        mutationOutcomeIndeterminate: true,
        message: "connection lost",
      },
    );
    expect(result.slots).toHaveLength(2);
    expect(deleteCardState(result.state, 7n)).toEqual({
      kind: "error",
      attempt: attempt(),
      mutationOutcomeIndeterminate: true,
      message: "connection lost",
    });
    expect(deleteCardState(result.state, 9n)).toEqual({ kind: "idle" });
  });

  test("stale failure commits nothing", () => {
    const base = slots(card(7n));
    const result = applyDeleteFailure(
      base,
      EMPTY_DELETE_STATE,
      attempt(),
      [],
      { mutationOutcomeIndeterminate: false, message: "x" },
    );
    expect(result.slots).toBe(base);
    expect(result.state).toBe(EMPTY_DELETE_STATE);
    expect(result.pinnedOrder).toEqual([]);
  });
});
