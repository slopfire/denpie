// Pure Flow review transition model: explicit discriminated slot states keyed
// by generated bigint card identity. No fetch, no component code — the caller
// owns when transitions run; this module owns what the next state is and
// ignores stale request generations.

import type {
  FlowCardInfo,
  ReviewActionValue,
  ReviewAndAdvanceResponse,
} from "../generated/denpie_pb";

/** The exact caller-owned mutation payload that a safe retry must preserve. */
export interface ReviewAttempt {
  grade: number;
  action: ReviewActionValue;
  idempotencyKey: string;
}

/**
 * One Flow slot's review lifecycle. Illegal combinations are
 * unrepresentable: a `reviewing` slot always carries its request generation
 * and the caller-owned idempotency key; an `error` slot records whether the
 * mutation outcome was indeterminate; placeholders (`completed`,
 * `awaitingRefill`) occupy the slot after the reviewed card was consumed,
 * where an `awaitingRefill` slot additionally tracks its refill poll token
 * and attempt count. A placeholder retains the reviewed identity needed for
 * a stable React key, a later topic-scoped refill, and exact placement in
 * the pinned section and topic/date sort (title, createdAt, pinned); it
 * cannot masquerade as a live card.
 */
export interface ReviewedSlotIdentity {
  reviewedCardId: bigint;
  topicName: string;
  title: string;
  createdAt: string;
  tipcardType: string;
  pinned: boolean;
}

export type ReviewSlot =
  | { kind: "idle"; card: FlowCardInfo }
  | {
      kind: "reviewing";
      card: FlowCardInfo;
      /** Monotonic per-slot request counter; stale generations are ignored. */
      generation: number;
      attempt: ReviewAttempt;
    }
  | {
      kind: "error";
      card: FlowCardInfo;
      generation: number;
      attempt: ReviewAttempt;
      /**
       * True when the request may have reached the server: a retry MUST keep
       * the exact same key and payload.
       */
      mutationOutcomeIndeterminate: boolean;
      message: string;
    }
  /** The reviewed card was consumed and nothing follows it today. */
  | ({ kind: "completed" } & ReviewedSlotIdentity)
  /**
   * The reviewed card was consumed and the server scheduled a refill. The
   * token starts at the review's generation so stale poll results are
   * ignored; `refillAttempts` bounds the miss progression.
   */
  | ({
      kind: "awaitingRefill";
      /** Poll generation this slot waits on; stale tokens are ignored. */
      refillToken: number;
      /** Completed-but-unproductive poll attempts so far. */
      refillAttempts: number;
    } & ReviewedSlotIdentity)
  /**
   * A completed slot is being continued via `continue_daily_review`. This is
   * the only normal start state for Continue; the identity (including the
   * semantic topic) is retained for retries.
   */
  | ({
      kind: "continuing";
      generation: number;
      attempt: ContinueAttempt;
    } & ReviewedSlotIdentity)
  /**
   * A Continue mutation failed. Records whether the outcome was
   * indeterminate (retry MUST reuse the exact same key) plus the retained
   * identity for a determinate retry with a fresh key.
   */
  | ({
      kind: "continueError";
      generation: number;
      attempt: ContinueAttempt;
      mutationOutcomeIndeterminate: boolean;
      message: string;
    } & ReviewedSlotIdentity);

/** Fields of a successful `review_and_advance` this model consumes. */
export type ReviewSuccessFields = Pick<
  ReviewAndAdvanceResponse,
  "nextCard" | "dailyComplete" | "refillScheduled" | "pendingCount"
>;

/** Fields of a failure this model consumes. */
export interface ReviewFailureFields {
  mutationOutcomeIndeterminate: boolean;
  message: string;
}

/**
 * The exact caller-owned Continue payload that a safe retry must preserve:
 * the semantic topic plus the idempotency key. A determinate failure may
 * allocate a fresh key but must retain the topic.
 */
export interface ContinueAttempt {
  topicName: string;
  idempotencyKey: string;
}

function isReviewing(
  slot: ReviewSlot,
  generation: number,
): slot is Extract<ReviewSlot, { kind: "reviewing" }> {
  return slot.kind === "reviewing" && slot.generation === generation;
}

