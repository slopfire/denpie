import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import {
    CardSourceSchema,
    FlowCardInfoSchema,
    type CardSource,
    type FlowCardInfo,
} from "../generated/denpie_pb";
import {
    closeCardDetail,
    detailFailed,
    detailSourceIcon,
    detailSources,
    detailSucceeded,
    humanDetailDate,
    INITIAL_FLOW_DETAIL_STATE,
    isUserFullscreenDismiss,
    loadingDetailRequest,
    openCardDetail,
    retryCardDetail,
    safeSourceUrl,
    toDetailSourceView,
    type DetailRequest,
    type FlowDetailState,
} from "./flow-detail-state";

function card(id: bigint, overrides: Partial<FlowCardInfo> = {}): FlowCardInfo {
    return create(FlowCardInfoSchema, {
        id,
        title: `card-${id}`,
        ...overrides,
    });
}

function source(overrides: Partial<CardSource> = {}): CardSource {
    return create(CardSourceSchema, { ...overrides });
}

function requireLoading(state: FlowDetailState): DetailRequest {
    const request = loadingDetailRequest(state);
    if (request === undefined)
        throw new TypeError("expected a loading request");
    return request;
}

describe("fullscreen dismiss reasons", () => {
    test("only explicit user dismissals close the overlay", () => {
        expect(isUserFullscreenDismiss("escape-key")).toBe(true);
        expect(isUserFullscreenDismiss("close-press")).toBe(true);
        expect(isUserFullscreenDismiss("outside-press")).toBe(true);
        expect(isUserFullscreenDismiss("close-watcher")).toBe(true);
        expect(isUserFullscreenDismiss("trigger-press")).toBe(true);
        expect(isUserFullscreenDismiss("imperative-action")).toBe(true);
        expect(isUserFullscreenDismiss("none")).toBe(false);
        expect(isUserFullscreenDismiss("focus-out")).toBe(false);
        expect(isUserFullscreenDismiss("item-press")).toBe(false);
        expect(isUserFullscreenDismiss("pointer")).toBe(false);
    });
});

describe("detail Sheet lifecycle", () => {
    test("opens one exact huge bigint card with a monotonic generation", () => {
        const huge = (1n << 120n) + 91n;
        const opened = openCardDetail(INITIAL_FLOW_DETAIL_STATE, huge);
        expect(opened).toEqual({
            kind: "loading",
            request: { cardId: huge, generation: 1 },
        });

        const next = openCardDetail(opened, 4n);
        expect(requireLoading(next)).toEqual({ cardId: 4n, generation: 2 });
    });

    test("commits a matching result only to the exact live request", () => {
        const loading = openCardDetail(INITIAL_FLOW_DETAIL_STATE, 7n);
        const request = requireLoading(loading);
        const ready = detailSucceeded(loading, request, card(7n));
        expect(ready).toEqual({ kind: "ready", request, card: card(7n) });

        const otherRequest = { cardId: 7n, generation: 2 };
        expect(detailSucceeded(loading, otherRequest, card(7n))).toBe(loading);
        expect(detailFailed(loading, otherRequest, "stale")).toBe(loading);
    });

    test("rejects a successful response carrying a different card ID", () => {
        const loading = openCardDetail(INITIAL_FLOW_DETAIL_STATE, 7n);
        const next = detailSucceeded(
            loading,
            requireLoading(loading),
            card(8n),
        );
        expect(next).toEqual({
            kind: "error",
            request: { cardId: 7n, generation: 1 },
            message: "The returned card did not match the requested card.",
        });
    });

    test("cross-card stale completions cannot replace a newer open Sheet", () => {
        const first = openCardDetail(INITIAL_FLOW_DETAIL_STATE, 7n);
        const firstRequest = requireLoading(first);
        const second = openCardDetail(first, 9n);
        const secondRequest = requireLoading(second);
        expect(detailSucceeded(second, firstRequest, card(7n))).toBe(second);
        expect(detailFailed(second, firstRequest, "old request")).toBe(second);
        expect(detailSucceeded(second, secondRequest, card(9n)).kind).toBe(
            "ready",
        );
    });

    test("close invalidates in-flight work and reopen claims another generation", () => {
        const loading = openCardDetail(INITIAL_FLOW_DETAIL_STATE, 7n);
        const request = requireLoading(loading);
        const closed = closeCardDetail(loading);
        expect(closed).toEqual({ kind: "closed", generation: 2 });
        expect(detailSucceeded(closed, request, card(7n))).toBe(closed);
        expect(detailFailed(closed, request, "late")).toBe(closed);

        const reopened = openCardDetail(closed, 7n);
        expect(requireLoading(reopened)).toEqual({ cardId: 7n, generation: 3 });
        expect(closeCardDetail(closed)).toBe(closed);
    });

    test("failure persists for the exact request and retry claims a fresh generation", () => {
        const loading = openCardDetail(INITIAL_FLOW_DETAIL_STATE, 7n);
        const request = requireLoading(loading);
        const error = detailFailed(loading, request, "offline");
        expect(error).toEqual({ kind: "error", request, message: "offline" });
        expect(retryCardDetail(error)).toEqual({
            kind: "loading",
            request: { cardId: 7n, generation: 2 },
        });
        expect(retryCardDetail(loading)).toBe(loading);
    });
});

