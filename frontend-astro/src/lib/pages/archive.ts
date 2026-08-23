import { create } from "@bufbuild/protobuf";
import {
    FlowCardInfoSchema,
    type FlowCardInfo,
    type TipcardInfo,
} from "@/generated/denpie_pb";

export type ArchiveStatus =
    "all" | "active" | "completed" | "pending" | "scheduled" | "custom";

export type ArchiveSort = "topic" | "date";

export interface ArchiveFilters {
    query: string;
    status: ArchiveStatus;
    topic: string;
    sort: ArchiveSort;
}

export type ArchiveLocationFilters = Pick<ArchiveFilters, "status" | "topic">;

const archiveStatuses: readonly ArchiveStatus[] = [
    "all",
    "active",
    "completed",
    "pending",
    "scheduled",
    "custom",
];

function isArchiveStatus(value: string | null): value is ArchiveStatus {
    return value !== null && archiveStatuses.some((status) => status === value);
}

/** Parse the topic/status deep-link used by grounding topic affordances. */
export function parseArchiveSearch(search: string): ArchiveLocationFilters {
    const params = new URLSearchParams(search);
    const status = params.get("status");
    return {
        status: isArchiveStatus(status) ? status : "all",
        topic: params.get("topic")?.trim() ?? "",
    };
}

/** Serialize archive filters without adding empty/default values to the URL. */
export function archiveSearch(filters: ArchiveLocationFilters): string {
    const params = new URLSearchParams();
    if (filters.status !== "all") params.set("status", filters.status);
    if (filters.topic.trim() !== "") params.set("topic", filters.topic.trim());
    const value = params.toString();
    return value === "" ? "" : `?${value}`;
}

function statusMatches(card: TipcardInfo, status: ArchiveStatus): boolean {
    switch (status) {
        case "all":
            return true;
        case "scheduled":
            return card.status === "active" && card.repeatCount > 0;
        default:
            return card.status === status;
    }
}

function textMatches(card: TipcardInfo, query: string): boolean {
    const needle = query.trim().toLocaleLowerCase();
    if (needle === "") return true;
    return [
        card.title,
        card.topicName,
        card.fullContent,
        card.compressedContent,
    ].some((value) => value.toLocaleLowerCase().includes(needle));
}

/** Pure archive filtering/sorting; source cards are never mutated. */
export function filterArchiveCards(
    cards: readonly TipcardInfo[],
    filters: ArchiveFilters,
): TipcardInfo[] {
    const filtered = cards.filter(
        (card) =>
            statusMatches(card, filters.status) &&
            textMatches(card, filters.query) &&
            (filters.topic === "" ||
                card.topicName.localeCompare(filters.topic, undefined, {
                    sensitivity: "base",
                }) === 0),
    );
    return [...filtered].sort((left, right) => {
        if (filters.sort === "date") {
            return (
                right.createdAt.localeCompare(left.createdAt) ||
                (right.id < left.id ? -1 : right.id > left.id ? 1 : 0)
            );
        }
        return (
            left.topicName.localeCompare(right.topicName, undefined, {
                sensitivity: "base",
            }) ||
            left.title.localeCompare(right.title, undefined, {
                sensitivity: "base",
            }) ||
            (left.id < right.id ? -1 : left.id > right.id ? 1 : 0)
        );
    });
}

const HEADING_MAX = 80;

/** First rows of the archive grid hydrate immediately; the rest wait for view. */
export const ARCHIVE_EAGER_HYDRATE_COUNT = 9;

/** Overscan around the viewport before a card drops markdown and images. */
export const ARCHIVE_VIEWPORT_ROOT_MARGIN = "800px 0px";

const PREVIEW_CHARS = 280;

export function shouldEagerHydrateArchiveCard(index: number): boolean {
    return index >= 0 && index < ARCHIVE_EAGER_HYDRATE_COUNT;
}

/** Plain clipped preview used while an archive card is off-screen. */
export function archiveCardPreview(content: string): string {
    const stripped = content.replace(/\s+/g, " ").trim();
    if (stripped.length <= PREVIEW_CHARS) return stripped;
    return `${stripped.slice(0, PREVIEW_CHARS - 1).trimEnd()}…`;
}

/**
 * Visible archive heading: a real title wins; otherwise the first line of
 * body text. Empty only when the card has no title and no content.
 */
export function archiveCardHeading(
    card: Pick<TipcardInfo, "title" | "fullContent" | "compressedContent">,
): string {
    const titled = card.title.trim();
    if (titled !== "") return titled;
    const content = card.fullContent.trim() || card.compressedContent.trim();
    if (content === "") return "";
    let firstLine = "";
    for (const line of content.split(/\r?\n/)) {
        const stripped = line.replace(/^#+\s*/, "").trim();
        if (stripped !== "") {
            firstLine = stripped;
            break;
        }
    }
    if (firstLine === "") return "";
    if (firstLine.length <= HEADING_MAX) return firstLine;
    return `${firstLine.slice(0, HEADING_MAX - 1).trimEnd()}…`;
}

export function archiveTopics(cards: readonly TipcardInfo[]): string[] {
    return [
        ...new Set(cards.map((card) => card.topicName).filter(Boolean)),
    ].sort((left, right) =>
        left.localeCompare(right, undefined, { sensitivity: "base" }),
    );
}

export function archiveStatusLabel(status: ArchiveStatus): string {
    switch (status) {
        case "all":
            return "All cards";
        case "active":
            return "Active";
        case "completed":
            return "Completed";
        case "pending":
            return "Pending";
        case "scheduled":
            return "Scheduled";
        case "custom":
            return "Custom";
    }
}

export function cardStatusLabel(card: TipcardInfo): string {
    return card.status === "active" && card.repeatCount > 0
        ? "Scheduled"
        : card.status === ""
          ? "Unknown"
          : card.status.charAt(0).toUpperCase() + card.status.slice(1);
}

/** Project an inventory card onto the Flow card message so Archive can reuse Flow UI. */
export function inventoryToFlowCard(card: TipcardInfo): FlowCardInfo {
    return create(FlowCardInfoSchema, {
        id: card.id,
        topicName: card.topicName,
        topicIcon: card.topicIcon,
        topicColor: card.topicColor,
        title: card.title,
        fullContent: card.fullContent,
        compressedContent: card.compressedContent,
        createdAt: card.createdAt,
        tipcardType: card.tipcardType,
        status: card.status,
        nextReviewAt: card.nextReviewAt,
        repeatCount: card.repeatCount,
        pinned: card.pinned,
        pendingCount: 0n,
        images: card.images,
        sources: [],
    });
}
