import { useEffect } from "react";

const REFRESH_INTERVAL_MS = 60_000;

/**
 * Re-fetch when a view becomes active, when the tab is visible again, and
 * every 60 seconds while it stays active. Inactive views do not poll.
 */
export function useViewRefresh(active: boolean, refresh: () => void): void {
    useEffect(() => {
        if (!active) return;
        refresh();
    }, [active, refresh]);

    useEffect(() => {
        if (!active) return;
        const onVisibility = () => {
            if (document.visibilityState === "visible") refresh();
        };
        document.addEventListener("visibilitychange", onVisibility);
        const interval = window.setInterval(refresh, REFRESH_INTERVAL_MS);
        return () => {
            document.removeEventListener("visibilitychange", onVisibility);
            window.clearInterval(interval);
        };
    }, [active, refresh]);
}
