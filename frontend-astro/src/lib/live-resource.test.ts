import { describe, expect, test } from "bun:test";
import { createLiveResource, type LiveState } from "./live-resource";

/** A fetch whose resolution the test controls, one deferred per call. */
function controlledFetch<T>() {
    const pending: {
        resolve(value: T): void;
        reject(error: Error): void;
    }[] = [];
    return {
        fetch: () =>
            new Promise<T>((resolve, reject) => {
                pending.push({ resolve, reject });
            }),
        calls: () => pending.length,
        resolve: (index: number, value: T) => pending[index]!.resolve(value),
        reject: (index: number, error: Error) => pending[index]!.reject(error),
    };
}

const message = (error: unknown) =>
    error instanceof Error ? error.message : String(error);

async function tick() {
    await new Promise((resolve) => setTimeout(resolve, 0));
}

function dataOf<T>(state: LiveState<T>): T | undefined {
    return state.kind === "ready" ? state.data : undefined;
}

describe("createLiveResource", () => {
    test("starts loading, then ready with data", async () => {
        const source = controlledFetch<string>();
        const resource = createLiveResource({
            fetch: source.fetch,
            errorMessage: message,
        });
        expect(resource.getState()).toEqual({ kind: "loading" });
        const done = resource.refresh();
        source.resolve(0, "one");
        await done;
        expect(resource.getState()).toEqual({
            kind: "ready",
            data: "one",
            refreshing: false,
            error: null,
        });
    });

    test("a refresh with data never goes back to loading", async () => {
        const source = controlledFetch<string>();
        const resource = createLiveResource({
            fetch: source.fetch,
            errorMessage: message,
        });
        const first = resource.refresh();
        source.resolve(0, "one");
        await first;

        const kinds: string[] = [];
        resource.subscribe(() => kinds.push(resource.getState().kind));
        const second = resource.refresh();
        expect(resource.getState()).toMatchObject({
            kind: "ready",
            data: "one",
            refreshing: true,
        });
        source.resolve(1, "two");
        await second;
        expect(kinds.every((kind) => kind === "ready")).toBe(true);
        expect(dataOf(resource.getState())).toBe("two");
    });

    test("a failed refresh keeps the data and reports a soft error", async () => {
        const source = controlledFetch<string>();
        const resource = createLiveResource({
            fetch: source.fetch,
            errorMessage: message,
        });
        const first = resource.refresh();
        source.resolve(0, "one");
        await first;
        const second = resource.refresh();
        source.reject(1, new Error("offline"));
        await second;
        expect(resource.getState()).toEqual({
            kind: "ready",
            data: "one",
            refreshing: false,
            error: "offline",
        });
    });

    test("a failed cold start is an error, and retry shows loading again", async () => {
        const source = controlledFetch<string>();
        const resource = createLiveResource({
            fetch: source.fetch,
            errorMessage: message,
        });
        const first = resource.refresh();
        source.reject(0, new Error("down"));
        await first;
        expect(resource.getState()).toEqual({ kind: "error", message: "down" });
        const retry = resource.refresh();
        expect(resource.getState()).toEqual({ kind: "loading" });
        source.resolve(1, "back");
        await retry;
        expect(dataOf(resource.getState())).toBe("back");
    });

    test("a refresh requested mid-fetch queues exactly one rerun", async () => {
        const source = controlledFetch<string>();
        const resource = createLiveResource({
            fetch: source.fetch,
            errorMessage: message,
        });
        const first = resource.refresh();
        const second = resource.refresh();
        const third = resource.refresh();
        expect(source.calls()).toBe(1);
        source.resolve(0, "one");
        await tick();
        expect(source.calls()).toBe(2);
        source.resolve(1, "two");
        await Promise.all([first, second, third]);
        expect(source.calls()).toBe(2);
        expect(dataOf(resource.getState())).toBe("two");
    });

    test("a result fetched before the server data changed is discarded and refetched", async () => {
        const source = controlledFetch<string>();
        let epoch = 0;
        const resource = createLiveResource({
            fetch: source.fetch,
            errorMessage: message,
            epoch: () => epoch,
        });
        const done = resource.refresh();
        epoch += 1; // a mutation lands while the request is on the wire
        source.resolve(0, "predates-mutation");
        await tick();
        expect(resource.getState()).toEqual({ kind: "loading" });
        expect(source.calls()).toBe(2);
        source.resolve(1, "after-mutation");
        await done;
        expect(dataOf(resource.getState())).toBe("after-mutation");
    });

    test("a local patch supersedes the fetch in flight", async () => {
        const source = controlledFetch<string[]>();
        const resource = createLiveResource({
            fetch: source.fetch,
            errorMessage: message,
        });
        const first = resource.refresh();
        source.resolve(0, ["a"]);
        await first;

        const second = resource.refresh();
        resource.patch((data) => [...data, "local"]);
        expect(dataOf(resource.getState())).toEqual(["a", "local"]);
        // The older response must not clobber the edit...
        source.resolve(1, ["a"]);
        await tick();
        expect(dataOf(resource.getState())).toEqual(["a", "local"]);
        // ...the rerun's authoritative answer replaces it.
        source.resolve(2, ["a", "server"]);
        await second;
        expect(dataOf(resource.getState())).toEqual(["a", "server"]);
    });

    test("patch without data is a no-op", () => {
        const resource = createLiveResource<string[]>({
            fetch: () => Promise.resolve([]),
            errorMessage: message,
        });
        resource.patch((data) => [...data, "x"]);
        expect(resource.getState()).toEqual({ kind: "loading" });
    });

    test("a disposed store ignores late results", async () => {
        const source = controlledFetch<string>();
        const resource = createLiveResource({
            fetch: source.fetch,
            errorMessage: message,
        });
        const done = resource.refresh();
        resource.dispose();
        source.resolve(0, "late");
        await done;
        expect(resource.getState()).toEqual({ kind: "loading" });
    });
});
