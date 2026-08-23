import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Shared state + synced-ref mirror: `current` re-renders on change while
 * `setCurrent` keeps the ref readable inside event handlers and async
 * completions without launching work from a setState updater. This replaces
 * the hand-rolled `useState` + `useRef` + `apply()` triple per concern.
 */
export function useSyncedRef<T>(initial: T) {
    const [value, setValue] = useState<T>(initial);
    const valueRef = useRef<T>(initial);
    const apply = useCallback((next: T) => {
        valueRef.current = next;
        setValue(next);
    }, []);
    return [value, valueRef, apply] as const;
}

/** Plain mutable ref with a setter, for non-render counters/guards. */
export function useRefValue<T>(initial: T) {
    const ref = useRef<T>(initial);
    return [ref, useCallback((next: T) => (ref.current = next), [])] as const;
}

const IDLE_CALLBACK: "requestIdleCallback" | "setTimeout" =
    typeof window !== "undefined" &&
    typeof window.requestIdleCallback === "function"
        ? "requestIdleCallback"
        : "setTimeout";

/**
 * Run `task` once the browser is idle — requestIdleCallback where available,
 * a 1s timeout otherwise. SSR-safe no-op. The timer is cleared on unmount if
 * it has not fired yet.
 */
export function useIdleEffect(task: () => void): void {
    useEffect(() => {
        let handle = 0;
        let cancelled = false;
        if (IDLE_CALLBACK === "requestIdleCallback") {
            handle = window.requestIdleCallback(() => {
                if (!cancelled) task();
            });
            return () => {
                cancelled = true;
                window.cancelIdleCallback(handle);
            };
        }
        handle = window.setTimeout(() => {
            if (!cancelled) task();
        }, 1000);
        return () => {
            cancelled = true;
            window.clearTimeout(handle);
        };
    });
}
