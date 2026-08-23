// Per-card pin/delete mutation hooks extracted from the Flow component. Each
// hook owns one slice of mutation state plus its synced ref and handlers;
// behavior is identical to the previous inline implementation — a pure
// extraction. The Flow component stays the sole owner of `apply(state)` and
// provides the `MutationHost`, so every commit remains atomic on the shared
// Flow state union.
import { useCallback } from "react";
import type { FlowState } from "./use-flow-pager";
import type { ReviewSlot } from "@/lib/flow-review-state";
import { pinTipcard, deleteTipcard } from "@/lib/api-v1/ops";
import { newIdempotencyKey } from "@/lib/api-v1/transport";
import {
    applyPinFailure,
    applyPinSuccess,
    EMPTY_PIN_STATE,
    pinCardState,
    pinRetryDecision,
    startPin,
    type PinAttempt,
    type PinState,
} from "@/lib/flow-pin-state";
import {
    applyDeleteFailure,
    applyDeleteSuccess,
    deleteCardState,
    deleteRetryDecision,
    EMPTY_DELETE_STATE,
    startDelete,
    type DeleteAttempt,
    type DeleteState,
} from "@/lib/flow-delete-state";
import { classifyReviewError } from "@/lib/flow-review-actions";
import { useSyncedRef } from "./use-synced-ref";

type SlotsState = Extract<
    FlowState,
    { kind: "ready" | "loading-more" | "load-error" }
>;

function isSlotsState(state: FlowState): state is SlotsState {
    return (
        state.kind === "ready" ||
        state.kind === "loading-more" ||
        state.kind === "load-error"
    );
}

/** What a pin/delete hook needs from the component to commit results. */
export interface MutationHost {
    /** Latest committed Flow state (synced ref). */
    getState(): FlowState;
    /**
     * Atomically replace the Flow state: `produce` receives the current
     * state and returns the next one; returning `current` aborts the commit.
     */
    produce(produce: (current: FlowState) => FlowState): void;
    /** Latest saved pinned order (synced ref), for delete success commits. */
    getPinnedOrder(): readonly bigint[];
    /** Persist a new saved pinned order. */
    applyPinnedOrder(next: readonly bigint[]): void;
    /** Still mounted; stale async results must not commit after unmount. */
    mounted(): boolean;
}

/**
 * Pin mutation lifecycle: per-card pin states with a synced ref, plus the
 * toggle/retry handlers and the async launcher. Commits land only while the
 * same attempt is still current; the slots commit goes through the host so
 * the Flow state union stays owned by the component.
 */
export function usePinMutations(host: MutationHost): PinMutations {
    const [pinStates, pinStatesRef, applyPins] =
        useSyncedRef<PinState>(EMPTY_PIN_STATE);

    const launchPin = useCallback(
        (attempt: PinAttempt) => {
            void (async () => {
                try {
                    await pinTipcard({
                        cardId: attempt.cardId,
                        pinned: attempt.targetPinned,
                        idempotencyKey: attempt.idempotencyKey,
                    });
                    if (!host.mounted()) return;
                    const current = host.getState();
                    if (!isSlotsState(current)) return;
                    const committed = applyPinSuccess(
                        current.slots,
                        pinStatesRef.current,
                        attempt,
                    );
                    if (committed.state === pinStatesRef.current) return; // stale
                    host.produce((state) =>
                        isSlotsState(state)
                            ? { ...state, slots: committed.slots }
                            : state,
                    );
                    applyPins(committed.state);
                } catch (error) {
                    if (!host.mounted()) return;
                    const current = host.getState();
                    if (!isSlotsState(current)) return;
                    const committed = applyPinFailure(
                        current.slots,
                        pinStatesRef.current,
                        attempt,
                        classifyReviewError(error),
                    );
                    if (committed.state === pinStatesRef.current) return; // stale
                    host.produce((state) =>
                        isSlotsState(state)
                            ? { ...state, slots: committed.slots }
                            : state,
                    );
                    applyPins(committed.state);
                }
            })();
        },
        [applyPins, host, pinStatesRef],
    );

    const beginPin = useCallback(
        (attempt: PinAttempt) => {
            const current = host.getState();
            if (!isSlotsState(current)) return;
            const started = startPin(
                current.slots,
                pinStatesRef.current,
                attempt,
            );
            if (started === undefined) return;
            applyPins(started);
            launchPin(attempt);
        },
        [host, launchPin, applyPins, pinStatesRef],
    );

    const onPinToggle = useCallback(
        (cardId: bigint, targetPinned: boolean) => {
            const previous = pinCardState(pinStatesRef.current, cardId);
            beginPin({
                cardId,
                targetPinned,
                idempotencyKey: newIdempotencyKey(),
                generation:
                    previous.kind === "idle"
                        ? 1
                        : previous.attempt.generation + 1,
            });
        },
        [beginPin, pinStatesRef],
    );

    const onPinRetry = useCallback(
        (cardId: bigint) => {
            const errored = pinCardState(pinStatesRef.current, cardId);
            if (errored.kind !== "error") return;
            const decision = pinRetryDecision(errored);
            beginPin(
                decision.kind === "reuseAttempt"
                    ? decision.attempt
                    : {
                          cardId: decision.cardId,
                          targetPinned: decision.targetPinned,
                          idempotencyKey: newIdempotencyKey(),
                          generation: decision.generation,
                      },
            );
        },
        [beginPin, pinStatesRef],
    );

    return {
        pinStates,
        pinStatesRef,
        onPinToggle,
        onPinRetry,
    };
}

