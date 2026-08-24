import { describe, expect, test } from "bun:test";
import {
    pairedReviewRows,
    parseLabReviewFile,
    parseLabReviewPayload,
    promptArtifactToFlowCard,
} from "./lab-review";

const promptRow = {
    kind: "prompt",
    key: "rust",
    caseId: "rust",
    expected: "Focused Rust card",
    artifact: {
        title: "Ownership",
        fullContent: "Full",
        compressedContent: "Compact",
        useImage: false,
        imageQuery: "",
    },
    metrics: { total_tokens: 42 },
};

function run(side: "baseline" | "candidate", rows: unknown[]) {
    return {
        side,
        label: side,
        bench: "prompts",
        source: `lab/runs/${side}`,
        manifest: null,
        rows,
    };
}

describe("lab review payload", () => {
    test("parses, pairs added rows, and projects prompt cards", () => {
        const payload = parseLabReviewPayload({
            version: 1,
            baseline: run("baseline", [promptRow]),
            candidate: run("candidate", [
                promptRow,
                { ...promptRow, key: "new", caseId: "new" },
            ]),
        });
        const pairs = pairedReviewRows(payload);
        expect(pairs).toHaveLength(2);
        expect(pairs.find((pair) => pair.key === "new")?.baseline).toBeNull();
        const artifact = payload.baseline.rows[0];
        if (artifact.kind !== "prompt" || artifact.artifact === null)
            throw new Error("expected prompt");
        const card = promptArtifactToFlowCard(artifact.artifact, 99n);
        expect(card.id).toBe(99n);
        expect(card.title).toBe("Ownership");
    });

    test("rejects mixed benches and invalid review verdicts", () => {
        expect(() =>
            parseLabReviewPayload({
                version: 1,
                baseline: run("baseline", [promptRow]),
                candidate: { ...run("candidate", []), bench: "images" },
            }),
        ).toThrow("different bench");
        const file = parseLabReviewFile({
            version: 1,
            baselineSource: "a",
            candidateSource: "b",
            updatedAt: "now",
            judgments: [
                { key: "rust", note: "", dimensions: { overall: "invalid" } },
            ],
        });
        expect(file.judgments[0].dimensions.overall).toBeUndefined();
    });
});
