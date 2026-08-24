import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { LabCardFixtureJson } from "@/lib/lab-card-view";
import { LabGallery } from "./LabGallery";

function fixture(
    id: string,
    overrides: Partial<LabCardFixtureJson> = {},
): LabCardFixtureJson {
    return {
        id,
        topic_name: "Rust",
        title: "Ownership",
        full_content: "body",
        compressed_content: "compact",
        tipcard_type: "repeatable_tip",
        status: "active",
        pinned: false,
        pending_count: 0,
        notes: `${id} fixture`,
        ...overrides,
    };
}

describe("LabGallery", () => {
    test("exposes production review, Continue, pin, and delete controls", () => {
        const fixtures = [
            fixture("active"),
            fixture("daily-complete", {
                status: "reviewed",
                review_message: "Continue with another set.",
            }),
        ];

        const markup = renderToStaticMarkup(
            createElement(LabGallery, { fixtures }),
        );

        expect(markup).toContain('data-testid="lab-fixture-active"');
        expect(markup).toContain('data-lab-state="idle"');
        expect(markup).toContain('data-testid="review-again-1"');
        expect(markup).toContain('data-testid="pin-1"');
        expect(markup).toContain('data-testid="card-more-1"');
        expect(markup).toContain('data-testid="continue-2"');
        expect(markup).toContain('data-lab-state="completed"');
    });
});
