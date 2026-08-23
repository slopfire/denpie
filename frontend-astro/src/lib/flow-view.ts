// Pure Flow view logic: merging paginated cards by generated bigint ID and
// mapping protocol values onto human-readable UI data. No fetch, no React.
import type { FlowCardInfo } from "../generated/denpie_pb";

/**
 * Merge newly fetched cards into already rendered ones, deduplicating by the
 * generated `bigint` card ID (first occurrence wins, preserving order).
 */
export function mergeCardsById(
    existing: readonly FlowCardInfo[],
    incoming: readonly FlowCardInfo[],
): FlowCardInfo[] {
    const seen = new Set(existing.map((card) => card.id));
    return [...existing, ...incoming.filter((card) => !seen.has(card.id))];
}

/** UI-facing card projection: protocol IDs become human labels at this edge. */
export interface FlowCardView {
    id: bigint;
    title: string;
    topicName: string;
    /** Compressed content, falling back to full content when compression is empty. */
    content: string;
    pinned: boolean;
    repeatCount: number;
    pendingCount: bigint;
    typeLabel: string;
    statusLabel: string;
    imageUrls: string[];
}

// Protocol values verified against `proto/denpie.proto`, the generated
// `TipcardTypeValue` enum, and the backend vocabulary in
// `src/domain/tipcard.rs` (`casual_tip`/`repeatable_tip`/`manual_tip`/
// `custom_tip`) and review status writes (`active`, `pending`, `custom`,
// `learned`, `dismissed`). No invented values.
const TYPE_LABELS: Record<string, string> = {
    casual_tip: "Casual tip",
    repeatable_tip: "Repeatable tip",
    manual_tip: "Manual tip",
    custom_tip: "Custom tip",
};

const STATUS_LABELS: Record<string, string> = {
    active: "Active",
    pending: "Pending",
    custom: "Custom",
    learned: "Learned",
    dismissed: "Dismissed",
};

function labelFrom(map: Record<string, string>, raw: string): string {
    if (raw === "") return "Unspecified";
    return map[raw] ?? raw;
}

/**
 * Image URLs from each generated `downloadPath`, falling back to the
 * canonical `/api/v1/tipcard-images/{id}` route when the server omitted it
 * (same fallback as the browser client).
 */
export function cardImageUrls(card: Pick<FlowCardInfo, "images">): string[] {
    return card.images.map((image) =>
        image.downloadPath === ""
            ? `/api/v1/tipcard-images/${image.id}`
            : image.downloadPath,
    );
}

/** Repeatable-card backing stack, capped at three visible layers. */
export function repeatableStackLayers(
    card: Pick<FlowCardInfo, "tipcardType" | "pendingCount">,
): number {
    if (card.tipcardType !== "repeatable_tip" || card.pendingCount <= 0n)
        return 0;
    return card.pendingCount >= 3n ? 3 : Number(card.pendingCount);
}

export function toFlowCardView(card: FlowCardInfo): FlowCardView {
    return {
        id: card.id,
        title: card.title,
        topicName: card.topicName,
        content:
            card.compressedContent === ""
                ? card.fullContent
                : card.compressedContent,
        pinned: card.pinned,
        repeatCount: card.repeatCount,
        pendingCount: card.pendingCount,
        typeLabel: labelFrom(TYPE_LABELS, card.tipcardType),
        statusLabel: labelFrom(STATUS_LABELS, card.status),
        imageUrls: cardImageUrls(card),
    };
}

export function toFlowCardViews(
    cards: readonly FlowCardInfo[],
): FlowCardView[] {
    return cards.map(toFlowCardView);
}
