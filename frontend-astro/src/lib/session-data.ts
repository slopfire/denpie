// Per-session client data. Everything the UI caches about the signed-in user
// (read cache, Flow snapshot, bootstrap prefetch) is keyed by nothing but the
// browser tab, so it must be dropped at every session boundary or the next
// account would briefly render the previous account's cards.

import { invalidateReadCache } from "./api-v1/cache";
import { takePrefetchedFlowPage } from "./bootstrap-prefetch";
import { FLOW_SNAPSHOT_KEY } from "./flow-snapshot";

/** Forget all user-scoped client data; call on login, logout, and deletion. */
export function clearSessionScopedData(): void {
    invalidateReadCache();
    // Taking the prefetched page discards it.
    void takePrefetchedFlowPage();
    if (typeof window === "undefined") return;
    try {
        window.sessionStorage.removeItem(FLOW_SNAPSHOT_KEY);
    } catch {
        // Storage can be unavailable (private mode); the snapshot is best-effort.
    }
}
