// Pure Flow pin transition model, independent of any component or fetch.
// Per-card pin mutation state is a discriminated union keyed by the exact
// bigint card identity; the caller owns when transitions run and provides
// the live slot list so a commit can only land on an exact still-live card.

import type { ReviewSlot } from "./flow-review-state";

/**
 * The exact caller-owned pin mutation payload that a safe retry must
 * preserve: the target card, the intended pinned value, the idempotency
 * key, and the request generation that makes stale results committable
 * only while they are still current.
 */
export interface PinAttempt {
  cardId: bigint;
  targetPinned: boolean;
  idempotencyKey: string;
  generation: number;
}

/** One card's pin mutation lifecycle. Illegal combinations are unrepresentable. */
export type PinCardState =
  | { kind: "idle" }
  | { kind: "saving"; attempt: PinAttempt }
  | {
      kind: "error";
      attempt: PinAttempt;
      /**
       * True when the request may have reached the server: a retry MUST keep
       * the exact same key and payload.
       */
      mutationOutcomeIndeterminate: boolean;
      message: string;
    };

/** Per-card pin state keyed by `cardId.toString()`; absent means idle. */
export type PinState = ReadonlyMap<string, PinCardState>;

export const EMPTY_PIN_STATE: PinState = new Map();

/** Stable idle singleton: reference-equality memoization can rely on it. */
const IDLE_PIN_CARD: PinCardState = { kind: "idle" };

function pinKey(cardId: bigint): string {
  return cardId.toString();
}

/** The pin mutation state for one card; cards never touched default to idle. */
export function pinCardState(state: PinState, cardId: bigint): PinCardState {
  return state.get(pinKey(cardId)) ?? IDLE_PIN_CARD;
}

function findLiveIdleSlotIndex(
  slots: readonly ReviewSlot[],
  cardId: bigint,
): number {
  return slots.findIndex(
    (slot) => slot.kind === "idle" && slot.card.id === cardId,
  );
}

function sameAttempt(a: PinAttempt, b: PinAttempt): boolean {
  return (
    a.cardId === b.cardId &&
    a.targetPinned === b.targetPinned &&
    a.idempotencyKey === b.idempotencyKey &&
    a.generation === b.generation
  );
}

/**
 * Begin (or retry) the pin mutation for one card with an exact caller-owned
 * attempt. Returns `undefined` — changing nothing — when:
 * - no live idle slot carries that exact card ID (pin is only actionable
 *   for a live idle slot), or
 * - that card already has a `saving` pin mutation (a double click cannot
 *   launch twice).
 * Only that card's entry changes; every other entry keeps its reference.
 */
export function startPin(
  slots: readonly ReviewSlot[],
  state: PinState,
  attempt: PinAttempt,
): Map<string, PinCardState> | undefined {
  if (findLiveIdleSlotIndex(slots, attempt.cardId) === -1) return undefined;
  const key = pinKey(attempt.cardId);
  const current = state.get(key);
  if (current?.kind === "saving") return undefined;
  const next = new Map(state);
  next.set(key, { kind: "saving", attempt });
  return next;
}

/**
 * Retry decision for an errored pin mutation: an outcome-indeterminate
 * failure reuses the exact same attempt including its generation and key
 * (a lost-after-commit response must never execute twice); a determinate
 * failure requires a fresh caller-provided key but preserves the exact
 * card ID and target pinned value, bumping the generation.
 */
export type PinRetryDecision =
  | { kind: "reuseAttempt"; attempt: PinAttempt }
  | {
      kind: "needsNewKey";
      cardId: bigint;
      targetPinned: boolean;
      generation: number;
    };

export function pinRetryDecision(errorState: Extract<PinCardState, { kind: "error" }>): PinRetryDecision {
  return errorState.mutationOutcomeIndeterminate
    ? { kind: "reuseAttempt", attempt: errorState.attempt }
    : {
        kind: "needsNewKey",
        cardId: errorState.attempt.cardId,
        targetPinned: errorState.attempt.targetPinned,
        generation: errorState.attempt.generation + 1,
      };
}

export interface PinCommitResult {
  slots: ReviewSlot[];
  state: PinState;
}

function unchanged(slots: ReviewSlot[], state: PinState): PinCommitResult {
  return { slots, state };
}

/**
 * Apply a successful pin to `attempt.cardId`. The result commits only when
 * the current state for that card is exactly this attempt's `saving` entry
 * (stale generations and unknown attempts are ignored). On commit the pin
 * state entry is cleared and — only if a live idle slot still carries that
 * exact card ID — that single slot's card gets a new object with the updated
 * `pinned` value. A replaced/consumed card cannot commit: slots stay
 * untouched. Every other slot and map entry keeps its reference.
 */
export function applyPinSuccess(
  slots: ReviewSlot[],
  state: PinState,
  attempt: PinAttempt,
): PinCommitResult {
  const key = pinKey(attempt.cardId);
  const current = state.get(key);
  if (
    current?.kind !== "saving" ||
    !sameAttempt(current.attempt, attempt)
  ) {
    return unchanged(slots, state);
  }
  const index = findLiveIdleSlotIndex(slots, attempt.cardId);
  const nextSlots =
    index === -1
      ? slots
      : slots.map((slot, position) =>
          position === index && slot.kind === "idle"
            ? { ...slot, card: { ...slot.card, pinned: attempt.targetPinned } }
            : slot,
        );
  const nextState = new Map(state);
  nextState.delete(key);
  return { slots: nextSlots, state: nextState };
}

/** Fields of a failed pin this model consumes. */
export interface PinFailureFields {
  mutationOutcomeIndeterminate: boolean;
  message: string;
}

/**
 * Apply a failed pin to `attempt.cardId`. Only that card's entry changes,
 * and only when it is exactly this attempt's `saving` entry; a stale
 * generation or unknown attempt leaves everything untouched. Slots are
 * never modified by a failure.
 */
export function applyPinFailure(
  slots: ReviewSlot[],
  state: PinState,
  attempt: PinAttempt,
  failure: PinFailureFields,
): PinCommitResult {
  const key = pinKey(attempt.cardId);
  const current = state.get(key);
  if (
    current?.kind !== "saving" ||
    !sameAttempt(current.attempt, attempt)
  ) {
    return unchanged(slots, state);
  }
  const nextState = new Map(state);
  nextState.set(key, {
    kind: "error",
    attempt,
    mutationOutcomeIndeterminate: failure.mutationOutcomeIndeterminate,
    message: failure.message,
  });
  return { slots, state: nextState };
}
