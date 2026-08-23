// Add-card form lifecycle hook: owns `AddLifecycle` state, the double-submit
// guard, and every launch path (mutation, resolution/reconciliation). The
// component stays the sole owner of the Flow slots state; slot integration
// goes through `host.apply` exactly like pin/delete. Behavior is identical
// to the previous inline implementation in Flow.tsx — a pure extraction.

import { useCallback, useState } from "react";
import type { FlowCardInfo } from "@/generated/denpie_pb";
import type { ReviewSlot } from "@/lib/flow-review-state";
import { createTips, getTipcard, listFlowCards } from "@/lib/api-v1/ops";
import { TransportError, newIdempotencyKey } from "@/lib/api-v1/transport";
import {
    addFailed,
    addMutationSucceeded,
    addRetryDecision,
    canStartAdd,
    resolutionRetryDecision,
    resolveFailed,
    resolveSettled,
    startAdd,
    startMutationRetry,
    startResolutionRetry,
    type AddAttempt,
    type AddLifecycle,
    type AddResolutionRun,
} from "@/lib/flow-add-state";
import {
    integrateCreatedCards,
    mergeReconciledCards,
} from "@/lib/flow-add-integration";
import { buildTipsRequest, clearAddPrefill } from "@/lib/flow-add-form";
import type { AddTipsPayload } from "@/lib/flow-add-form";
import { useSyncedRef } from "./use-synced-ref";

/** Default first-page size for the quiet reconciliation read. */
const PAGE_SIZE = 48;

/**
 * A Flow state shape the add lifecycle may integrate created cards into:
 * every variant carries a discriminant, and the slots-bearing ones carry
 * their slots. `S` (the component's concrete union) satisfies this.
 */
export interface AddFlowState {
    kind: string;
    /** Present exactly on the slots-bearing variants. */
    slots?: ReviewSlot[];
}

/** What the add hook needs from the component to integrate created cards. */
export interface AddHost<S extends AddFlowState> {
    /** Latest committed Flow state (synced ref). */
    getState(): S;
    /** Atomically replace the Flow state. */
    apply(next: S): void;
    /** IDs currently owned by pin/delete mutations; never replaced. */
    busyCardIds(): bigint[];
    /** Latest saved pinned order (synced ref). */
    getPinnedOrder(): readonly bigint[];
    /** Persist a new saved pinned order. */
    applyPinnedOrder(next: readonly bigint[]): void;
}

export interface FlowAddLifecycle {
    addState: AddLifecycle;
    addedNotice: boolean;
    /** True while a submit/mutation/resolution run owns the form. */
    isInFlight(): boolean;
    onAddSubmit: (payload: AddTipsPayload) => void;
    onAddRetryMutation: () => void;
    onAddRetryResolve: () => void;
}

/** A slots-bearing state of the concrete Flow union `S`. */
type ReadyLike<S> = Extract<S, { kind: "ready" | "loading-more" | "load-error" }>;

