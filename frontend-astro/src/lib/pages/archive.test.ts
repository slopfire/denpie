import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { TipcardInfoSchema } from "@/generated/denpie_pb";
import {
    archiveSearch,
    archiveStatusLabel,
    archiveTopics,
    cardStatusLabel,
    filterArchiveCards,
    inventoryToFlowCard,
    parseArchiveSearch,
} from "./archive";

function card(
    init: Partial<Parameters<typeof create<typeof TipcardInfoSchema>>[1]> & {
        id: bigint;
        topicName: string;
        title: string;
        status: string;
        createdAt: string;
        repeatCount: number;
    },
) {
    return create(TipcardInfoSchema, init);
}

describe("archive page helpers", () => {
    const cards = [
        card({
            id: 2n,
            topicName: "Rust",
            title: "Ownership",
            status: "active",
            createdAt: "2026-01-01",
            repeatCount: 2,
        }),
        card({
            id: 1n,
            topicName: "Astro",
            title: "Routing",
            status: "completed",
            createdAt: "2027-01-01",
            repeatCount: 0,
        }),
        card({
            id: 3n,
            topicName: "Rust",
            title: "Borrowing",
            status: "pending",
            createdAt: "2025-01-01",
            repeatCount: 0,
        }),
    ];

    test("filters scheduled cards and searches card content", () => {
        expect(
            filterArchiveCards(cards, {
                query: "own",
                status: "scheduled",
                topic: "",
                sort: "topic",
            }).map((item) => item.id),
        ).toEqual([2n]);
    });

    test("sorts without mutating source and returns unique topics", () => {
        expect(
            filterArchiveCards(cards, {
                query: "",
                status: "all",
                topic: "",
                sort: "date",
            }).map((item) => item.id),
        ).toEqual([1n, 2n, 3n]);
        expect(cards.map((item) => item.id)).toEqual([2n, 1n, 3n]);
        expect(archiveTopics(cards)).toEqual(["Astro", "Rust"]);
    });

    test("labels every supported status", () => {
        expect(archiveStatusLabel("custom")).toBe("Custom");
        expect(cardStatusLabel(cards[0]!)).toBe("Scheduled");
        expect(
            cardStatusLabel(
                card({
                    id: 4n,
                    topicName: "",
                    title: "",
                    status: "",
                    createdAt: "",
                    repeatCount: 0,
                }),
            ),
        ).toBe("Unknown");
    });

    test("round-trips topic archive deep links and ignores invalid status", () => {
        expect(
            parseArchiveSearch("?status=pending&topic=Astro%20migration"),
        ).toEqual({
            status: "pending",
            topic: "Astro migration",
        });
        expect(parseArchiveSearch("?status=not-a-status&topic=%20%20")).toEqual(
            {
                status: "all",
                topic: "",
            },
        );
        expect(
            archiveSearch({ status: "scheduled", topic: "Astro migration" }),
        ).toBe("?status=scheduled&topic=Astro+migration");
        expect(archiveSearch({ status: "all", topic: "" })).toBe("");
    });

    test("projects inventory cards onto the Flow card message", () => {
        const flow = inventoryToFlowCard(
            card({
                id: 44n,
                topicName: "Rust",
                title: "Ownership",
                status: "active",
                createdAt: "2026-01-01",
                repeatCount: 2,
                fullContent: "full",
                compressedContent: "compact",
            }),
        );
        expect(flow.id).toBe(44n);
        expect(flow.topicName).toBe("Rust");
        expect(flow.fullContent).toBe("full");
        expect(flow.pendingCount).toBe(0n);
    });
});
