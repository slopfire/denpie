import { useEffect, useRef, useState, type RefObject } from "react";
import { ARCHIVE_VIEWPORT_ROOT_MARGIN } from "@/lib/pages/archive";

/**
 * True while `ref` intersects the viewport plus overscan. Off-screen cards
 * report the last measured height so un-hydrating them does not jump scroll.
 */
export function useNearViewport(initial: boolean): {
    ref: RefObject<HTMLDivElement | null>;
    near: boolean;
    minHeight: number | undefined;
} {
    const ref = useRef<HTMLDivElement>(null);
    const [near, setNear] = useState(initial);
    const [minHeight, setMinHeight] = useState<number>();

    useEffect(() => {
        const element = ref.current;
        if (element === null) return;
        if (typeof IntersectionObserver !== "function") {
            setNear(true);
            return;
        }
        const observer = new IntersectionObserver(
            ([entry]) => {
                if (entry === undefined) return;
                if (entry.isIntersecting) {
                    setNear(true);
                    return;
                }
                const height = element.getBoundingClientRect().height;
                if (height > 0) setMinHeight(height);
                setNear(false);
            },
            {
                root: null,
                rootMargin: ARCHIVE_VIEWPORT_ROOT_MARGIN,
                threshold: 0,
            },
        );
        observer.observe(element);
        return () => observer.disconnect();
    }, []);

    return { ref, near, minHeight };
}
