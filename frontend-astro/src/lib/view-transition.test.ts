import { describe, expect, test } from "bun:test";
import {
    prefersReducedMotion,
    runViewTransition,
    supportsViewTransitions,
} from "./view-transition";

type BrowserStub = {
    document: Pick<Document, "startViewTransition">;
    window?: { matchMedia: (query: string) => { matches: boolean } };
};

function withBrowserStub(
    stub: BrowserStub | null,
    run: () => void,
): void {
    const g = globalThis as {
        document: unknown;
        window: unknown;
    };
    const previousDocument = g.document;
    const previousWindow = g.window;
    g.document = stub?.document ?? {};
    g.window = stub?.window;
    try {
        run();
    } finally {
        g.document = previousDocument;
        g.window = previousWindow;
    }
}

describe("view-transition", () => {
    test("reports support only when the API exists", () => {
        withBrowserStub(
            {
                document: {
                    startViewTransition: () => ({
                        finished: Promise.resolve(),
                    }),
                },
                window: { matchMedia: () => ({ matches: false }) },
            },
            () => {
                expect(supportsViewTransitions()).toBe(true);
                expect(prefersReducedMotion()).toBe(false);
            },
        );

        withBrowserStub(null, () => {
            expect(supportsViewTransitions()).toBe(false);
            expect(prefersReducedMotion()).toBe(false);
        });
    });

    test("commits synchronously inside startViewTransition", () => {
        const events: string[] = [];
        let pendingUpdate: (() => void) | undefined;
        withBrowserStub(
            {
                document: {
                    startViewTransition: (update) => {
                        events.push("start");
                        pendingUpdate = update;
                        return { finished: Promise.resolve() };
                    },
                },
                window: { matchMedia: () => ({ matches: false }) },
            },
            () => {
                runViewTransition(() => {
                    events.push("commit");
                });
                // The callback must be handed to startViewTransition, not
                // invoked directly by this module.
                expect(events).toEqual(["start"]);
            },
        );
        pendingUpdate?.();
        expect(events).toEqual(["start", "commit"]);
    });

    test("falls back to a direct commit without the API", () => {
        let committed = false;
        withBrowserStub(null, () => {
            runViewTransition(() => {
                committed = true;
            });
        });
        expect(committed).toBe(true);
    });

    test("skips animation under prefers-reduced-motion", () => {
        let started = false;
        let committed = false;
        withBrowserStub(
            {
                document: {
                    startViewTransition: () => {
                        started = true;
                        return { finished: Promise.resolve() };
                    },
                },
                window: {
                    matchMedia: (query) => ({
                        matches:
                            query === "(prefers-reduced-motion: reduce)",
                    }),
                },
            },
            () => {
                expect(prefersReducedMotion()).toBe(true);
                runViewTransition(() => {
                    committed = true;
                });
            },
        );
        expect(started).toBe(false);
        expect(committed).toBe(true);
    });
});
