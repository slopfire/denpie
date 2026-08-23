/**
 * In-app views that stay mounted after you leave them.
 *
 * Flow is the review surface: keeping it mounted preserves scroll, review
 * slots, and the view-transition snapshot. Archive (and the other routes)
 * unmount when you leave so card markdown and decoded images can be freed.
 */
export const PERSISTED_APP_VIEWS = ["flow"] as const;

/**
 * Next keep-alive set after navigating to `current`.
 *
 * `persist` views stay once visited; every other view is only present while
 * it is the current route.
 */
export function nextMountedViews<T extends string>(
    previous: ReadonlySet<T>,
    current: T,
    persist: readonly T[],
): Set<T> {
    const next = new Set<T>();
    for (const view of persist) {
        if (previous.has(view) || current === view) next.add(view);
    }
    next.add(current);
    return next;
}