/** Begin reviewing `cardId` with an exact caller-owned mutation attempt. */
export function startReview(
  slots: ReviewSlot[],
  cardId: bigint,
  attempt: ReviewAttempt,
): ReviewSlot[] {
  const slot = slots.find(
    (candidate) =>
      (candidate.kind === "idle" || candidate.kind === "error") &&
      candidate.card.id === cardId,
  );
  if (slot === undefined) return slots;
  const nextGeneration =
    slot.kind === "error" ? slot.generation + 1 : 1;
  return slots.map((current) =>
    (current.kind === "idle" || current.kind === "error") &&
    current.card.id === cardId
      ? {
          kind: "reviewing",
          card: current.card,
          generation: nextGeneration,
          attempt,
        }
      : current,
  );
}

/**
 * Apply a successful review to `reviewedCardId`. A stale generation leaves
 * every slot untouched (same array reference). With a `nextCard`, the
 * replacement takes the reviewed card's pinned state plus the response's
 * `pendingCount`; otherwise the placeholder is chosen from `dailyComplete` /
 * `refillScheduled`.
 */
export function reviewSuccess(
  slots: ReviewSlot[],
  reviewedCardId: bigint,
  generation: number,
  response: ReviewSuccessFields,
): ReviewSlot[] {
  const slot = slots.find(
    (candidate) =>
      candidate.kind === "reviewing" &&
      candidate.card.id === reviewedCardId,
  );
  if (slot === undefined || !isReviewing(slot, generation)) {
    return slots;
  }
  return slots.map((current) => {
    if (
      !isReviewing(current, generation) ||
      current.card.id !== reviewedCardId
    ) {
      return current;
    }
    if (response.nextCard !== undefined) {
      const replacement: FlowCardInfo = { ...response.nextCard };
      replacement.pinned = current.card.pinned;
      replacement.pendingCount = BigInt(response.pendingCount);
      return { kind: "idle", card: replacement };
    }
    const identity: ReviewedSlotIdentity = {
      reviewedCardId,
      topicName: current.card.topicName,
      title: current.card.title,
      createdAt: current.card.createdAt,
      tipcardType: current.card.tipcardType,
      pinned: current.card.pinned,
    };
    return response.dailyComplete || !response.refillScheduled
      ? { kind: "completed", ...identity }
      : {
          kind: "awaitingRefill",
          ...identity,
          refillToken: generation,
          refillAttempts: 0,
        };
  });
}

/**
 * Apply a failed review to `reviewedCardId`. Only that slot changes; a
 * stale generation leaves everything untouched.
 */
export function reviewFailure(
  slots: ReviewSlot[],
  reviewedCardId: bigint,
  generation: number,
  failure: ReviewFailureFields,
): ReviewSlot[] {
  const slot = slots.find(
    (candidate) =>
      candidate.kind === "reviewing" &&
      candidate.card.id === reviewedCardId,
  );
  if (slot === undefined || !isReviewing(slot, generation)) {
    return slots;
  }
  return slots.map((current) =>
    isReviewing(current, generation) && current.card.id === reviewedCardId
      ? {
          kind: "error",
          card: current.card,
          generation: current.generation,
          attempt: current.attempt,
          mutationOutcomeIndeterminate: failure.mutationOutcomeIndeterminate,
          message: failure.message,
        }
      : current,
  );
}

/**
 * Retry decision for an errored slot: an outcome-indeterminate failure reuses
 * the exact same key (a lost-after-commit response must never execute
 * twice); a determinate failure requires a fresh caller-provided key.
 */
export type RetryDecision =
  | { kind: "reuseAttempt"; attempt: ReviewAttempt; generation: number }
  | {
      kind: "needsNewKey";
      grade: number;
      action: ReviewActionValue;
      generation: number;
    };

export function retryDecision(slot: ReviewSlot): RetryDecision {
  if (slot.kind !== "error") {
    throw new TypeError("retryDecision requires an errored review slot");
  }
  return slot.mutationOutcomeIndeterminate
    ? {
        kind: "reuseAttempt",
        attempt: slot.attempt,
        generation: slot.generation,
      }
    : {
        kind: "needsNewKey",
        grade: slot.attempt.grade,
        action: slot.attempt.action,
        generation: slot.generation,
      };
}

/**
 * Total slot identity for React keys / test IDs: every union member maps to
 * a stable string without hardcoding members at the call site. Live cards
 * key on the card ID; consumed placeholders key on the reviewed card ID.
 */
