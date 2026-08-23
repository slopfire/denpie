/**
 * Dual bootstrap prefetch for the authenticated app shell.
 *
 * `startBootstrapPrefetch()` fires GET /auth/me and the first Flow page
 * concurrently, at the same moment AppShell's mount-time session refresh
 * runs — so by the time Flow renders, its page-1 request is already in
 * flight. The consumer contract (FlowCore) is `takePrefetchedFlowPage()`.
 *
 * Call-site note: auth-client is not a mount point, and AppShell must stay
 * untouched here. Instead, the first `fetchMe()` invocation inside
 * `createAuthClient()` triggers the prefetch lazily. AppShell's existing
 * mount effect (`void refresh()`) is that first invocation, so wiring needs
 * zero edits outside this file pair; the fetchMe dedupe keeps it race-free
 * when several clients exist.
 */
import type { AuthResult } from "./auth-client";

/** Result of the prefetched /auth/me call; null marker on any failure. */
export type PrefetchedMe = AuthResult | null;

import { defaultAuthClient } from "./auth-client";
import { listFlowCards, type FlowCardsPage } from "./api-v1/ops";

let mePromise: Promise<PrefetchedMe> | null = null;
let pagePromise: Promise<FlowCardsPage | null> | null = null;
let started = false;

/**
 * Fire the dual prefetch exactly once per document. Idempotent: later calls
 * reuse the stored promises. Unauthorized or network failures resolve to
 * null markers instead of rejecting, so nothing here produces an unhandled
 * rejection. No-op outside a browser window (SSR/tests).
 */
export function startBootstrapPrefetch(): void {
    if (started || typeof window === "undefined") {
        return;
    }
    started = true;
    mePromise = defaultAuthClient.fetchMe().catch(() => null);
    // Page 1 with the shell's default size; failures become a null marker.
    pagePromise = listFlowCards().catch(() => null);
    // Detached consumers keep both promises "handled" from birth.
    void mePromise.then(() => undefined);
    void pagePromise.then(() => undefined);
}

/**
 * Take the prefetched first Flow page promise, or `null` when no prefetch
 * is pending (not started, already taken, or window-less). The promise
 * resolves to `null` when the prefetch failed; callers fall back to a
 * normal `listFlowCards` request.
 */
export function takePrefetchedFlowPage(): Promise<FlowCardsPage | null> | null {
    const taken = pagePromise;
    pagePromise = null;
    return taken;
}