export function useFlowAddLifecycle<S extends AddFlowState>(
    host: AddHost<S>,
): FlowAddLifecycle {
    const [addState, addStateRef, applyAdd] =
        useSyncedRef<AddLifecycle>({ kind: "idle" });
    // Monotonic submission counter for stale-run rejection.
    const [addGeneration, addGenerationRef, setAddGeneration] = useSyncedRef(0);
    // Double-submit guard: a click while a launch is pending cannot start twice.
    const [addInFlight, addInFlightRef, setAddInFlight] = useSyncedRef(false);
    const [addedNotice, setAddedNotice] = useState(false);

    /**
     * Atomically integrate one resolved batch into the latest slots and saved
     * pinned order. Returns whether an authoritative quiet list read is still
     * required; stale resolution runs cannot alter Flow state.
     */
    const commitIntegratedCards = useCallback(
        (run: AddResolutionRun, cards: FlowCardInfo[]): boolean => {
            const add = addStateRef.current;
            if (add.kind !== "resolving" || add.run !== run) return false;
            const current = host.getState();
            const slots =
                current.kind === "empty"
                    ? []
                    : current.kind === "ready" ||
                        current.kind === "loading-more" ||
                        current.kind === "load-error"
                      ? (current as ReadyLike<S>).slots
                      : undefined;
            if (slots === undefined) return false;
            const integrated = integrateCreatedCards({
                slots,
                cards,
                pinnedOrder: host.getPinnedOrder(),
                busyCardIds: host.busyCardIds(),
            });
            host.applyPinnedOrder(integrated.pinnedOrder);
            if (current.kind === "empty") {
                if (integrated.slots.length > 0) {
                    host.apply({
                        ...current,
                        kind: "ready",
                        slots: integrated.slots,
                        cursor: { kind: "end" },
                    });
                }
            } else {
                host.apply({
                    ...current,
                    slots: integrated.slots,
                });
            }
            return integrated.needsReconciliation;
        },
        [addStateRef, host],
    );

    /**
     * Resolution phase: after `tips_v1` succeeded, resolve every returned
     * positive ID with `get_tipcard`, integrate the details, then — for
     * repeatable creation, an empty created-ID list, or any detail failure —
     * run one authoritative quiet list reconciliation. Never resubmits the
     * mutation; stale/unmounted results cannot commit.
     */
    const launchResolution = useCallback(
        (run: AddResolutionRun) => {
            void (async () => {
                try {
                    const outcomes = await Promise.allSettled(
                        run.createdIds.map(
                            async (id) =>
                                (await getTipcard({ cardId: id })).card,
                        ),
                    );
                    if (
                        addStateRef.current.kind !== "resolving" ||
                        addStateRef.current.run !== run
                    )
                        return;
                    const details = outcomes.flatMap((outcome) =>
                        outcome.status === "fulfilled" ? [outcome.value] : [],
                    );
                    const detailFailed = outcomes.some(
                        (outcome) => outcome.status === "rejected",
                    );
                    const integrationNeedsReconcile = commitIntegratedCards(
                        run,
                        details,
                    );
                    const needsReconcile =
                        run.attempt.payload.kind === "repeatable" ||
                        run.createdIds.length === 0 ||
                        detailFailed ||
                        integrationNeedsReconcile;
                    if (!needsReconcile) {
                        const next = resolveSettled(addStateRef.current, run);
                        if (next !== addStateRef.current) {
                            applyAdd(next);
                            setAddedNotice(true);
                        }
                        return;
                    }
                    try {
                        const page = await listFlowCards({
                            pageSize: PAGE_SIZE,
                        });
                        if (
                            addStateRef.current.kind !== "resolving" ||
                            addStateRef.current.run !== run
                        )
                            return;
                        const current = host.getState();
                        if (
                            current.kind === "ready" ||
                            current.kind === "loading-more" ||
                            current.kind === "load-error"
                        ) {
                            const ready = current as ReadyLike<S>;
                            host.apply({
                                ...ready,
                                slots: mergeReconciledCards(
                                    ready.slots ?? [],
                                    page.cards,
                                ),
                            });
                        }
                        const next = resolveSettled(addStateRef.current, run);
                        if (next !== addStateRef.current) {
                            applyAdd(next);
                            setAddedNotice(true);
                        }
                    } catch (error) {
                        applyAdd(
                            resolveFailed(
                                addStateRef.current,
                                run,
                                error instanceof Error
                                    ? error.message
                                    : String(error),
                            ),
                        );
                    }
                } catch (error) {
                    applyAdd(
                        resolveFailed(
                            addStateRef.current,
                            run,
                            error instanceof Error
                                ? error.message
                                : String(error),
                        ),
                    );
                } finally {
                    // Resolution owns the guard: it starts only after the mutation
                    // settled and covers both the submit and resolve-retry paths.
                    setAddInFlight(false);
                }
            })();
        },
        [addStateRef, applyAdd, commitIntegratedCards, host, setAddInFlight],
    );

    /** Launch exactly one `tips_v1` mutation for the captured attempt. */
    const launchAddMutation = useCallback(
        (attempt: AddAttempt) => {
            void (async () => {
                try {
                    const outcome = await createTips({
                        request: buildTipsRequest(attempt.payload),
                        idempotencyKey: attempt.payload.idempotencyKey,
                    });
                    if (addStateRef.current.kind === "idle") return;
                    const createdIds = outcome.tips.map((tip) => tip.id);
                    const run: AddResolutionRun = {
                        attempt,
                        createdIds,
                        resolutionGeneration: 1,
                    };
                    const next = addMutationSucceeded(
                        addStateRef.current,
                        attempt,
                        run,
                    );
                    if (next === addStateRef.current) return;
                    clearAddPrefill();
                    applyAdd(next);
                    launchResolution(run);
                } catch (error) {
                    if (
                        addStateRef.current.kind !== "submitting" &&
                        addStateRef.current.kind !== "mutationError"
                    )
                        return;
                    const indeterminate =
                        error instanceof TransportError
                            ? error.mutationOutcomeIndeterminate
                            : true;
                    applyAdd(
                        addFailed(addStateRef.current, attempt, {
                            mutationOutcomeIndeterminate: indeterminate,
                            message:
                                error instanceof Error
                                    ? error.message
                                    : String(error),
                        }),
                    );
                    setAddInFlight(false);
                }
            })();
        },
        [addStateRef, applyAdd, launchResolution, setAddInFlight],
    );

    const onAddSubmit = useCallback(
        (payload: AddTipsPayload) => {
            if (addInFlightRef.current || !canStartAdd(addStateRef.current))
                return;
            setAddInFlight(true);
            setAddGeneration(addGenerationRef.current + 1);
            setAddedNotice(false);
            const attempt: AddAttempt = {
                payload,
                submissionGeneration: addGenerationRef.current,
            };
            applyAdd(startAdd(addStateRef.current, attempt));
            launchAddMutation(attempt);
        },
        [
            addGenerationRef,
            addInFlightRef,
            addStateRef,
            applyAdd,
            launchAddMutation,
            setAddGeneration,
        ],
    );

    /**
     * Mutation retry. An outcome-indeterminate failure reuses the exact
     * captured payload and key; a determinate failure preserves the semantic
     * payload with a fresh key/generation.
     */
    const onAddRetryMutation = useCallback(() => {
        if (addInFlightRef.current) return;
        const errored = addStateRef.current;
        if (errored.kind !== "mutationError") return;
        const decision = addRetryDecision(errored);
        const attempt: AddAttempt =
            decision.kind === "reuseAttempt"
                ? decision.attempt
                : {
                      payload: {
                          ...decision.payload,
                          idempotencyKey: newIdempotencyKey(),
                      },
                      submissionGeneration: decision.submissionGeneration,
                  };
        const next = startMutationRetry(errored, attempt);
        if (next === errored) return;
        setAddInFlight(true);
        setAddGeneration(Math.max(addGenerationRef.current, attempt.submissionGeneration));
        applyAdd(next);
        launchAddMutation(attempt);
    }, [
        addGenerationRef,
        addInFlightRef,
        addStateRef,
        applyAdd,
        launchAddMutation,
        setAddGeneration,
        setAddInFlight,
    ]);

    /**
     * Post-mutation retry: retries only detail resolution/reconciliation —
     * never the already-successful mutation.
     */
    const onAddRetryResolve = useCallback(() => {
        const errored = addStateRef.current;
        if (errored.kind !== "resolutionError" || addInFlightRef.current) return;
        const decision = resolutionRetryDecision(
            errored,
            errored.run.resolutionGeneration + 1,
        );
        if (decision === undefined) return;
        const next = startResolutionRetry(errored, decision);
        if (next === errored) return;
        setAddInFlight(true);
        applyAdd(next);
        launchResolution(decision.run);
    }, [
        addInFlightRef,
        addStateRef,
        applyAdd,
        launchResolution,
        setAddInFlight,
    ]);

    // Stable identity: consumers use this inside dependency arrays (e.g. the
    // silent-refresh guard); a per-render arrow would retrigger them forever.
    const isInFlight = useCallback(() => addInFlightRef.current, []);

    return {
        addState,
        addedNotice,
        isInFlight,
        onAddSubmit,
        onAddRetryMutation,
        onAddRetryResolve,
    };
}
