import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { FlowCardInfoSchema, type FlowCardInfo } from "../generated/denpie_pb";
import type { ReviewSlot } from "./flow-review-state";
import { selectTopicPicks, splitTopicPicks } from "./flow-transmission";

function card(
    id: bigint,
    topicName: string,
    options: Partial<Pick<FlowCardInfo, "tipcardType" | "status">> = {},
): FlowCardInfo {
    return create(FlowCardInfoSchema, {
        id,
        title: `card-${id}`,
        topicName,
        fullContent: `content-${id}`,
        tipcardType: options.tipcardType ?? "casual_tip",
        status: options.status ?? "active",
        createdAt: "2026-01-01T00:00:00Z",
    });
}

function idle(flowCard: FlowCardInfo): ReviewSlot {
    return { kind: "idle", card: flowCard };
}

function completedPlaceholder(
    reviewedCardId: bigint,
    topicName: string,
): ReviewSlot {
    return {
        kind: "completed",
        reviewedCardId,
        topicName,
        title: `reviewed-${reviewedCardId}`,
        createdAt: "2026-01-01T00:00:00Z",
        tipcardType: "repeatable_tip",
        pinned: false,
    };
}

function ids(slots: readonly ReviewSlot[]): bigint[] {
    return slots.map((slot) =>
        slot.kind === "idle" ||
        slot.kind === "reviewing" ||
        slot.kind === "error"
            ? slot.card.id
            : slot.reviewedCardId,
    );
}

describe("Transmission topic picks", () => {
    test("takes three from each of three topics and keeps overflow", () => {
        const slots = [
            ...Array.from({ length: 4 }, (_, index) =>
                idle(card(BigInt(index), "alpha")),
            ),
            ...Array.from({ length: 4 }, (_, index) =>
                idle(card(BigInt(10 + index), "beta")),
            ),
            ...Array.from({ length: 4 }, (_, index) =>
                idle(card(BigInt(20 + index), "gamma")),
            ),
        ];

        const partition = splitTopicPicks(slots);

        expect(partition.picks).toHaveLength(9);
        expect(
            partition.picks.map((slot) => slotMetadataForTest(slot)),
        ).toEqual([
            "alpha",
            "alpha",
            "alpha",
            "beta",
            "beta",
            "beta",
            "gamma",
            "gamma",
            "gamma",
        ]);
        expect(ids(partition.remaining)).toEqual([3n, 13n, 23n]);
    });

    test("adapts to four and ten topics", () => {
        const fourTopics = Array.from({ length: 4 }, (_, topicIndex) =>
            Array.from({ length: 3 }, (_, cardIndex) =>
                idle(
                    card(
                        BigInt(topicIndex * 10 + cardIndex),
                        `topic-${topicIndex}`,
                    ),
                ),
            ),
        ).flat();
        const tenTopics = Array.from({ length: 10 }, (_, topicIndex) =>
            idle(card(BigInt(100 + topicIndex), `topic-${topicIndex}`)),
        );

        expect(selectTopicPicks(fourTopics)).toHaveLength(8);
        expect(selectTopicPicks(fourTopics).map(slotMetadataForTest)).toEqual([
            "topic-0",
            "topic-0",
            "topic-1",
            "topic-1",
            "topic-2",
            "topic-2",
            "topic-3",
            "topic-3",
        ]);
        expect(selectTopicPicks(tenTopics)).toHaveLength(9);
    });

    test("keeps one active repeatable slot per topic", () => {
        const placeholder = completedPlaceholder(1n, "rust");
        const activeLater = idle(
            card(2n, "rust", { tipcardType: "repeatable_tip" }),
        );
        const activeDuplicate = idle(
            card(3n, "rust", { tipcardType: "repeatable_tip" }),
        );
        const other = idle(
            card(4n, "python", { tipcardType: "repeatable_tip" }),
        );

        const partition = splitTopicPicks([
            placeholder,
            activeLater,
            activeDuplicate,
            other,
        ]);

        // An active card wins over the reviewed placeholder even when it comes
        // later in source order, and the active topic contributes one slot.
        expect(ids(partition.picks)).toEqual([2n, 4n]);
        expect(ids(partition.remaining)).toEqual([]);
    });

    test("uses a repeatable placeholder when no live card exists", () => {
        const placeholder = completedPlaceholder(55n, "history");
        const casual = idle(card(56n, "math"));

        expect(ids(selectTopicPicks([placeholder, casual]))).toEqual([
            55n,
            56n,
        ]);
    });

    test("keeps every non-repeatable overflow card and collapses repeatable duplicates", () => {
        const slots: ReviewSlot[] = [
            idle(card(1n, "a")),
            idle(card(2n, "a")),
            idle(card(3n, "a", { status: "reviewed" })),
            completedPlaceholder(4n, "a"),
            idle(card(5n, "b")),
            idle(card(6n, "c")),
        ];
        const before = [...slots];
        const partition = splitTopicPicks(slots);

        expect(ids(partition.picks)).toEqual([1n, 2n, 5n, 6n]);
        expect(ids(partition.remaining)).toEqual([3n]);
        expect(slots).toEqual(before);
    });
});

function slotMetadataForTest(slot: ReviewSlot): string {
    return slot.kind === "idle" ||
        slot.kind === "reviewing" ||
        slot.kind === "error"
        ? slot.card.topicName
        : slot.topicName;
}
