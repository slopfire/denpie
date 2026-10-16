import { useEffect, useRef, useSyncExternalStore } from "react";
import {
    readCacheEpoch,
    subscribeReadCacheInvalidation,
} from "@/lib/api-v1/cache";
import {
    createLiveResource,
    type LiveResource,
    type LiveState,
} from "@/lib/live-resource";
import { useViewRefresh } from "./use-view-refresh";

export interface LiveResourceHandle<T> {
    state: LiveState<T>;
    /** Refetch now; resolves once the store settled on the latest data. */
    refresh: () => Promise<void>;
    /** Apply a pure local update to the rendered data. */
    patch: (update: (data: T) => T) => void;
}

/**
 * Bind a `createLiveResource` store to a view. While `active` it refetches on
 * activation, tab focus, a 60 s timer, and whenever any mutation invalidated
 * the shared read cache, so a view can never show data older than the last
 * change the app itself made. Rendered data stays on screen throughout; only
 * a cold start shows `loading`.
 */
export function useLiveResource<T>(
    active: boolean,
    fetcher: () => Promise<T>,
    errorMessage: (error: unknown) => string,
): LiveResourceHandle<T> {
    const fetcherRef = useRef(fetcher);
    fetcherRef.current = fetcher;
    const errorMessageRef = useRef(errorMessage);
    errorMessageRef.current = errorMessage;

    const resourceRef = useRef<LiveResource<T> | null>(null);
    if (resourceRef.current === null) {
        resourceRef.current = createLiveResource<T>({
            fetch: () => fetcherRef.current(),
            errorMessage: (error) => errorMessageRef.current(error),
            epoch: readCacheEpoch,
        });
    }
    const resource = resourceRef.current;

    const state = useSyncExternalStore(
        resource.subscribe,
        resource.getState,
        resource.getState,
    );

    // Stable identity: useViewRefresh re-fires on every refresh identity change.
    const refreshRef = useRef(() => resource.refresh());
    useViewRefresh(active, refreshRef.current);

    useEffect(() => {
        if (!active) return;
        return subscribeReadCacheInvalidation(() => {
            void resource.refresh();
        });
    }, [active, resource]);

    const handleRef = useRef({
        refresh: refreshRef.current,
        patch: (update: (data: T) => T) => resource.patch(update),
    });
    return { state, ...handleRef.current };
}
