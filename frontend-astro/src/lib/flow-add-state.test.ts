import { describe, expect, test } from "bun:test";
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
} from "./flow-add-state";
import type { AddTipsPayload } from "./flow-add-form";

function payload(key = "key-1"): AddTipsPayload {
    return {
        kind: "manual",
        topics: [" Rust "],
        manualContent: "notes",
        manualImageData: ["data:image/png;base64,abc"],
        idempotencyKey: key,
    };
}

function attempt(submissionGeneration = 1, key = "key-1"): AddAttempt {
    return { payload: payload(key), submissionGeneration };
}

function run(owner: AddAttempt, resolutionGeneration = 1): AddResolutionRun {
    return {
        attempt: owner,
        createdIds: [41n, 43n],
        resolutionGeneration,
    };
}

function requireMutationError(
    state: AddLifecycle,
): Extract<AddLifecycle, { kind: "mutationError" }> {
    if (state.kind !== "mutationError")
        throw new TypeError("expected mutation error");
    return state;
}

function requireResolutionError(
    state: AddLifecycle,
): Extract<AddLifecycle, { kind: "resolutionError" }> {
    if (state.kind !== "resolutionError")
        throw new TypeError("expected resolution error");
    return state;
}

describe("add lifecycle ownership", () => {
    test("starts only an idle state with the exact captured attempt", () => {
        const owner = attempt();
        const submitting = startAdd({ kind: "idle" }, owner);
        expect(submitting).toEqual({ kind: "submitting", attempt: owner });
        expect(canStartAdd(submitting)).toBe(false);
        expect(startAdd(submitting, attempt(2, "key-2"))).toBe(submitting);
    });

    test("stale mutation success and failure leave the current submission untouched", () => {
        const owner = attempt();
        const submitting = startAdd({ kind: "idle" }, owner);
        const stale = attempt(2, "key-2");
        expect(
            addFailed(submitting, stale, {
                mutationOutcomeIndeterminate: false,
                message: "stale",
            }),
        ).toBe(submitting);
        expect(addMutationSucceeded(submitting, stale, run(stale))).toBe(
            submitting,
        );
        expect(addMutationSucceeded(submitting, owner, run(stale))).toBe(
            submitting,
        );
    });

    test("records mutation and resolution failures as distinct states", () => {
        const owner = attempt();
        const mutationError = addFailed(
            startAdd({ kind: "idle" }, owner),
            owner,
            {
                mutationOutcomeIndeterminate: true,
                message: "network lost",
            },
        );
        expect(mutationError.kind).toBe("mutationError");

        const detailRun = run(owner);
        const resolutionError = resolveFailed(
            addMutationSucceeded(
                startAdd({ kind: "idle" }, owner),
                owner,
                detailRun,
            ),
            detailRun,
            "detail unavailable",
        );
        expect(resolutionError.kind).toBe("resolutionError");
    });
});

describe("mutation retry", () => {
    test("an indeterminate failure reuses the exact captured attempt", () => {
        const owner = attempt();
        const error = requireMutationError(
            addFailed(startAdd({ kind: "idle" }, owner), owner, {
                mutationOutcomeIndeterminate: true,
                message: "lost response",
            }),
        );
        const decision = addRetryDecision(error);
        expect(decision.kind).toBe("reuseAttempt");
        if (decision.kind !== "reuseAttempt")
            throw new TypeError("expected reuse");
        expect(decision.attempt).toBe(owner);
        expect(startMutationRetry(error, decision.attempt)).toEqual({
            kind: "submitting",
            attempt: owner,
        });
        expect(startMutationRetry(error, attempt(2, "new-key"))).toBe(error);
    });

    test("a determinate failure requires one new key and next submission generation", () => {
        const owner = attempt(4, "old-key");
        const error = requireMutationError(
            addFailed(startAdd({ kind: "idle" }, owner), owner, {
                mutationOutcomeIndeterminate: false,
                message: "rejected",
            }),
        );
        const decision = addRetryDecision(error);
        expect(decision).toEqual({
            kind: "needsFreshAttempt",
            payload: {
                kind: "manual",
                topics: [" Rust "],
                manualContent: "notes",
                manualImageData: ["data:image/png;base64,abc"],
            },
            submissionGeneration: 5,
        });
        if (decision.kind !== "needsFreshAttempt")
            throw new TypeError("expected fresh attempt");
        const fresh: AddAttempt = {
            payload: { ...decision.payload, idempotencyKey: "fresh-key" },
            submissionGeneration: decision.submissionGeneration,
        };
        expect(startMutationRetry(error, fresh)).toEqual({
            kind: "submitting",
            attempt: fresh,
        });
        expect(
            startMutationRetry(error, {
                payload: { ...decision.payload, idempotencyKey: "old-key" },
                submissionGeneration: decision.submissionGeneration,
            }),
        ).toBe(error);
        expect(
            startMutationRetry(error, {
                payload: { ...decision.payload, idempotencyKey: "fresh-key" },
                submissionGeneration: decision.submissionGeneration + 1,
            }),
        ).toBe(error);
    });
});

describe("resolution retry", () => {
    test("settle/failure guard the exact resolution run", () => {
        const owner = attempt();
        const detailRun = run(owner, 7);
        const resolving = addMutationSucceeded(
            startAdd({ kind: "idle" }, owner),
            owner,
            detailRun,
        );
        const staleRun = run(owner, 8);
        expect(resolveSettled(resolving, staleRun)).toBe(resolving);
        expect(resolveFailed(resolving, staleRun, "stale")).toBe(resolving);
        expect(resolveSettled(resolving, detailRun)).toEqual({ kind: "idle" });
    });

    test("resolution retry can only produce and claim a newer resolution run", () => {
        const owner = attempt();
        const detailRun = run(owner, 3);
        const error = requireResolutionError(
            resolveFailed(
                addMutationSucceeded(
                    startAdd({ kind: "idle" }, owner),
                    owner,
                    detailRun,
                ),
                detailRun,
                "list failed",
            ),
        );
        expect(resolutionRetryDecision(error, 5)).toBeUndefined();
        const decision = resolutionRetryDecision(error, 4);
        if (decision === undefined)
            throw new TypeError("expected resolution retry");
        expect(decision.kind).toBe("retryResolution");
        expect(decision.run.attempt).toBe(owner);
        expect(decision.run.createdIds).toBe(detailRun.createdIds);
        expect(startResolutionRetry(error, decision)).toEqual({
            kind: "resolving",
            run: decision.run,
        });
        const retrying = startResolutionRetry(error, decision);
        expect(resolveSettled(retrying, detailRun)).toBe(retrying);
        expect(resolveFailed(retrying, detailRun, "old run")).toBe(retrying);
    });
});
