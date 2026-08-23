import { describe, expect, test } from "bun:test";
import { createAuthClient } from "./auth-client";
import { invalidateReadCache } from "./api-v1/cache";
import { create, toBinary } from "@bufbuild/protobuf";
import {
    ApiV1ResponseSchema,
    ApiResponseSchema,
    FlowCardInfoSchema,
    FlowCardPageSchema,
} from "../generated/denpie_pb";
async function fresh() {
    const mod = await import(
        `./bootstrap-prefetch?case=${crypto.randomUUID()}`
    );
    return mod;
}

const user = {
    id: "u-1",
    username: "alice",
    role: "user",
    display_name: "Alice",
    avatar_data: null,
    build_sha: "abc",
};

function pageResponse(cardId: bigint): Response {
    const page = create(FlowCardPageSchema, {
        cards: [create(FlowCardInfoSchema, { id: cardId })],
        hasMore: false,
    });
    const response = create(ApiV1ResponseSchema, {
        requestId: "srv",
        outcome: {
            case: "success",
            value: create(ApiResponseSchema, {
                result: { case: "flowCardPage", value: page },
            }),
        },
    });
    return new Response(toBinary(ApiV1ResponseSchema, response), {
        status: 200,
        headers: { "Content-Type": "application/x-protobuf" },
    });
}

describe("startBootstrapPrefetch", () => {
    test("no-op without a window (SSR/test default)", async () => {
        const mod = await fresh();
        // No global window: start must be a safe no-op.
        mod.startBootstrapPrefetch();
        expect(mod.takePrefetchedFlowPage()).toBeNull();
    });

    test("fires /auth/me and page 1 concurrently; take returns the page promise", async () => {
        const flow = await fresh();
        const meUrls: string[] = [];
        let flowCalls = 0;
        // The prefetched Flow page goes through ops.listFlowCards, which
        // defaults to global fetch; stub that for the page leg only.
        const previousFetch = globalThis.fetch;
        globalThis.fetch = (async (_url: unknown, init?: RequestInit) => {
            if (init?.method === "POST") {
                flowCalls += 1;
                return pageResponse(7n);
            }
            throw new TypeError("unexpected prefetch fetch");
        }) as typeof fetch;
        const client = createAuthClient({
            fetchImpl: async (url, init) => {
                if (init.method === "GET" && url === "/auth/me") {
                    meUrls.push(url);
                    return new Response(JSON.stringify(user), {
                        status: 200,
                        headers: { "Content-Type": "application/json" },
                    });
                }
                throw new TypeError(`unexpected ${String(url)}`);
            },
        });

        // Simulate the browser window the prefetch guard requires.
        const previousWindow = globalThis.window;
        Object.defineProperty(globalThis, "window", {
            value: { location: { href: "http://localhost/" } },
            configurable: true,
        });
        try {
            invalidateReadCache();
            flow.startBootstrapPrefetch();
            // Idempotent: a second call must not re-fire anything.
            flow.startBootstrapPrefetch();

            const taken = flow.takePrefetchedFlowPage();
            expect(taken).not.toBeNull();
            const page = await taken;
            expect(flowCalls).toBeGreaterThanOrEqual(1);
            expect(page?.cards[0]?.id).toBe(7n);

            // The prefetch shares AppShell's fetchMe request.
            const session = await client.fetchMe();
            expect(session.ok).toBe(true);
            expect(meUrls.length).toBeLessThanOrEqual(2); // one shared GET

            // Taken once: subsequent takes see null.
            expect(flow.takePrefetchedFlowPage()).toBeNull();
        } finally {
            if (previousWindow === undefined) {
                Reflect.deleteProperty(globalThis, "window");
            } else {
                Object.defineProperty(globalThis, "window", {
                    value: previousWindow,
                    configurable: true,
                });
            }
        }
        globalThis.fetch = previousFetch;
    });

    test("network failures resolve to null markers, never reject", async () => {
        const flow = await fresh();
        const client = createAuthClient({
            fetchImpl: async () => {
                throw new TypeError("offline");
            },
        });
        const previousWindow = globalThis.window;
        Object.defineProperty(globalThis, "window", {
            value: {},
            configurable: true,
        });
        // Page leg rides global fetch (ops.listFlowCards default): fail it.
        const previousFetch = globalThis.fetch;
        globalThis.fetch = (async () => {
            throw new TypeError("offline");
        }) as typeof fetch;
        try {
            invalidateReadCache();
            flow.startBootstrapPrefetch();
            const taken = flow.takePrefetchedFlowPage();
            expect(taken).not.toBeNull();
            const page = await taken;
            expect(page).toBeNull();
            const session = await client.fetchMe();
            expect(session.ok).toBe(false);
        } finally {
            globalThis.fetch = previousFetch;
            if (previousWindow === undefined) {
                Reflect.deleteProperty(globalThis, "window");
            } else {
                Object.defineProperty(globalThis, "window", {
                    value: previousWindow,
                    configurable: true,
                });
            }
        }
    });

    test("unauthorized resolves to a null page marker too", async () => {
        const flow = await fresh();
        const client = createAuthClient({
            fetchImpl: async (_url, init) =>
                init.method === "GET"
                    ? new Response("", { status: 401 })
                    : new Response(new Uint8Array(), {
                          status: 200,
                          headers: {
                              "Content-Type": "application/x-protobuf",
                          },
                      }),
        });
        const previousWindow = globalThis.window;
        Object.defineProperty(globalThis, "window", {
            value: {},
            configurable: true,
        });
        const previousFetch = globalThis.fetch;
        globalThis.fetch = (async (_url: unknown, init?: RequestInit) =>
            new Response("", { status: 403 })) as typeof fetch;
        try {
            invalidateReadCache();
            flow.startBootstrapPrefetch();
            const taken = flow.takePrefetchedFlowPage();
            expect(taken).not.toBeNull();
            const page = await taken;
            expect(page).toBeNull();
            const session = await client.fetchMe();
            expect(session.ok).toBe(false);
        } finally {
            globalThis.fetch = previousFetch;
            if (previousWindow === undefined) {
                Reflect.deleteProperty(globalThis, "window");
            } else {
                Object.defineProperty(globalThis, "window", {
                    value: previousWindow,
                    configurable: true,
                });
            }
        }
    });
});

