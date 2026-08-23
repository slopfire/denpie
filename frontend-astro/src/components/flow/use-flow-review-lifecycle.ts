// Flow review/continue lifecycle hook: owns the per-slot review and Continue
// mutation launches (`launchReview`, `launchContinue`) plus their handlers.
// The component keeps ownership of `apply(state)`; slot commits go through
// it atomically, and stale generations cannot commit.

import { useCallback } from "react";
import { continueDailyReview, getTipcard, reviewAndAdvance } from "@/lib/api-v1/ops";
import { newIdempotencyKey } from "@/lib/api-v1/transport";
import {
    continueFailure,
    continueRetryDecision,
    continueSuccess,
    refillPollFound,
    refillPollMiss,
    retryDecision,
    reviewFailure,
    reviewSuccess,
    startContinue,
    startReview,
    type ContinueAttempt,
    type ReviewAttempt,
} from "@/lib/flow-review-state";
import type { ReviewSlot } from "@/lib/flow-review-state";
import {
    classifyReviewError,
    type ReviewChoice,
} from "@/lib/flow-review-actions";
import type { FlowState } from "./use-flow-pager";

/** Delay before each bounded refill poll after an awaiting-refill slot. */
const REFILL_POLL_DELAY_MS = 2000;
/** Miss budget per awaiting-refill slot before it becomes completed. */
const REFILL_MAX_ATTEMPTS = 4;
/** Visual swipe delay for repeatable-card reviews. */
const REVIEW_SWIPE_DELAY_MS = 180;

function isLiveReviewSlot(
    slot: ReviewSlot,
): slot is Extract<ReviewSlot, { kind: "idle" | "reviewing" | "error" }> {
    return (
        slot.kind === "idle" ||
        slot.kind === "reviewing" ||
        slot.kind === "error"
    );
}

export interface ReviewHost {
    /** Latest committed Flow state (synced ref). */
    getState(): FlowState;
    /** Atomically replace the Flow state. */
    apply(next: FlowState): void;
    /** Still mounted (component-owned lifetime ref). */
    mounted(): boolean;
}

export interface FlowReviewLifecycle {
    onReview: (cardId: bigint, choice: ReviewChoice) => void;
    onRetry: (cardId: bigint) => void;
    onContinue: (reviewedCardId: bigint) => void;
}

