// Stale-while-revalidate store for one server-backed view model.
//
// Pages used to flip to a skeleton on every refresh (focus, the 60 s timer,
// after each mutation). That unmounted the rendered tree, so scroll position,
// open dialogs, and half-typed forms were lost, and a slow response from
// before a mutation could overwrite the result of the mutation. This store
// owns those rules in one place:
//
// - `loading` exists only while there is no data at all. A refresh with data
//   keeps the data on screen (`refreshing`), and a failed refresh keeps it too
//   (`error` is a soft message beside the data).
// - At most one fetch is in flight. A refresh requested meanwhile queues one
//   rerun instead of being dropped, so a change that lands mid-fetch is never
//   missed.
// - A result is discarded, then refetched, when the server data changed while
//   it was in flight (the read-cache epoch moved) or when a local patch was
//   applied: it can no longer be trusted to be newer than what is on screen.
//
// No React, no DOM: the hook in `islands/use-live-resource.ts` binds it.

export type LiveState<T> =
    | { kind: "loading" }
    | { kind: "ready"; data: T; refreshing: boolean; error: string | null }
    | { kind: "error"; message: string };

export interface LiveResourceOptions<T> {
    fetch(): Promise<T>;
    errorMessage(error: unknown): string;
    /** Monotonic server-data epoch; a change mid-fetch marks the result stale. */
    epoch?(): number;
}

export interface LiveResource<T> {
    getState(): LiveState<T>;
    subscribe(listener: () => void): () => void;
    /** Fetch now; resolves when the store has settled on the latest data. */
    refresh(): Promise<void>;
    /**
     * Replace the data through a pure update (an authoritative mutation result
     * or a user edit). No-op unless data exists. Supersedes any in-flight
     * fetch, which is discarded and refetched.
     */
    patch(update: (data: T) => T): void;
    dispose(): void;
}

export function createLiveResource<T>(
    options: LiveResourceOptions<T>,
): LiveResource<T> {
    const epochOf = options.epoch ?? (() => 0);
    const listeners = new Set<() => void>();
    let state: LiveState<T> = { kind: "loading" };
    let disposed = false;
    // Bumped by patch(): a fetch that began earlier is stale.
    let patchVersion = 0;
    let inFlight: Promise<void> | null = null;
    let rerunRequested = false;

    const commit = (next: LiveState<T>): void => {
        state = next;
        for (const listener of listeners) listener();
    };

    const runOnce = async (): Promise<void> => {
        const startedPatchVersion = patchVersion;
        const startedEpoch = epochOf();
        if (state.kind === "ready" && !state.refreshing) {
            commit({ ...state, refreshing: true });
        }
        let outcome:
            | { ok: true; data: T }
            | { ok: false; message: string };
        try {
            outcome = { ok: true, data: await options.fetch() };
        } catch (error) {
            outcome = { ok: false, message: options.errorMessage(error) };
        }
        if (disposed) return;
        if (
            startedPatchVersion !== patchVersion ||
            startedEpoch !== epochOf()
        ) {
            // Older than what the user, or a mutation, already established.
            rerunRequested = true;
            return;
        }
        if (outcome.ok) {
            commit({
                kind: "ready",
                data: outcome.data,
                refreshing: false,
                error: null,
            });
        } else if (state.kind === "ready") {
            commit({ ...state, refreshing: false, error: outcome.message });
        } else {
            commit({ kind: "error", message: outcome.message });
        }
    };

    const drain = async (): Promise<void> => {
        do {
            rerunRequested = false;
            await runOnce();
        } while (rerunRequested && !disposed);
    };

    return {
        getState: () => state,
        subscribe(listener) {
            listeners.add(listener);
            return () => {
                listeners.delete(listener);
            };
        },
        refresh() {
            if (disposed) return Promise.resolve();
            if (inFlight !== null) {
                rerunRequested = true;
                return inFlight;
            }
            // A retry after a cold-start failure shows the skeleton again.
            if (state.kind === "error") commit({ kind: "loading" });
            const running = drain().finally(() => {
                if (inFlight === running) inFlight = null;
            });
            inFlight = running;
            return running;
        },
        patch(update) {
            if (disposed || state.kind !== "ready") return;
            patchVersion += 1;
            commit({ ...state, data: update(state.data) });
        },
        dispose() {
            disposed = true;
            listeners.clear();
        },
    };
}
