// Flow pager hook: owns the whole `FlowState` lifecycle — cold start via the
// bootstrap prefetch, silent background refresh of page 1, load-more
// pagination, and snapshot seeding/revalidation. Behavior notes:
//
// - Cold start (no rendered data) still renders the loading screen and can
//   end in the full-page error state.
// - Any refresh WITH rendered data fetches page 1 in the background and
//   merges through `mergeFlowRefresh` without leaving ready/loading-more/
//   load-error state: no skeleton flash, no scroll reset. A failed silent
//   refresh is swallowed (current slots stay).
// - A refresh requested while another fetch is in flight is queued, not
//   dropped, and a result fetched before a mutation landed (the read-cache
//   epoch moved) is discarded and refetched, so it can never overwrite newer
//   local state such as a just-reviewed slot.
// - The first ready render saves a session snapshot; every successful merge
//   refreshes it (an empty grid clears it).

import { useCallback, useEffect, useRef } from "react";
import { listFlowCards } from "@/lib/api-v1/ops";
import { readCacheEpoch } from "@/lib/api-v1/cache";
import type { FlowCardsPage } from "@/lib/api-v1/ops";
import { takePrefetchedFlowPage } from "@/lib/bootstrap-prefetch";
import {
    clearFlowSnapshot,
    loadFlowSnapshot,
    type SavedFlowPage,
} from "@/lib/flow-snapshot";
import { appendIdleSlots, slotsFromCards } from "@/lib/flow-review-actions";
import type { ReviewSlot } from "@/lib/flow-review-state";
import { mergeFlowRefresh } from "@/lib/flow-refresh-merge";
import type { FlowCursor } from "@/lib/flow-state";

/** Default page size; matches the bootstrap prefetch's request. */
export const FLOW_PAGE_SIZE = 48;
/**
 * `revalidating` marks a ready grid seeded from the session snapshot: cards
 * render immediately (with a subtle stale indicator) while page 1 refetches
 * in the background. It never survives a completed refresh.
 */
export type FlowState =
    | { kind: "initial-loading" }
    | {
          kind: "ready";
          slots: ReviewSlot[];
          cursor: FlowCursor;
          /** True while a snapshot-seeded grid awaits its first revalidation. */
          revalidating?: boolean;
      }
    | {
          kind: "loading-more";
          slots: ReviewSlot[];
          pageToken: string;
      }
    | {
          kind: "load-error";
          slots: ReviewSlot[];
          pageToken: string;
          message: string;
      }
    | { kind: "empty" }
    | { kind: "error"; message: string };

function slotsStateOf(
    state: FlowState,
): Extract<FlowState, { kind: "ready" | "loading-more" | "load-error" }> | null {
    return state.kind === "ready" ||
        state.kind === "loading-more" ||
        state.kind === "load-error"
        ? state
        : null;
}

export interface FlowPagerOptions {
    /** Synced-ref getter for the latest committed state. */
    getState(): FlowState;
    /** Commit the next state (syncs the component's ref mirror). */
    apply(next: FlowState): void;
    /** Set only the component's React state (no ref write). */
    setStateOnly(next: FlowState): void;
    /** True while any pin/delete/add mutation owns a card. */
    mutationsInFlight(): boolean;
    /** Persist the page-1-equivalent snapshot after each fresh first page. */
    saveSnapshot(page: SavedFlowPage): void;
}

export interface FlowPager {
    loadInitial(): Promise<void>;
    loadMoreFrom(slots: ReviewSlot[], pageToken: string): void;
}

/**
 * The Flow pager hook. Owns generation/in-flight guards for initial load,
 * silent refresh, and pagination; the component supplies its `apply` so
 * every commit stays atomic with the ref mirror.
 */