export interface PinMutations {
    pinStates: PinState;
    /** Synced ref mirror for synchronous readers (busy checks). */
    pinStatesRef: Readonly<{ current: PinState }>;
    onPinToggle: (cardId: bigint, targetPinned: boolean) => void;
    onPinRetry: (cardId: bigint) => void;
}

/**
 * Delete mutation lifecycle: per-card delete states with a synced ref plus
 * confirm/retry handlers and the async launcher. Success removes the exact
 * slot in place, updates the shared pinned-order key, and collapses to
 * `empty` through the host when the last slot disappears.
 */
export function useDeleteMutations(host: MutationHost): DeleteMutations {
    const [deleteStates, deleteStatesRef, applyDeletes] =
        useSyncedRef<DeleteState>(EMPTY_DELETE_STATE);

    const launchDelete = useCallback(
        (attempt: DeleteAttempt) => {
            void (async () => {
                try {
                    await deleteTipcard({
                        cardId: attempt.cardId,
                        idempotencyKey: attempt.idempotencyKey,
                    });
                    if (!host.mounted()) return;
                    const current = host.getState();
                    if (!isSlotsState(current)) return;
                    const committed = applyDeleteSuccess(
                        current.slots,
                        deleteStatesRef.current,
                        attempt,
                        host.getPinnedOrder(),
                    );
                    if (committed.state === deleteStatesRef.current) return;
                    applyDeletes(committed.state);
                    host.applyPinnedOrder(committed.pinnedOrder);
                    host.produce((next) =>
                        !isSlotsState(next)
                            ? next
                            : committed.slots.length === 0
                              ? { kind: "empty" }
                              : { ...next, slots: committed.slots },
                    );
                } catch (error) {
                    if (!host.mounted()) return;
                    const current = host.getState();
                    if (!isSlotsState(current)) return;
                    const committed = applyDeleteFailure(
                        current.slots,
                        deleteStatesRef.current,
                        attempt,
                        host.getPinnedOrder(),
                        classifyReviewError(error),
                    );
                    if (committed.state === deleteStatesRef.current) return;
                    applyDeletes(committed.state);
                }
            })();
        },
        [applyDeletes, deleteStatesRef, host],
    );

    const beginDelete = useCallback(
        (attempt: DeleteAttempt) => {
            const current = host.getState();
            if (!isSlotsState(current)) return;
            const started = startDelete(
                current.slots,
                deleteStatesRef.current,
                attempt,
            );
            if (started === undefined) return;
            applyDeletes(started);
            launchDelete(attempt);
        },
        [applyDeletes, deleteStatesRef, host, launchDelete],
    );

    const onDeleteConfirm = useCallback(
        (cardId: bigint) => {
            const previous = deleteCardState(deleteStatesRef.current, cardId);
            beginDelete({
                cardId,
                idempotencyKey: newIdempotencyKey(),
                generation:
                    previous.kind === "idle"
                        ? 1
                        : previous.attempt.generation + 1,
            });
        },
        [beginDelete, deleteStatesRef],
    );

    const onDeleteRetry = useCallback(
        (cardId: bigint) => {
            const errored = deleteCardState(deleteStatesRef.current, cardId);
            if (errored.kind !== "error") return;
            const decision = deleteRetryDecision(errored);
            beginDelete(
                decision.kind === "reuseAttempt"
                    ? decision.attempt
                    : {
                          cardId: decision.cardId,
                          idempotencyKey: newIdempotencyKey(),
                          generation: decision.generation,
                      },
            );
        },
        [beginDelete, deleteStatesRef],
    );

    return {
        deleteStates,
        deleteStatesRef,
        onDeleteConfirm,
        onDeleteRetry,
    };
}

export interface DeleteMutations {
    deleteStates: DeleteState;
    /** Synced ref mirror for synchronous readers (busy checks). */
    deleteStatesRef: Readonly<{ current: DeleteState }>;
    onDeleteConfirm: (cardId: bigint) => void;
    onDeleteRetry: (cardId: bigint) => void;
}
