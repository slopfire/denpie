import { create } from "@bufbuild/protobuf";
import { TipcardTypeValue, TipsRequestV1Schema } from "../generated/denpie_pb";
import type { TipsRequestV1 } from "../generated/denpie_pb";

/**
 * UI card kind. Protocol strings (`casual_tip`, …) are a wire concern and
 * never UI state; the mapping to generated `TipcardTypeValue` happens
 * exhaustively at {@link tipcardTypeFor}.
 */
export type AddCardKind = "casual" | "repeatable" | "manual";

export const ADD_CARD_KINDS: readonly AddCardKind[] = [
    "casual",
    "repeatable",
    "manual",
];

/** localStorage prefill keys. */
export const PREFILL_TOPIC_STORAGE_KEY = "denpie_prefill_topic";
export const PREFILL_TYPE_STORAGE_KEY = "denpie_prefill_type";

/** Stored prefill type is the raw protocol string. */
const STORED_KIND_VALUES: Record<string, AddCardKind> = {
    casual_tip: "casual",
    repeatable_tip: "repeatable",
    manual_tip: "manual",
};

/**
 * Normalize a stored prefill value: missing or unknown kinds fall back to
 * Casual.
 */
export function parseStoredCardKind(
    raw: string | null | undefined,
): AddCardKind {
    if (raw === null || raw === undefined) return "casual";
    return STORED_KIND_VALUES[raw] ?? "casual";
}

/** Delete both prefill keys — only after a successful mutation. */
export function clearAddPrefill(): void {
    if (typeof window === "undefined") return;
    window.localStorage.removeItem(PREFILL_TOPIC_STORAGE_KEY);
    window.localStorage.removeItem(PREFILL_TYPE_STORAGE_KEY);
}

/** Exhaustive kind → generated enum mapping at the API boundary. */
export function tipcardTypeFor(kind: AddCardKind): TipcardTypeValue {
    switch (kind) {
        case "casual":
            return TipcardTypeValue.CASUAL;
        case "repeatable":
            return TipcardTypeValue.REPEATABLE;
        case "manual":
            return TipcardTypeValue.MANUAL;
    }
}

/**
 * Parse the ToggleGroup UI value (`casual`/`repeatable`/`manual`) back into
 * the union; any unexpected value keeps Casual selected.
 */
export function parseAddCardKind(raw: string): AddCardKind {
    return raw === "repeatable" || raw === "manual" ? raw : "casual";
}

/**
 * Parse comma-separated topics: split on commas, trim each item, discard
 * empty items, preserve order, never deduplicate.
 */
export function parseTopicsCsv(topics: string): string[] {
    return topics
        .split(",")
        .map((topic) => topic.trim())
        .filter((topic) => topic !== "");
}

/** Repeatable creation asks for five cards; every other kind sends zero. */
export const REPEATABLE_COUNT = 5;

/** The exact caller-owned mutation payload captured at submit time. */
export interface AddTipsPayload {
    kind: AddCardKind;
    /** Normalized (trimmed, non-empty, order-preserving) topic list. */
    topics: string[];
    /** Manual content; empty for other kinds. */
    manualContent: string;
    /** Processed manual images as data URLs; empty for other kinds. */
    manualImageData: string[];
    /** Caller-owned non-empty idempotency key. */
    idempotencyKey: string;
}

/** Deterministic validation failure before any fetch launches. */
export class AddValidationError extends Error {
    constructor(message: string) {
        super(message);
        this.name = "AddValidationError";
    }
}

/**
 * Build the exact wire fields for one submission: repeatable sends count 5,
 * others 0; manual sends its content and images; other kinds send empty
 * manual fields; `excludeCardIds` and `manualCompressedContent` stay empty.
 */