describe("detail source projection", () => {
    test("uses title, then a safe canonical URL, then an explicit fallback", () => {
        expect(
            toDetailSourceView(
                source({ title: " Rust book ", url: "https://example.com" }),
            ),
        ).toEqual({
            label: "Rust book",
            href: "https://example.com/",
            icon: "unknown",
        });
        expect(
            toDetailSourceView(source({ url: "https://example.com/read" })),
        ).toEqual({
            label: "https://example.com/read",
            href: "https://example.com/read",
            icon: "unknown",
        });
        expect(toDetailSourceView(source())).toEqual({
            label: "Untitled source",
            href: null,
            icon: "unknown",
        });
    });

    test("allows only absolute HTTP(S) links without credentials", () => {
        expect(safeSourceUrl("http://example.com/a")).toBe(
            "http://example.com/a",
        );
        expect(safeSourceUrl("https://example.com/a")).toBe(
            "https://example.com/a",
        );
        expect(safeSourceUrl("/relative")).toBeNull();
        expect(safeSourceUrl("javascript:alert(1)")).toBeNull();
        expect(safeSourceUrl("data:text/plain,unsafe")).toBeNull();
        expect(safeSourceUrl("ftp://example.com/file")).toBeNull();
        expect(safeSourceUrl("https://user@example.com/private")).toBeNull();
        expect(
            safeSourceUrl("https://user:pass@example.com/private"),
        ).toBeNull();
    });

    test("maps only known source types to a specific icon", () => {
        expect(detailSourceIcon(source({ sourceType: "link" }))).toBe("link");
        expect(detailSourceIcon(source({ sourceType: "document" }))).toBe(
            "document",
        );
        expect(detailSourceIcon(source({ sourceType: "import" }))).toBe(
            "unknown",
        );
    });

    test("projects generated card sources without mutating the card", () => {
        const generated = card(1n, {
            sources: [
                source({
                    title: "Guide",
                    sourceType: "link",
                    url: "https://example.com",
                }),
                source({ sourceType: "document" }),
            ],
        });
        expect(detailSources(generated)).toEqual([
            { label: "Guide", href: "https://example.com/", icon: "link" },
            { label: "Untitled source", href: null, icon: "document" },
        ]);
        expect(generated.sources).toHaveLength(2);
    });

    test("formats valid created dates and omits invalid or blank values", () => {
        expect(humanDetailDate("2026-08-23T00:00:00Z")).toBe("Aug 23, 2026");
        expect(humanDetailDate("")).toBeNull();
        expect(humanDetailDate("not-a-date")).toBeNull();
    });
});