export function useFlowReviewLifecycle(
    host: ReviewHost & {
        /** True when the card has no pin mutation in flight. */
        pinIdle(cardId: bigint): boolean;
    },
): FlowReviewLifecycle {
    const { pinIdle } = host;
    const apply = host.apply;

    /**
     * Launch exactly one card's review mutation. `attempt` and the
     * post-`startReview` slot list are provided by the caller; the per-slot
     * generation makes stale results — including after unmount or a newer
     * retry — unable to commit.
     */
    const launchReview = useCallback(
        (started: ReviewSlot[], cardId: bigint, attempt: ReviewAttempt) => {
            const target = started.find(
                (candidate) =>
                    candidate.kind === "reviewing" &&
                    candidate.card.id === cardId,
            );
            if (target?.kind !== "reviewing") return;
            const generation = target.generation;
            void (async () => {
                try {
                    if (target.card.tipcardType === "repeatable_tip") {
                        await new Promise((resolve) =>
                            setTimeout(resolve, REVIEW_SWIPE_DELAY_MS),
                        );
                        if (!host.mounted()) return;
                    }
                    const outcome = await reviewAndAdvance({
                        cardId,
                        grade: attempt.grade,
                        action: attempt.action,
                        idempotencyKey: attempt.idempotencyKey,
                    });
                    if (!host.mounted()) return;
                    const current = host.getState();
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const nextSlots = reviewSuccess(
                        current.slots,
                        outcome.reviewedCardId,
                        generation,
                        outcome,
                    );
                    if (nextSlots === current.slots) return; // stale generation
                    apply({ ...current, slots: nextSlots });
                } catch (error) {
                    if (!host.mounted()) return;
                    const current = host.getState();
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const nextSlots = reviewFailure(
                        current.slots,
                        cardId,
                        generation,
                        classifyReviewError(error),
                    );
                    if (nextSlots === current.slots) return; // stale generation
                    apply({ ...current, slots: nextSlots });
                }
            })();
        },
        [apply, host],
    );

    const onReview = useCallback(
        (cardId: bigint, choice: ReviewChoice) => {
            if (!pinIdle(cardId)) return;
            const current = host.getState();
            if (
                current.kind !== "ready" &&
                current.kind !== "loading-more" &&
                current.kind !== "load-error"
            )
                return;
            const attempt: ReviewAttempt = {
                grade: choice.grade,
                action: choice.action,
                idempotencyKey: newIdempotencyKey(),
            };
            const started = startReview(current.slots, cardId, attempt);
            if (started === current.slots) return;
            apply({ ...current, slots: started });
            launchReview(started, cardId, attempt);
        },
        [apply, host, launchReview, pinIdle],
    );

    const onRetry = useCallback(
        (cardId: bigint) => {
            if (!pinIdle(cardId)) return;
            const current = host.getState();
            if (
                current.kind !== "ready" &&
                current.kind !== "loading-more" &&
                current.kind !== "load-error"
            )
                return;
            const errored = current.slots.find(
                (slot) => slot.kind === "error" && slot.card.id === cardId,
            );
            if (errored?.kind !== "error") return;
            const decision = retryDecision(errored);
            const attempt: ReviewAttempt =
                decision.kind === "reuseAttempt"
                    ? decision.attempt
                    : {
                          grade: decision.grade,
                          action: decision.action,
                          idempotencyKey: newIdempotencyKey(),
                      };
            const started = startReview(current.slots, cardId, attempt);
            if (started === current.slots) return;
            apply({ ...current, slots: started });
            launchReview(started, cardId, attempt);
        },
        [apply, host, launchReview, pinIdle],
    );

    /**
     * Launch exactly one Continue mutation. The per-slot generation captured
     * from the `continuing` slot makes stale results unable to commit.
     */
    const launchContinue = useCallback(
        (
            started: ReviewSlot[],
            reviewedCardId: bigint,
            attempt: ContinueAttempt,
        ) => {
            const target = started.find(
                (candidate) =>
                    candidate.kind === "continuing" &&
                    candidate.reviewedCardId === reviewedCardId,
            );
            if (target?.kind !== "continuing") return;
            const generation = target.generation;
            void (async () => {
                let mutationReturned = false;
                try {
                    const outcome = await continueDailyReview({
                        topicName: attempt.topicName,
                        idempotencyKey: attempt.idempotencyKey,
                    });
                    mutationReturned = true;
                    const detail = await getTipcard({
                        cardId: outcome.activeCardId,
                    });
                    if (!host.mounted()) return;
                    const current = host.getState();
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const nextSlots = continueSuccess(
                        current.slots,
                        reviewedCardId,
                        generation,
                        detail.card,
                        outcome.pendingCount,
                    );
                    if (nextSlots === current.slots) return; // stale generation
                    apply({ ...current, slots: nextSlots });
                } catch (error) {
                    if (!host.mounted()) return;
                    const current = host.getState();
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const classified = classifyReviewError(error);
                    // Once the mutation itself returned successfully, a
                    // detail-read failure cannot prove the mutation did not
                    // commit: force the indeterminate verdict so Retry reuses
                    // the exact same key and obtains the idempotent result.
                    const failure = mutationReturned
                        ? { ...classified, mutationOutcomeIndeterminate: true }
                        : classified;
                    const nextSlots = continueFailure(
                        current.slots,
                        reviewedCardId,
                        generation,
                        failure,
                    );
                    if (nextSlots === current.slots) return; // stale generation
                    apply({ ...current, slots: nextSlots });
                }
            })();
        },
        [apply, host],
    );

    /** Start (or retry) Continue for one completed/errored slot. */
    const onContinue = useCallback(
        (reviewedCardId: bigint) => {
            const current = host.getState();
            if (
                current.kind !== "ready" &&
                current.kind !== "loading-more" &&
                current.kind !== "load-error"
            )
                return;
            const slot = current.slots.find(
                (candidate) =>
                    (candidate.kind === "completed" ||
                        candidate.kind === "continueError") &&
                    candidate.reviewedCardId === reviewedCardId,
            );
            if (slot?.kind !== "completed" && slot?.kind !== "continueError")
                return;
            const attempt: ContinueAttempt =
                slot.kind === "continueError"
                    ? (() => {
                          const decision = continueRetryDecision(slot);
                          return decision.kind === "reuseAttempt"
                              ? decision.attempt
                              : {
                                    topicName: decision.topicName,
                                    idempotencyKey: newIdempotencyKey(),
                                };
                      })()
                    : {
                          topicName: slot.topicName,
                          idempotencyKey: newIdempotencyKey(),
                      };
            const started = startContinue(
                current.slots,
                reviewedCardId,
                attempt,
            );
            if (started === current.slots) return;
            apply({ ...current, slots: started });
            launchContinue(started, reviewedCardId, attempt);
        },
        [apply, host, launchContinue],
    );

    return { onReview, onRetry, onContinue };
}

export {
    isLiveReviewSlot,
    REFILL_POLL_DELAY_MS,
    REFILL_MAX_ATTEMPTS,
};
