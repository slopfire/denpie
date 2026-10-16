/**
 * Read-through TTL cache for `/api/v1` read operations.
 *
 * Module-level Map keyed by op case + canonical params. Producers run at most
 * once per key until the entry expires or a mutation invalidates the cache.
 * Cache failures never surface: on producer throw the key is evicted and the
 * error rethrown untouched.
 */

export const READ_CACHE_DEFAULT_TTL_MS = 30_000;

type CachedValue =
    | { state: "pending"; promise: Promise<unknown> }
    | { state: "done"; value: unknown };

interface CacheEntry {
    cached: CachedValue;
    /** Epoch milliseconds after which the entry is stale. */
    expiresAt: number;
}

const cache = new Map<string, CacheEntry>();

/**
 * Monotonic data epoch. Every invalidation advances it, so a reader that
 * started before a mutation can tell its result is already stale, and views
 * can subscribe to "server data changed" instead of polling.
 */
let epoch = 0;
const listeners = new Set<() => void>();

/** Current data epoch; changes exactly when cached reads were invalidated. */
export function readCacheEpoch(): number {
    return epoch;
}

/**
 * Subscribe to invalidations. The listener runs after the cache is cleared and
 * the epoch advanced; it returns an unsubscribe function.
 */
export function subscribeReadCacheInvalidation(
    listener: () => void,
): () => void {
    listeners.add(listener);
    return () => {
        listeners.delete(listener);
    };
}

/** Canonical cache key: op case plus its identifying parameters. */
export function readCacheKey(
    op: string,
    ...params: readonly (string | number)[]
): string {
    return [op, ...params].join(":");
}

/**
 * Resolve `key` through the TTL cache. A fresh hit skips `producer`; an
 * in-flight call shares the same promise; anything else invokes `producer`
 * exactly once and caches its success for `ttlMs`. On producer failure the
 * key is evicted and the error propagates unchanged.
 */
export function withReadCache<T>(
    key: string,
    ttlMs: number = READ_CACHE_DEFAULT_TTL_MS,
    producer: () => Promise<T>,
): Promise<T> {
    const hit = cache.get(key);
    if (hit !== undefined && hit.expiresAt > Date.now()) {
        // Fresh entry: settled value or the shared in-flight promise.
        return hit.cached.state === "done"
            ? Promise.resolve(hit.cached.value as T)
            : (hit.cached.promise as Promise<T>);
    }
    cache.delete(key);
    const startedAtEpoch = epoch;
    const pending: Promise<T> = producer().then(
        (value) => {
            // A mutation that landed while this read was in flight made the
            // value stale on arrival: hand it to its own callers but never
            // store it, or it would be served for a full TTL.
            if (startedAtEpoch === epoch) {
                cache.set(key, {
                    cached: { state: "done", value },
                    expiresAt: Date.now() + ttlMs,
                });
            }
            return value;
        },
        (error: unknown) => {
            // Cache failures never surface as stale data: evict and rethrow.
            const current = cache.get(key);
            if (
                current !== undefined &&
                current.cached.state === "pending" &&
                current.cached.promise === pending
            ) {
                cache.delete(key);
            }
            throw error;
        },
    );
    cache.set(key, {
        cached: { state: "pending", promise: pending },
        expiresAt: Number.MAX_SAFE_INTEGER,
    });
    return pending;
}

/**
 * Drop cached reads. With `prefix`, only keys starting with it are dropped
 * (e.g. `"listFlowCards:"`); without, the whole cache is cleared.
 */
export function invalidateReadCache(prefix?: string): void {
    if (prefix === undefined) {
        cache.clear();
    } else {
        for (const key of cache.keys()) {
            if (key.startsWith(prefix)) cache.delete(key);
        }
    }
    epoch += 1;
    for (const listener of listeners) listener();
}
