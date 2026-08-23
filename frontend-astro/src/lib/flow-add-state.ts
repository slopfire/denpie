// Pure add-form lifecycle. The component owns effects; this module makes an
// async completion committable only by the exact submission and resolution run
// that launched it.

import type { AddTipsPayload } from "./flow-add-form";

/** The exact mutation submission, including its caller-owned idempotency key. */
export interface AddAttempt {
    payload: AddTipsPayload;
    /** Monotonic per-form submission ownership token. */
    submissionGeneration: number;
}

/** One detail/reconciliation pass after a successful mutation. */
export interface AddResolutionRun {
    attempt: AddAttempt;
    createdIds: readonly bigint[];
    /** Monotonic per-success resolution ownership token. */
    resolutionGeneration: number;
}

export type AddLifecycle =
    | { kind: "idle" }
    | { kind: "submitting"; attempt: AddAttempt }
    | { kind: "resolving"; run: AddResolutionRun }
    | {
          kind: "mutationError";
          attempt: AddAttempt;
          message: string;
          mutationOutcomeIndeterminate: boolean;
      }
    | { kind: "resolutionError"; run: AddResolutionRun; message: string };

export interface AddFailureFields {
    mutationOutcomeIndeterminate: boolean;
    message: string;
}

function sameAttempt(left: AddAttempt, right: AddAttempt): boolean {
    return (
        left === right ||
        (left.submissionGeneration === right.submissionGeneration &&
            left.payload === right.payload &&
            left.payload.idempotencyKey === right.payload.idempotencyKey)
    );
}

function sameResolutionRun(
    left: AddResolutionRun,
    right: AddResolutionRun,
): boolean {
    return (
        left === right ||
        (left.resolutionGeneration === right.resolutionGeneration &&
            sameAttempt(left.attempt, right.attempt) &&
            left.createdIds === right.createdIds)
    );
}

function sameTextList(
    left: readonly string[],
    right: readonly string[],
): boolean {
    return (
        left.length === right.length &&
        left.every((value, index) => value === right[index])
    );
}

function sameSemanticPayload(
    left: AddTipsPayload,
    right: AddTipsPayload,
): boolean {
    return (
        left.kind === right.kind &&
        left.manualContent === right.manualContent &&
        sameTextList(left.topics, right.topics) &&
        sameTextList(left.manualImageData, right.manualImageData)
    );
}

/** Only a clean idle form may launch a brand-new mutation. */
export function canStartAdd(state: AddLifecycle): boolean {
    return state.kind === "idle";
}

/** Begin the exact caller-captured initial submission. */
export function startAdd(
    state: AddLifecycle,
    attempt: AddAttempt,
): AddLifecycle {
    return canStartAdd(state) ? { kind: "submitting", attempt } : state;
}

/** Commit a mutation failure only to its still-current submission. */
export function addFailed(
    state: AddLifecycle,
    attempt: AddAttempt,
    failure: AddFailureFields,
): AddLifecycle {
    if (state.kind !== "submitting" || !sameAttempt(state.attempt, attempt)) {
        return state;
    }
    return {
        kind: "mutationError",
        attempt: state.attempt,
        message: failure.message,
        mutationOutcomeIndeterminate: failure.mutationOutcomeIndeterminate,
    };
}

/** Commit mutation success only when its captured submission still owns state. */
export function addMutationSucceeded(
    state: AddLifecycle,
    attempt: AddAttempt,
    run: AddResolutionRun,
): AddLifecycle {
    if (
        state.kind !== "submitting" ||
        !sameAttempt(state.attempt, attempt) ||
        !sameAttempt(run.attempt, attempt)
    ) {
        return state;
    }
    return { kind: "resolving", run };
}

/** Commit successful details/reconciliation only to the exact live run. */
export function resolveSettled(
    state: AddLifecycle,
    run: AddResolutionRun,
): AddLifecycle {
    return state.kind === "resolving" && sameResolutionRun(state.run, run)
        ? { kind: "idle" }
        : state;
}

/** Record a details/reconciliation failure only for its still-current run. */
export function resolveFailed(
    state: AddLifecycle,
    run: AddResolutionRun,
    message: string,
): AddLifecycle {
    return state.kind === "resolving" && sameResolutionRun(state.run, run)
        ? { kind: "resolutionError", run: state.run, message }
        : state;
}

export type AddRetryDecision =
    | { kind: "reuseAttempt"; attempt: AddAttempt }
    | {
          kind: "needsFreshAttempt";
          payload: Omit<AddTipsPayload, "idempotencyKey">;
          submissionGeneration: number;
      };

/**
 * Decide a mutation retry from an actual mutation error. Indeterminate
 * outcomes reuse the complete captured attempt; determinate outcomes require
 * Flow to provide a different idempotency key in the returned next attempt.
 */
export function addRetryDecision(
    state: Extract<AddLifecycle, { kind: "mutationError" }>,
): AddRetryDecision {
    if (state.mutationOutcomeIndeterminate) {
        return { kind: "reuseAttempt", attempt: state.attempt };
    }
    const { idempotencyKey: _previousKey, ...payload } = state.attempt.payload;
    return {
        kind: "needsFreshAttempt",
        payload,
        submissionGeneration: state.attempt.submissionGeneration + 1,
    };
}

/**
 * Start a mutation retry only when it obeys the decision above. This prevents
 * a stale retry from replacing a newer error, and prevents a determinate
 * retry from accidentally reusing the old key or generation.
 */
export function startMutationRetry(
    state: AddLifecycle,
    attempt: AddAttempt,
): AddLifecycle {
    if (state.kind !== "mutationError") return state;
    if (state.mutationOutcomeIndeterminate) {
        return sameAttempt(state.attempt, attempt)
            ? { kind: "submitting", attempt: state.attempt }
            : state;
    }
    const previous = state.attempt;
    const validFreshAttempt =
        attempt.submissionGeneration === previous.submissionGeneration + 1 &&
        attempt.payload.idempotencyKey.trim() !== "" &&
        attempt.payload.idempotencyKey !== previous.payload.idempotencyKey &&
        sameSemanticPayload(attempt.payload, previous.payload);
    return validFreshAttempt ? { kind: "submitting", attempt } : state;
}

export interface ResolutionRetryDecision {
    kind: "retryResolution";
    run: AddResolutionRun;
}

/**
 * Resolution retry has no mutation-shaped result: it can only reuse the
 * already-created IDs and advance the resolution ownership token.
 */
export function resolutionRetryDecision(
    state: Extract<AddLifecycle, { kind: "resolutionError" }>,
    nextResolutionGeneration: number,
): ResolutionRetryDecision | undefined {
    if (nextResolutionGeneration !== state.run.resolutionGeneration + 1) {
        return undefined;
    }
    return {
        kind: "retryResolution",
        run: {
            attempt: state.run.attempt,
            createdIds: state.run.createdIds,
            resolutionGeneration: nextResolutionGeneration,
        },
    };
}

/** Claim the exact run produced by {@link resolutionRetryDecision}. */
export function startResolutionRetry(
    state: AddLifecycle,
    decision: ResolutionRetryDecision,
): AddLifecycle {
    if (state.kind !== "resolutionError") return state;
    const next = decision.run;
    return sameAttempt(next.attempt, state.run.attempt) &&
        next.createdIds === state.run.createdIds &&
        next.resolutionGeneration === state.run.resolutionGeneration + 1
        ? { kind: "resolving", run: next }
        : state;
}
