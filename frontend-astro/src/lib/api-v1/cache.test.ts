import { describe, expect, test } from "bun:test";
import {
    invalidateReadCache,
    readCacheKey,
    withReadCache,
    READ_CACHE_DEFAULT_TTL_MS,
} from "./cache";

/** Fresh cache.ts instance per test: the query string busts Bun's module cache. */
async function freshCache() {
    return import(`./cache?case=${crypto.randomUUID()}`);
}

describe("readCacheKey", () => {
    test("op case and params join with colons", () => {
        expect(readCacheKey("listFlowCards", 48, "")).toBe("listFlowCards:48:");
        expect(readCacheKey("getTipcard", "42")).toBe("getTipcard:42");
    });
});

describe("withReadCache (default instance)", () => {
    test("default TTL constant is 30s", () => {
        expect(READ_CACHE_DEFAULT_TTL_MS).toBe(30_000);
    });

    test("second call within TTL reuses the cached value", async () => {
        const { withReadCache: withCache } = await freshCache();
        let runs = 0;
        const producer = async () => {
            runs += 1;
            return "value";
        };
        expect(await withCache("k", 1000, producer)).toBe("value");
        expect(await withCache("k", 1000, producer)).toBe("value");
        expect(runs).toBe(1);
    });

    test("concurrent calls share one in-flight promise", async () => {
        const { withReadCache: withCache } = await freshCache();
        let runs = 0;
        const producer = async () => {
            runs += 1;
            await new Promise((resolve) => setTimeout(resolve, 5));
            return runs;
        };
        const [a, b] = await Promise.all([
            withCache("k", 1000, producer),
            withCache("k", 1000, producer),
        ]);
        expect(a).toBe(1);
        expect(b).toBe(1);
        expect(runs).toBe(1);
    });

    test("expired entry is refetched", async () => {
        const { withReadCache: withCache } = await freshCache();
        let runs = 0;
        const producer = async () => {
            runs += 1;
            return runs;
        };
        expect(await withCache("k", 1, producer)).toBe(1);
        await new Promise((resolve) => setTimeout(resolve, 10));
        expect(await withCache("k", 1, producer)).toBe(2);
    });

    test("producer failure evicts the key and rethrows untouched", async () => {
        const { withReadCache: withCache } = await freshCache();
        class Boom extends Error {}
        const boom = new Boom("nope");
        let runs = 0;
        const failing = async (): Promise<never> => {
            runs += 1;
            throw boom;
        };
        try {
            await withCache("k", 1000, failing);
            throw new Error("unreachable");
        } catch (error) {
            expect(error).toBe(boom);
        }
        // Key evicted: next call invokes the producer again.
        const ok = async () => {
            runs += 1;
            return "recovered";
        };
        expect(await withCache("k", 1000, ok)).toBe("recovered");
        expect(runs).toBe(2);
    });

    test("failure does not poison a concurrently shared promise result", async () => {
        const { withReadCache: withCache } = await freshCache();
        const pending = withCache("k", 1000, async (): Promise<never> => {
            throw new TypeError("first fails");
        });
        await pending.catch(() => "handled");
        const second = await withCache("k", 1000, async () => "second");
        expect(second).toBe("second");
    });

    test("invalidateReadCache drops everything without a prefix", async () => {
        const mod = await freshCache();
        const { withReadCache: withCache } = mod;
        let runs = 0;
        const producer = async () => {
            runs += 1;
            return runs;
        };
        await withCache("a", 60_000, producer);
        await withCache("b", 60_000, producer);
        mod.invalidateReadCache();
        expect(await withCache("a", 60_000, producer)).toBe(3);
        expect(await withCache("b", 60_000, producer)).toBe(4);
    });

    test("invalidateReadCache with prefix keeps other keys", async () => {
        const mod = await freshCache();
        const { withReadCache: withCache } = mod;
        // Distinct sentinel per key so assertions do not depend on run order.
        let flowRuns = 0;
        let infoRuns = 0;
        const flowProducer = async () => {
            flowRuns += 1;
            return `flow-${flowRuns}`;
        };
        const infoProducer = async () => {
            infoRuns += 1;
            return `info-${infoRuns}`;
        };
        expect(await withCache("listFlowCards:48:", 60_000, flowProducer)).toBe(
            "flow-1",
        );
        expect(await withCache("getApiInfo", 60_000, infoProducer)).toBe(
            "info-1",
        );
        mod.invalidateReadCache("listFlowCards:");
        // "getApiInfo" was untouched: served from cache without a producer run.
        expect(await withCache("getApiInfo", 60_000, infoProducer)).toBe(
            "info-1",
        );
        expect(infoRuns).toBe(1);
        // "listFlowCards:48:" was dropped: the producer runs again.
        expect(await withCache("listFlowCards:48:", 60_000, flowProducer)).toBe(
            "flow-2",
        );
    });
});