export function buildTipsRequest(payload: AddTipsPayload): TipsRequestV1 {
    if (payload.idempotencyKey.trim() === "") {
        throw new AddValidationError(
            "add requires a non-empty idempotency key",
        );
    }
    // `topics` is already the captured form list. Normalize each entry without
    // joining/re-splitting it, so commas, duplicates, and order are not altered
    // after the CSV boundary has been crossed.
    const topics = payload.topics
        .map((topic) => topic.trim())
        .filter((topic) => topic !== "");
    if (topics.length === 0) {
        throw new AddValidationError("Add at least one topic.");
    }
    const isManual = payload.kind === "manual";
    if (isManual && payload.manualContent.trim() === "") {
        throw new AddValidationError("Manual cards need content.");
    }
    return create(TipsRequestV1Schema, {
        count: payload.kind === "repeatable" ? REPEATABLE_COUNT : 0,
        topics,
        tipcardType: tipcardTypeFor(payload.kind),
        excludeCardIds: [],
        manualContent: isManual ? payload.manualContent : "",
        manualCompressedContent: "",
        manualImageData: isManual ? [...payload.manualImageData] : [],
    });
}

// ---------------------------------------------------------------------------
// Image selection rules (matching backend limits).
// ---------------------------------------------------------------------------

/** Matches backend `MAX_IMAGE_BYTES`. */
export const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

/** Files at or below this size remain untouched data URLs. */
export const SKIP_IF_SMALLER_BYTES = 200 * 1024;

/** Longest edge after browser downscale (matches backend libcaesium). */
export const MAX_EDGE_PX = 2048;

/** WebP/JPEG encode quality in the 0.80–0.85 sweet spot. */
export const OUTPUT_QUALITY = 0.82;

/** Maximum images per manual card. */
export const MAX_MANUAL_IMAGES = 4;

const ALLOWED_IMAGE_TYPES: Record<string, true> = {
    "image/png": true,
    "image/jpeg": true,
    "image/webp": true,
    "image/gif": true,
};

export function isAllowedImageType(type: string): boolean {
    return ALLOWED_IMAGE_TYPES[type] === true;
}

export interface SelectableImage {
    type: string;
    size: number;
}

export type ImageSelectionResult =
    | { kind: "ok"; accepted: SelectableImage[] }
    | { kind: "rejected"; reason: string };

/**
 * Validate one selection batch against the current image count. Unsupported
 * types and over-limit selections are rejected persistently (the whole batch
 * is refused with an explanatory message — nothing partial is accepted).
 */
export function selectImages(
    candidates: readonly SelectableImage[],
    currentCount: number,
): ImageSelectionResult {
    if (candidates.length === 0) {
        return { kind: "rejected", reason: "No files selected." };
    }
    for (const file of candidates) {
        if (!isAllowedImageType(file.type)) {
            return {
                kind: "rejected",
                reason: "Unsupported image type. Use PNG, JPEG, WebP, or GIF files only.",
            };
        }
        if (
            !Number.isFinite(file.size) ||
            file.size < 0 ||
            file.size > MAX_IMAGE_BYTES
        ) {
            return {
                kind: "rejected",
                reason: "Each image must be 10 MiB or smaller.",
            };
        }
    }
    if (currentCount + candidates.length > MAX_MANUAL_IMAGES) {
        return {
            kind: "rejected",
            reason: `At most ${MAX_MANUAL_IMAGES} images per card.`,
        };
    }
    return { kind: "ok", accepted: [...candidates] };
}
/**
 * Longest-edge fit used by the browser downscale step: scale so the longest
 * edge becomes `maxEdge`; an image already within the bound keeps its size.
 */
export function fitWithin(
    width: number,
    height: number,
    maxEdge: number,
): { width: number; height: number } {
    if (width <= 0 || height <= 0) return { width: width, height: height };
    const longest = Math.max(width, height);
    if (longest <= maxEdge) return { width: width, height: height };
    const scale = maxEdge / longest;
    return {
        width: Math.max(1, Math.round(width * scale)),
        height: Math.max(1, Math.round(height * scale)),
    };
}