export function useFlowPager(options: FlowPagerOptions): FlowPager {
    const { getState, apply, setStateOnly, mutationsInFlight, saveSnapshot } =
        options;
    const generationRef = useRef(0);
    const inFlightRef = useRef(false);
    const mountedRef = useRef(true);
    // A refresh was requested (or went stale) while the pager was busy.
    const rerunRef = useRef(false);
    const loadInitialRef = useRef<() => Promise<void>>(async () => undefined);

    useEffect(() => {
        mountedRef.current = true;
        return () => {
            mountedRef.current = false;
        };
    }, []);

    const saveReadySnapshot = useCallback(
        (slots: readonly ReviewSlot[], cursor: FlowCursor) => {
            // Snapshot only the page-1-equivalent prefix; cards beyond the
            // first page are not part of a cold-start seed.
            const cards = slots
                .flatMap((slot) =>
                    slot.kind === "idle" ||
                    slot.kind === "reviewing" ||
                    slot.kind === "error"
                        ? [slot.card]
                        : [],
                )
                .slice(0, FLOW_PAGE_SIZE);
            if (cards.length === 0) {
                // Nothing to seed from; a stale snapshot would resurrect
                // cards that no longer exist on the next cold start.
                clearFlowSnapshot();
                return;
            }
            saveSnapshot({ savedAt: Date.now(), cards, cursor });
        },
        [saveSnapshot],
    );

    /** Run a queued refresh once the pager is idle again. */
    const flushQueuedRefresh = useCallback(() => {
        if (!rerunRef.current || !mountedRef.current) return;
        rerunRef.current = false;
        void loadInitialRef.current();
    }, []);

    /**
     * Integrate one successfully fetched first page. With rendered data it
     * reconciles silently (no skeleton, no scroll reset); from a cold start it
     * becomes the ready or empty result.
     */
    const integrateFreshPage = useCallback(
        (page: FlowCardsPage): void => {
            const current = getState();
            const slotsState = slotsStateOf(current);
            if (slotsState === null) {
                if (page.cards.length === 0) {
                    if (current.kind !== "empty") apply({ kind: "empty" });
                    return;
                }
                const slots = slotsFromCards(page.cards);
                apply({ kind: "ready", slots, cursor: page.cursor });
                saveReadySnapshot(slots, page.cursor);
                return;
            }
            const previousCursor: FlowCursor =
                slotsState.kind === "ready"
                    ? slotsState.cursor
                    : { kind: "more", pageToken: slotsState.pageToken };
            const merged = mergeFlowRefresh({
                currentSlots: slotsState.slots,
                freshPageCards: page.cards,
                cursor: page.cursor,
                previousCursor,
                mutationsInFlight: mutationsInFlight(),
            });
            if (merged.slots.length === 0) {
                apply({ kind: "empty" });
                saveReadySnapshot(merged.slots, merged.cursor);
                return;
            }
            const unchanged =
                merged.slots === slotsState.slots &&
                slotsState.kind === "ready" &&
                slotsState.revalidating !== true &&
                sameCursor(merged.cursor, slotsState.cursor);
            if (!unchanged) {
                apply({
                    kind: "ready",
                    slots:
                        merged.slots === slotsState.slots
                            ? slotsState.slots
                            : [...merged.slots],
                    cursor: merged.cursor,
                });
            }
            saveReadySnapshot(merged.slots, merged.cursor);
        },
        [apply, getState, mutationsInFlight, saveReadySnapshot],
    );

    const fetchFirstPage = useCallback(async (): Promise<FlowCardsPage> => {
        const taken = takePrefetchedFlowPage();
        if (taken !== null) {
            const page = await taken;
            // Prefetch failures resolve to null; fall back to a direct call.
            if (page !== null) return page;
            return await listFlowCards({ pageSize: FLOW_PAGE_SIZE });
        }
        return await listFlowCards({ pageSize: FLOW_PAGE_SIZE });
    }, []);

    const loadInitial = useCallback(async (): Promise<void> => {
        if (inFlightRef.current) {
            // Dropping this request would lose the change that triggered it.
            rerunRef.current = true;
            return;
        }
        inFlightRef.current = true;
        const generation = ++generationRef.current;
        const startedEpoch = readCacheEpoch();
        try {
            const page = await fetchFirstPage();
            if (!mountedRef.current || generationRef.current !== generation)
                return;
            if (readCacheEpoch() !== startedEpoch) {
                // A mutation landed mid-flight: this page may predate it.
                rerunRef.current = true;
                return;
            }
            integrateFreshPage(page);
        } catch (error) {
            if (!mountedRef.current || generationRef.current !== generation)
                return;
            // Only a true cold start may unmount to the full-page error.
            // With rendered data the failed silent refresh stays silent.
            if (getState().kind === "initial-loading") {
                apply({
                    kind: "error",
                    message:
                        error instanceof Error
                            ? error.message
                            : String(error),
                });
            }
        } finally {
            if (generationRef.current === generation) {
                inFlightRef.current = false;
                flushQueuedRefresh();
            }
        }
    }, [apply, fetchFirstPage, flushQueuedRefresh, getState, integrateFreshPage]);
    loadInitialRef.current = loadInitial;

    const loadMoreFrom = useCallback(
        (slots: ReviewSlot[], pageToken: string): void => {
            if (inFlightRef.current) return;
            inFlightRef.current = true;
            const generation = ++generationRef.current;
            // Rendered slots stay on screen; only the footer changes.
            setStateOnly({ kind: "loading-more", slots, pageToken });
            const startedEpoch = readCacheEpoch();
            void (async () => {
                try {
                    const page = await listFlowCards({
                        pageSize: FLOW_PAGE_SIZE,
                        pageToken,
                    });
                    if (
                        !mountedRef.current ||
                        generationRef.current !== generation
                    )
                        return;
                    const current = getState();
                    if (
                        current.kind !== "loading-more" ||
                        current.pageToken !== pageToken
                    )
                        return;
                    const nextSlots = appendIdleSlots(current.slots, page.cards);
                    apply({
                        kind: "ready",
                        slots: nextSlots,
                        cursor: page.cursor,
                    });
                    saveReadySnapshot(nextSlots.slice(0, FLOW_PAGE_SIZE), page.cursor);
                } catch (error) {
                    // Recoverable: keep the rendered slots and the same cursor
                    // so a retry re-requests exactly the failed page.
                    if (
                        !mountedRef.current ||
                        generationRef.current !== generation
                    )
                        return;
                    const current = getState();
                    if (
                        current.kind !== "loading-more" ||
                        current.pageToken !== pageToken
                    )
                        return;
                    apply({
                        kind: "load-error",
                        slots: current.slots,
                        pageToken,
                        message:
                            error instanceof Error
                                ? error.message
                                : String(error),
                    });
                } finally {
                    if (generationRef.current === generation) {
                        inFlightRef.current = false;
                        // The page may predate a mutation; reconcile page 1.
                        if (readCacheEpoch() !== startedEpoch)
                            rerunRef.current = true;
                        flushQueuedRefresh();
                    }
                }
            })();
        },
        [apply, flushQueuedRefresh, getState, saveReadySnapshot, setStateOnly],
    );

    return { loadInitial, loadMoreFrom };
}

function sameCursor(a: FlowCursor, b: FlowCursor): boolean {
    return (
        a.kind === b.kind &&
        (a.kind === "end" ||
            (b.kind === "more" && a.pageToken === b.pageToken))
    );
}

/**
 * Seed the initial state from the session snapshot when present: cards render
 * immediately in a `revalidating` ready grid instead of skeletons. Returns
 * `null` for a true cold start.
 */
export function seedFromSnapshot(): FlowState | null {
    const saved = loadFlowSnapshot();
    if (saved === null || saved.cards.length === 0) return null;
    return {
        kind: "ready",
        slots: slotsFromCards(saved.cards),
        cursor: saved.cursor,
        revalidating: true,
    };
}
