// Pure detail-sheet state and source projections. The component owns network
// work; these transitions make a response committable only by its exact card
// identity and request generation.

import type { CardSource, FlowCardInfo } from "../generated/denpie_pb";

/** A single detail fetch, owned by the exact generated card ID. */
export interface DetailRequest {
    cardId: bigint;
    generation: number;
}

/** One controlled card-detail Sheet lifecycle. */
export type FlowDetailState =
    | { kind: "closed"; generation: number }
    | { kind: "loading"; request: DetailRequest }
    | { kind: "ready"; request: DetailRequest; card: FlowCardInfo }
    | { kind: "error"; request: DetailRequest; message: string };

/** The closed sheet starts at generation zero. */
export const INITIAL_FLOW_DETAIL_STATE: FlowDetailState = {
    kind: "closed",
    generation: 0,
};

function generationOf(state: FlowDetailState): number {
    return state.kind === "closed"
        ? state.generation
        : state.request.generation;
}

function sameRequest(left: DetailRequest, right: DetailRequest): boolean {
    return left.cardId === right.cardId && left.generation === right.generation;
}

function nextRequest(state: FlowDetailState, cardId: bigint): DetailRequest {
    return { cardId, generation: generationOf(state) + 1 };
}

/** Open one card's Sheet and claim a newer request generation. */
export function openCardDetail(
    state: FlowDetailState,
    cardId: bigint,
): FlowDetailState {
    return { kind: "loading", request: nextRequest(state, cardId) };
}

/**
 * Close the Sheet. Closing a live request increments the generation so its
 * eventual completion cannot reopen the Sheet.
 */
export function closeCardDetail(state: FlowDetailState): FlowDetailState {
    return state.kind === "closed"
        ? state
        : { kind: "closed", generation: generationOf(state) + 1 };
}

const USER_FULLSCREEN_DISMISS = new Set([
    "escape-key",
    "close-press",
    "outside-press",
    "close-watcher",
    "trigger-press",
    "imperative-action",
]);

/**
 * True when a Dialog `onOpenChange(false)` came from an explicit dismiss
 * (Escape, close button, overlay, or the fullscreen trigger). Review,
 * Continue, pin, and other in-overlay controls must not close it.
 */
export function isUserFullscreenDismiss(reason: string): boolean {
    return USER_FULLSCREEN_DISMISS.has(reason);
}

/** Retry only an exact visible error, with a newly claimed generation. */
export function retryCardDetail(state: FlowDetailState): FlowDetailState {
    return state.kind === "error"
        ? { kind: "loading", request: nextRequest(state, state.request.cardId) }
        : state;
}

/** The request owned by a loading Sheet, if it has one. */
export function loadingDetailRequest(
    state: FlowDetailState,
): DetailRequest | undefined {
    return state.kind === "loading" ? state.request : undefined;
}

/**
 * Commit a response only to its still-current request. A response for another
 * card is a protocol failure, not a detail view for the wrong card.
 */
export function detailSucceeded(
    state: FlowDetailState,
    request: DetailRequest,
    card: FlowCardInfo,
): FlowDetailState {
    if (state.kind !== "loading" || !sameRequest(state.request, request)) {
        return state;
    }
    if (card.id !== request.cardId) {
        return {
            kind: "error",
            request: state.request,
            message: "The returned card did not match the requested card.",
        };
    }
    return { kind: "ready", request: state.request, card };
}

/** Commit a failure only to its still-current request. */
export function detailFailed(
    state: FlowDetailState,
    request: DetailRequest,
    message: string,
): FlowDetailState {
    return state.kind === "loading" && sameRequest(state.request, request)
        ? { kind: "error", request: state.request, message }
        : state;
}

export type DetailSourceIconKind = "link" | "document" | "unknown";

/** A source ready for rendering without trusting its raw URL. */
export interface DetailSourceView {
    label: string;
    href: string | null;
    icon: DetailSourceIconKind;
}

/**
 * Returns a canonical link only for absolute HTTP(S) URLs without embedded
 * credentials. Every other input remains plain text in the Sheet.
 */
export function safeSourceUrl(raw: CardSource["url"]): string | null {
    if (raw === undefined || raw.trim() === "") return null;
    try {
        const url = new URL(raw);
        if (
            (url.protocol !== "http:" && url.protocol !== "https:") ||
            url.username !== "" ||
            url.password !== ""
        ) {
            return null;
        }
        return url.href;
    } catch {
        return null;
    }
}

/** Map the protocol source type onto the icon vocabulary used by the Sheet. */
export function detailSourceIcon(
    source: Pick<CardSource, "sourceType">,
): DetailSourceIconKind {
    switch (source.sourceType) {
        case "link":
            return "link";
        case "document":
            return "document";
        default:
            return "unknown";
    }
}

/** Project one source with a safe optional link and a useful visible label. */
export function toDetailSourceView(source: CardSource): DetailSourceView {
    const href = safeSourceUrl(source.url);
    const title = source.title.trim();
    return {
        label: title === "" ? (href ?? "Untitled source") : title,
        href,
        icon: detailSourceIcon(source),
    };
}

/** Project all generated sources attached to one detail card. */
export function detailSources(
    card: Pick<FlowCardInfo, "sources">,
): DetailSourceView[] {
    return card.sources.map(toDetailSourceView);
}

/** A stable human date for the Sheet, or null when the server omitted it. */
export function humanDetailDate(raw: FlowCardInfo["createdAt"]): string | null {
    if (raw.trim() === "") return null;
    const date = new Date(raw);
    if (Number.isNaN(date.getTime())) return null;
    return new Intl.DateTimeFormat("en", {
        day: "numeric",
        month: "short",
        year: "numeric",
    }).format(date);
}