export function slotIdentity(slot: ReviewSlot): string {
  switch (slot.kind) {
    case "idle":
    case "reviewing":
    case "error":
      return slot.card.id.toString();
    case "completed":
    case "awaitingRefill":
    case "continuing":
    case "continueError":
      return slot.reviewedCardId.toString();
  }
}

/**
 * Visual slot key for React list identity. Repeatable cards stay on one
 * topic-scoped slot across replacement, Continue, and refill so fullscreen
 * can keep going. Other types key on the generated card ID.
 */
export function flowSlotKey(slot: ReviewSlot): string {
  if (
    slot.kind === "idle" ||
    slot.kind === "reviewing" ||
    slot.kind === "error"
  ) {
    return slot.card.tipcardType === "repeatable_tip"
      ? `repeatable:${slot.card.topicName}`
      : `card:${slot.card.id}`;
  }
  return slot.tipcardType === "repeatable_tip"
    ? `repeatable:${slot.topicName}`
    : `card:${slot.reviewedCardId}`;
}

function findAwaitingRefill(
  slots: ReviewSlot[],
  reviewedCardId: bigint,
  token: number,
): Extract<ReviewSlot, { kind: "awaitingRefill" }> | undefined {
  return slots.find(
    (
      candidate,
    ): candidate is Extract<ReviewSlot, { kind: "awaitingRefill" }> =>
      candidate.kind === "awaitingRefill" &&
      candidate.reviewedCardId === reviewedCardId &&
      candidate.refillToken === token,
  );
}

/**
 * A refill poll found a next card: replace the awaiting-refill slot at its
 * exact index with an idle card, transferring the reviewed pin. A stale
 * token returns the exact input array; other slots keep their references.
 */
export function refillPollFound(
  slots: ReviewSlot[],
  reviewedCardId: bigint,
  token: number,
  nextCard: FlowCardInfo,
): ReviewSlot[] {
  const awaiting = findAwaitingRefill(slots, reviewedCardId, token);
  if (
    awaiting === undefined ||
    nextCard.id === reviewedCardId ||
    nextCard.topicName !== awaiting.topicName ||
    nextCard.tipcardType !== "repeatable_tip" ||
    nextCard.status !== "active"
  ) {
    return slots;
  }
  return slots.map((current) =>
    current.kind === "awaitingRefill" &&
    current.reviewedCardId === reviewedCardId &&
    current.refillToken === token
      ? {
          kind: "idle",
          card: { ...nextCard, pinned: current.pinned },
        }
      : current,
  );
}

/**
 * A refill poll found nothing: increment the attempt count until
 * `maxAttempts`, then turn that same slot into `completed`. A stale token
 * or unknown ID returns the exact input array.
 */
export function refillPollMiss(
  slots: ReviewSlot[],
  reviewedCardId: bigint,
  token: number,
  maxAttempts: number,
): ReviewSlot[] {
  const slot = findAwaitingRefill(slots, reviewedCardId, token);
  if (
    slot === undefined ||
    slot.kind !== "awaitingRefill" ||
    maxAttempts < 1 ||
    slot.refillAttempts + 1 > maxAttempts
  ) {
    return slots;
  }
  return slots.map((current) =>
    current.kind === "awaitingRefill" &&
    current.reviewedCardId === reviewedCardId &&
    current.refillToken === token
      ? current.refillAttempts + 1 >= maxAttempts
        ? {
            kind: "completed",
            reviewedCardId: current.reviewedCardId,
            topicName: current.topicName,
            title: current.title,
            createdAt: current.createdAt,
            tipcardType: current.tipcardType,
            pinned: current.pinned,
          }
        : { ...current, refillAttempts: current.refillAttempts + 1 }
      : current,
  );
}

/**
 * Begin continuing `reviewedCardId` with the caller-owned attempt. Only a
 * `completed` slot may start a Continue; a `continueError` slot retries via
 * {@link startContinue} too — generations increment across retries. Unknown
 * IDs leave everything untouched.
 */
