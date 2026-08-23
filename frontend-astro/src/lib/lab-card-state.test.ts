import { describe, expect, test } from "bun:test";
import {
    continueLabCard,
    deleteLabCard,
    pinLabCard,
    reviewLabCard,
} from "./lab-card-state";
import { fixtureToFlowCard, type LabCardFixtureJson } from "./lab-card-view";

const fixture: LabCardFixtureJson = {
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
};

describe("lab card local mutations", () => {
    test("pin, review, continue, and delete stay on the fixture list", () => {
        const card = fixtureToFlowCard(fixture, 0);
        const start = [
            {
                fixtureId: fixture.id,
                notes: fixture.notes,
                reviewMessage: null,
                card,
            },
        ];
        const pinned = pinLabCard(start, card.id, true);
        expect(pinned[0].card.pinned).toBe(true);
        const reviewed = reviewLabCard(pinned, card.id, "Review saved");
        expect(reviewed[0].card.status).toBe("reviewed");
        expect(reviewed[0].reviewMessage).toBe("Review saved");
        const continued = continueLabCard(reviewed, card.id);
        expect(continued[0].card.status).toBe("active");
        expect(continued[0].reviewMessage).toBeNull();
        expect(deleteLabCard(continued, card.id)).toEqual([]);
    });
});
