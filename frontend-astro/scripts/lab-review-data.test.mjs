import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
    loadLabReviewPayload,
    resolveLabReviewInput,
} from "./lab-review-data.mjs";

const temporaryDirectories = [];

afterEach(() => {
    for (const path of temporaryDirectories.splice(0)) {
        rmSync(path, { recursive: true, force: true });
    }
});

function promptRun(label, tokenCount) {
    const root = mkdtempSync(join(tmpdir(), "denpie-lab-review-"));
    temporaryDirectories.push(root);
    mkdirSync(join(root, "cases"));
    writeFileSync(
        join(root, "manifest.json"),
        JSON.stringify({ bench: "prompts", label }),
    );
    writeFileSync(
        join(root, "scorecard.json"),
        JSON.stringify([
            {
                case_id: "rust",
                mode: "one_shot",
                expected: "A focused Rust card",
                total_tokens: tokenCount,
            },
        ]),
    );
    writeFileSync(
        join(root, "cases", "rust.card.json"),
        JSON.stringify({
            title: "Ownership",
            full_content: "Full",
            compressed_content: "Compact",
            use_image: false,
            image_query: "",
        }),
    );
    return root;
}

describe("lab review build data", () => {
    test("loads manifests, scorecards, and prompt artifacts", () => {
        const baseline = promptRun("before", 20);
        const candidate = promptRun("after", 12);
        const payload = loadLabReviewPayload({ baseline, candidate });
        expect(payload.baseline.label).toBe("before");
        expect(payload.candidate.rows[0].artifact.title).toBe("Ownership");
        expect(payload.candidate.rows[0].metrics.total_tokens).toBe(12);
    });

    test("stays disabled unless both paths are supplied", () => {
        expect(
            loadLabReviewPayload({ baseline: "", candidate: "" }),
        ).toBeNull();
    });

    test("resolves run labels, latest, and named baselines", () => {
        const root = mkdtempSync(join(tmpdir(), "denpie-lab-selectors-"));
        temporaryDirectories.push(root);
        const run = join(root, "lab", "runs", "2026-prompts");
        mkdirSync(run, { recursive: true });
        writeFileSync(
            join(run, "manifest.json"),
            JSON.stringify({ bench: "prompts", label: "candidate" }),
        );
        writeFileSync(
            join(root, "lab", "runs", "baselines.json"),
            JSON.stringify({ stable: "lab/runs/2026-prompts" }),
        );

        expect(resolveLabReviewInput("candidate", root)).toBe(run);
        expect(resolveLabReviewInput("latest", root)).toBe(run);
        expect(resolveLabReviewInput("stable", root)).toBe(run);
    });
});