export function startContinue(
  slots: ReviewSlot[],
  reviewedCardId: bigint,
  attempt: ContinueAttempt,
): ReviewSlot[] {
  const slot = slots.find(
    (
      candidate,
    ): candidate is Extract<
      ReviewSlot,
      { kind: "completed" | "continueError" }
    > =>
      (candidate.kind === "completed" || candidate.kind === "continueError") &&
      candidate.reviewedCardId === reviewedCardId,
  );
  if (
    slot === undefined ||
    slot.tipcardType !== "repeatable_tip" ||
    attempt.topicName !== slot.topicName
  )
    return slots;
  return slots.map((current) =>
    current.kind === "completed" && current.reviewedCardId === reviewedCardId
      ? {
          kind: "continuing",
          generation: 1,
          attempt,
          reviewedCardId: current.reviewedCardId,
          topicName: current.topicName,
          title: current.title,
          createdAt: current.createdAt,
          tipcardType: current.tipcardType,
          pinned: current.pinned,
        }
      : current.kind === "continueError" &&
          current.reviewedCardId === reviewedCardId
        ? {
            kind: "continuing",
            generation: current.generation + 1,
            attempt,
            reviewedCardId: current.reviewedCardId,
            topicName: current.topicName,
            title: current.title,
            createdAt: current.createdAt,
            tipcardType: current.tipcardType,
            pinned: current.pinned,
          }
        : current,
  );
}

/**
 * Apply a successful Continue to `reviewedCardId` using the fetched
 * `FlowCardInfo` and the response's pendingCount. The same slot becomes
 * idle and inherits the reviewed pin; others are untouched. A missing ID or
 * stale generation is a no-op returning the same array.
 */
export function continueSuccess(
  slots: ReviewSlot[],
  reviewedCardId: bigint,
  generation: number,
  card: FlowCardInfo,
  pendingCount: number,
): ReviewSlot[] {
  const slot = slots.find(
    (
      candidate,
    ): candidate is Extract<ReviewSlot, { kind: "continuing" }> =>
      candidate.kind === "continuing" &&
      candidate.reviewedCardId === reviewedCardId &&
      candidate.generation === generation,
  );
  if (
    slot === undefined ||
    card.topicName !== slot.topicName ||
    card.tipcardType !== "repeatable_tip" ||
    card.status !== "active"
  )
    return slots;
  return slots.map((current) =>
    current.kind === "continuing" &&
    current.reviewedCardId === reviewedCardId &&
    current.generation === generation
      ? {
          kind: "idle",
          card: { ...card, pinned: current.pinned, pendingCount: BigInt(pendingCount) },
        }
      : current,
  );
}

/** Fields of a failed Continue this model consumes. */
export interface ContinueFailureFields {
  mutationOutcomeIndeterminate: boolean;
  message: string;
}

/**
 * Apply a failed Continue to `reviewedCardId`. Only that slot changes; a
 * stale generation or missing ID leaves everything untouched.
 */
export function continueFailure(
  slots: ReviewSlot[],
  reviewedCardId: bigint,
  generation: number,
  failure: ContinueFailureFields,
): ReviewSlot[] {
  const slot = slots.find(
    (
      candidate,
    ): candidate is Extract<ReviewSlot, { kind: "continuing" }> =>
      candidate.kind === "continuing" &&
      candidate.reviewedCardId === reviewedCardId &&
      candidate.generation === generation,
  );
  if (slot === undefined) return slots;
  return slots.map((current) =>
    current.kind === "continuing" &&
    current.reviewedCardId === reviewedCardId &&
    current.generation === generation
      ? {
          kind: "continueError",
          generation: current.generation,
          attempt: current.attempt,
          mutationOutcomeIndeterminate: failure.mutationOutcomeIndeterminate,
          message: failure.message,
          reviewedCardId: current.reviewedCardId,
          topicName: current.topicName,
          title: current.title,
          createdAt: current.createdAt,
          tipcardType: current.tipcardType,
          pinned: current.pinned,
        }
      : current,
  );
}

/**
 * Retry decision for a continue-errored slot: an outcome-indeterminate
 * failure reuses the exact prior {@link ContinueAttempt} (topic and key); a
 * determinate failure requires a fresh caller-provided key while retaining
 * the semantic topic.
 */
export type ContinueRetryDecision =
  | { kind: "reuseAttempt"; attempt: ContinueAttempt; generation: number }
  | {
      kind: "needsNewKey";
      topicName: string;
      generation: number;
    };

export function continueRetryDecision(slot: ReviewSlot): ContinueRetryDecision {
  if (slot.kind !== "continueError") {
    throw new TypeError("continueRetryDecision requires a continueError slot");
  }
  return slot.mutationOutcomeIndeterminate
    ? {
        kind: "reuseAttempt",
        attempt: slot.attempt,
        generation: slot.generation,
      }
    : {
        kind: "needsNewKey",
        topicName: slot.attempt.topicName,
        generation: slot.generation,
      };
}
