import { describe, expect, test } from "bun:test";
import {
    continueLabCard,
    deleteLabCard,
    labCardsFromFixtures,
    pinLabCard,
    reviewLabCard,
} from "./lab-card-state";
import type { LabCardFixtureJson } from "./lab-card-view";

function fixture(
    overrides: Partial<LabCardFixtureJson> = {},
): LabCardFixtureJson {
    return {
        id: "active",
        topic_name: "Rust",
        title: "Ownership",
        full_content: "body",
        compressed_content: "compact",
        tipcard_type: "repeatable_tip",
        status: "active",
        pinned: false,
        pending_count: 1,
        notes: "active fixture",
        ...overrides,
    };
}

describe("lab card local mutations", () => {
    test("pin, review, Continue, and delete use production slot states", () => {
        const start = labCardsFromFixtures([fixture()]);
        expect(start[0].kind).toBe("card");
        expect(start[0].slot.kind).toBe("idle");
        const id = start[0].slot.kind === "idle" ? start[0].slot.card.id : 0n;

        const pinned = pinLabCard(start, id, true);
        expect(pinned[0].kind === "card" && pinned[0].slot.card.pinned).toBe(
            true,
        );

        const reviewed = reviewLabCard(pinned, id);
        expect(reviewed[0].kind).toBe("followUp");
        expect(reviewed[0].slot.kind).toBe("completed");

        const continued = continueLabCard(reviewed, id);
        expect(continued[0].kind).toBe("card");
        expect(continued[0].slot.kind).toBe("idle");
        expect(
            continued[0].kind === "card" && continued[0].slot.card.status,
        ).toBe("active");
        expect(deleteLabCard(continued, id)).toEqual([]);
    });

    test("maps checked-in reviewed fixtures to honest production states", () => {
        const states = labCardsFromFixtures([
            fixture({
                id: "reviewed-hold",
                status: "reviewed",
                review_message: "Review saved",
            }),
            fixture({
                id: "await-refill",
                status: "reviewed",
                review_message: null,
            }),
            fixture({
                id: "daily-complete",
                status: "reviewed",
                review_message: "Continue with another set.",
            }),
        ]);

        expect(states.map((state) => state.slot.kind)).toEqual([
            "idle",
            "awaitingRefill",
            "completed",
        ]);
    });

    test("preserves fixture metadata used by the production detail view", () => {
        const [state] = labCardsFromFixtures([
            fixture({
                created_at: "2026-08-23T00:00:00Z",
                next_review_at: "2026-08-24T00:00:00Z",
                repeat_count: 4,
                topic_color: "#7c3aed",
                topic_icon: "lucide:book-open",
                sources: [
                    {
                        document_id: 42,
                        source_type: "link",
                        title: "Rust Book",
                        url: "https://doc.rust-lang.org/book/",
                    },
                ],
            }),
        ]);

        if (state.kind !== "card") throw new Error("expected a live card");
        expect(state.slot.card.createdAt).toBe("2026-08-23T00:00:00Z");
        expect(state.slot.card.nextReviewAt).toBe("2026-08-24T00:00:00Z");
        expect(state.slot.card.repeatCount).toBe(4);
        expect(state.slot.card.topicColor).toBe("#7c3aed");
        expect(state.slot.card.topicIcon).toBe("lucide:book-open");
        expect(state.slot.card.sources[0]).toMatchObject({
            documentId: 42n,
            sourceType: "link",
            title: "Rust Book",
            url: "https://doc.rust-lang.org/book/",
        });
    });
});
