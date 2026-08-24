import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { LabReviewPayload } from "@/lib/lab-review";
import { LabReviewWorkbench } from "./LabReviewWorkbench";

const payload: LabReviewPayload = {
    version: 1,
    baseline: {
        side: "baseline",
        label: "before",
        bench: "prompts",
        source: "lab/runs/before",
        manifest: null,
        rows: [
            {
                kind: "prompt",
                key: "rust/1",
                caseId: "rust",
                expected: "A focused, correct Rust card",
                artifact: {
                    title: "Ownership before",
                    fullContent: "Full baseline content",
                    compressedContent: "Compact baseline",
                    useImage: false,
                    imageQuery: "",
                },
                metrics: { total_tokens: 40 },
            },
        ],
    },
    candidate: {
        side: "candidate",
        label: "after",
        bench: "prompts",
        source: "lab/runs/after",
        manifest: null,
        rows: [
            {
                kind: "prompt",
                key: "rust/1",
                caseId: "rust",
                expected: "A focused, correct Rust card",
                artifact: {
                    title: "Ownership after",
                    fullContent: "Full candidate content",
                    compressedContent: "Compact candidate",
                    useImage: false,
                    imageQuery: "",
                },
                metrics: { total_tokens: 32 },
            },
        ],
    },
};

describe("LabReviewWorkbench", () => {
    test("renders blinded artifacts, structured judgments, and export controls", () => {
        const markup = renderToStaticMarkup(
            createElement(LabReviewWorkbench, { payload }),
        );
        expect(markup).toContain("A focused, correct Rust card");
        expect(markup).toContain("Ownership before");
        expect(markup).toContain("Ownership after");
        expect(markup).toContain("Export review.json");
        expect(markup).toContain("Correctness");
        expect(markup).toContain("Image relevance");
    });
});
