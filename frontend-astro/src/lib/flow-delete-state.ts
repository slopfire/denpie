// Pure Flow card-delete transition model, independent of any component or
// fetch. Per-card delete mutation state is a discriminated union keyed by
// the exact bigint card identity; the caller owns when transitions run and
// provides the live slot list so a commit can only land on an exact
// still-live card.

import { setPinnedMembership } from "./flow-pinned-order";
import type { ReviewSlot } from "./flow-review-state";

/**
 * The exact caller-owned delete mutation payload that a safe retry must
 * preserve: the target card and the idempotency key. The generation makes
 * stale results committable only while they are still current.
 */
export interface DeleteAttempt {
  cardId: bigint;
  idempotencyKey: string;
  generation: number;
}

/** One card's delete mutation lifecycle. Illegal combinations are unrepresentable. */
export type DeleteCardState =
  | { kind: "idle" }
  | { kind: "deleting"; attempt: DeleteAttempt }
  | {
      kind: "error";
      attempt: DeleteAttempt;
      /**
       * True when the request may have reached the server: a retry MUST keep
       * the exact same key (and attempt) verbatim.
       */
      mutationOutcomeIndeterminate: boolean;
      message: string;
    };

/** Per-card delete state keyed by `cardId.toString()`; absent means idle. */
export type DeleteState = ReadonlyMap<string, DeleteCardState>;

export const EMPTY_DELETE_STATE: DeleteState = new Map();

/** Stable idle singleton: reference-equality memoization can rely on it. */
const IDLE_DELETE_CARD: DeleteCardState = { kind: "idle" };

function deleteKey(cardId: bigint): string {
  return cardId.toString();
}

/** The delete state for one card; cards never touched default to idle. */
export function deleteCardState(
  state: DeleteState,
  cardId: bigint,
): DeleteCardState {
  return state.get(deleteKey(cardId)) ?? IDLE_DELETE_CARD;
}

function findLiveIdleSlotIndex(
  slots: ReviewSlot[],
  cardId: bigint,
): number {
  return slots.findIndex(
    (slot) => slot.kind === "idle" && slot.card.id === cardId,
  );
}

function sameAttempt(a: DeleteAttempt, b: DeleteAttempt): boolean {
  return (
    a.cardId === b.cardId &&
    a.idempotencyKey === b.idempotencyKey &&
    a.generation === b.generation
  );
}

/**
 * Begin (or retry) the delete mutation for one card with an exact
 * caller-owned attempt. Returns `undefined` — changing nothing — when:
 * - no live idle slot carries that exact card ID (only a live idle card can
 *   start delete), or
 * - that card already has a `deleting` mutation (a second click cannot
 *   launch another request).
 * Only that card's entry changes; every other entry keeps its reference.
 */
export function startDelete(
  slots: ReviewSlot[],
  state: DeleteState,
  attempt: DeleteAttempt,
): Map<string, DeleteCardState> | undefined {
  if (findLiveIdleSlotIndex(slots, attempt.cardId) === -1) return undefined;
  const key = deleteKey(attempt.cardId);
  const current = state.get(key);
  if (current?.kind === "deleting") return undefined;
  const next = new Map(state);
  next.set(key, { kind: "deleting", attempt });
  return next;
}

/**
 * Retry decision for an errored delete: an outcome-indeterminate failure
 * reuses the exact same attempt including its key and generation (a
 * lost-after-commit response must never execute twice); a determinate
 * failure requires a fresh caller-provided key but preserves the exact
 * card ID, bumping the generation.
 */
export type DeleteRetryDecision =
  | { kind: "reuseAttempt"; attempt: DeleteAttempt }
  | { kind: "needsNewKey"; cardId: bigint; generation: number };

export function deleteRetryDecision(
  errorState: Extract<DeleteCardState, { kind: "error" }>,
): DeleteRetryDecision {
  return errorState.mutationOutcomeIndeterminate
    ? { kind: "reuseAttempt", attempt: errorState.attempt }
    : {
        kind: "needsNewKey",
        cardId: errorState.attempt.cardId,
        generation: errorState.attempt.generation + 1,
      };
}

export interface DeleteCommitResult {
  /** Slots with exactly the deleted live idle slot removed; others untouched. */
  slots: ReviewSlot[];
  state: DeleteState;
  /**
   * Saved pinned order with the deleted ID dropped; unchanged when the
   * deleted card was not pinned-tracked.
   */
  pinnedOrder: readonly bigint[];
}

/**
 * Apply a successful delete of `attempt.cardId`. The result commits only
 * when the current state for that card is exactly this attempt's `deleting`
 * entry (stale generations and unknown attempts cannot remove anything). On
 * commit the entry is cleared, the single still-live idle slot carrying
 * that exact ID is removed from its section preserving every other slot,
 * and the saved pinned order drops that one ID without disturbing the rest.
 * A stale success after replacement finds no live idle slot and removes
 * nothing from the list.
 */
export function applyDeleteSuccess(
  slots: ReviewSlot[],
  state: DeleteState,
  attempt: DeleteAttempt,
  pinnedOrder: readonly bigint[],
): DeleteCommitResult {
  const key = deleteKey(attempt.cardId);
  const current = state.get(key);
  if (current?.kind !== "deleting" || !sameAttempt(current.attempt, attempt)) {
    return { slots, state, pinnedOrder };
  }
  const index = findLiveIdleSlotIndex(slots, attempt.cardId);
  const nextSlots =
    index === -1 ? slots : slots.filter((_, position) => position !== index);
  const nextState = new Map(state);
  nextState.delete(key);
  const nextOrder = pinnedOrder.includes(attempt.cardId)
    ? setPinnedMembership(pinnedOrder, attempt.cardId, false)
    : pinnedOrder;
  return { slots: nextSlots, state: nextState, pinnedOrder: nextOrder };
}

/** Fields of a failed delete this model consumes. */
export interface DeleteFailureFields {
  mutationOutcomeIndeterminate: boolean;
  message: string;
}

/**
 * Apply a failed delete to `attempt.cardId`. Only that card's entry changes,
 * and only when it is exactly this attempt's `deleting` entry; a stale
 * generation or unknown attempt leaves everything untouched. Slots and the
 * pinned order are never modified by a failure — the card stays visible.
 */
export function applyDeleteFailure(
  slots: ReviewSlot[],
  state: DeleteState,
  attempt: DeleteAttempt,
  pinnedOrder: readonly bigint[],
  failure: DeleteFailureFields,
): DeleteCommitResult {
  const key = deleteKey(attempt.cardId);
  const current = state.get(key);
  if (current?.kind !== "deleting" || !sameAttempt(current.attempt, attempt)) {
    return { slots, state, pinnedOrder };
  }
  const nextState = new Map(state);
  nextState.set(key, {
    kind: "error",
    attempt,
    mutationOutcomeIndeterminate: failure.mutationOutcomeIndeterminate,
    message: failure.message,
  });
  return { slots, state: nextState, pinnedOrder };
}
