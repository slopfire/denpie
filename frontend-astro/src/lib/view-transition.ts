/**
 * View Transitions API wrapper for the in-app route swaps in `AppShell`.
 *
 * `AppShell` keeps Flow mounted after the first visit and unmounts the other
 * routes when you leave, so Astro's `<ClientRouter />` never runs: navigation
 * happens through the History API inside a single React island. This module
 * animates those swaps by starting
 * `document.startViewTransition` and committing the React state update with
 * `flushSync` inside its update callback (the documented flushSync +
 * startViewTransition interop pattern), falling back to a direct commit when
 * the API is unavailable or reduced motion is requested.
 */

import { flushSync } from "react-dom";

/** True when the browser can run view transitions right now. */
export function supportsViewTransitions(): boolean {
    return typeof document !== "undefined"
        && typeof document.startViewTransition === "function";
}

/** True when the user prefers reduced motion. */
export function prefersReducedMotion(): boolean {
    return typeof window !== "undefined"
        && typeof window.matchMedia === "function"
        && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Run `commit()` synchronously inside a view transition.
 *
 * Falls back to calling `commit()` directly when view transitions are
 * unsupported or reduced motion is requested. This is the single entry
 * point for animated navigation; callers never branch on support.
 */
export function runViewTransition(commit: () => void): void {
    if (!supportsViewTransitions() || prefersReducedMotion()) {
        commit();
        return;
    }
    document.startViewTransition(() => {
        flushSync(commit);
    });
}
