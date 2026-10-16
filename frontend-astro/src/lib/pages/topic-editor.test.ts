import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import {
    AppTopicInfoSchema,
    GroundingSearchProfileSchema,
} from "@/generated/denpie_pb";
import {
    applyTopicEditorDraft,
    applyTopicSourceIds,
    hasTopicEditorPatch,
    setTopicSourceAssignment,
    topicEditorDraft,
    topicEditorPatch,
    topicSourceIds,
    topicSourceMutations,
} from "./topic-editor";

function topic() {
    return create(AppTopicInfoSchema, {
        id: 9n,
        name: "Rust",
        promptTemplate: "Be terse",
        dailyCardCount: 3,
        dailyTimeZone: "UTC",
        dailyUpdateTime: "07:00",
        compressionLevel: "balanced",
        groundingModel: "gpt-5.4-mini",
        groundingReasoningEffort: "medium",
        groundingStrategy: "factual",
        groundingSourceMode: "model",
        groundingVerificationMode: "standard",
        groundingBehavior: "standard",
        imageStrategy: "bing_html",
    });
}

describe("topic editor patch", () => {
    test("sends only changed fields", () => {
        const current = topic();
        const draft = topicEditorDraft(current);
        expect(hasTopicEditorPatch(topicEditorPatch(current, draft))).toBe(
            false,
        );
        draft.dailyCardCount = "5";
        draft.groundingModel = "gpt-5.4";
        draft.groundingReasoningEffort = "high";
        draft.imageStrategy = "pool";
        const patch = topicEditorPatch(current, draft);
        expect(patch.id).toBe(9n);
        expect(patch.dailyCardCount).toBe(5);
        expect(patch.groundingModel).toBe("gpt-5.4");
        expect(patch.groundingReasoningEffort).toBe("high");
        expect(patch.imageStrategy).toBe("pool");
        expect(patch.promptTemplate).toBeUndefined();
    });

    test("treats a blank daily count as zero", () => {
        const current = topic();
        const draft = topicEditorDraft(current);
        draft.dailyCardCount = "";
        expect(topicEditorPatch(current, draft).dailyCardCount).toBe(0);
    });

    test("clears topic grounding overrides to inherit settings", () => {
        const current = topic();
        const draft = topicEditorDraft(current);
        draft.groundingModel = "";
        draft.groundingReasoningEffort = "";

        const patch = topicEditorPatch(current, draft);
        expect(patch.groundingModel).toBe("");
        expect(patch.groundingReasoningEffort).toBe("");

        const updated = applyTopicEditorDraft(current, draft);
        expect(updated.groundingModel).toBe("");
        expect(updated.groundingReasoningEffort).toBe("");
    });

    test("patches independent Grounding policy fields", () => {
        const current = topic();
        const draft = topicEditorDraft(current);
        draft.groundingSourceMode = "web_and_documents";
        draft.groundingBehavior = "fact_check";
        draft.groundingSearchDepth = "deep";
        draft.groundingFreshness = "week";
        draft.groundingSearchGuidance = "Prefer the Rust project.";
        draft.groundingIncludeDomains = "rust-lang.org";
        draft.groundingFailurePolicy = "stop";

        const patch = topicEditorPatch(current, draft);
        expect(patch.groundingSourceMode).toBe("web_and_documents");
        expect(patch.groundingVerificationMode).toBe("thorough");
        expect(patch.groundingBehavior).toBe("fact_check");
        expect(patch.groundingStrategy).toBe("create_and_ground");
        expect(patch.groundingSearchProfile).toEqual(
            create(GroundingSearchProfileSchema, {
                depth: "deep",
                freshness: "week",
                guidance: "Prefer the Rust project.",
                includeDomains: ["rust-lang.org"],
                failurePolicy: "stop",
            }),
        );
    });

    test("clears a custom search profile back to Grounding defaults", () => {
        const current = topic();
        current.groundingSearchProfile = create(GroundingSearchProfileSchema, {
            depth: "deep",
            freshness: "month",
            failurePolicy: "stop",
        });
        const draft = topicEditorDraft(current);
        draft.groundingSearchDepth = "";
        draft.groundingFreshness = "";
        draft.groundingSearchGuidance = "";
        draft.groundingIncludeDomains = "";
        draft.groundingFailurePolicy = "";

        const patch = topicEditorPatch(current, draft);
        expect(patch.clearGroundingSearchProfile).toBe(true);
        expect(patch.groundingSearchProfile).toBeUndefined();
        expect(
            applyTopicEditorDraft(current, draft).groundingSearchProfile,
        ).toBeUndefined();
    });
});

describe("topic source assignments", () => {
    const documents = [
        { id: 1n, topicIds: [9n] },
        { id: 2n, topicIds: [4n] },
        { id: 3n, topicIds: [4n, 9n] },
    ];

    test("reads and edits the selected source IDs", () => {
        expect(topicSourceIds(documents, 9n)).toEqual([1n, 3n]);
        expect(setTopicSourceAssignment([1n, 3n], 2n, true)).toEqual([
            1n,
            3n,
            2n,
        ]);
        expect(setTopicSourceAssignment([1n, 3n], 1n, false)).toEqual([3n]);
    });

    test("plans only the changed assignments", () => {
        expect(topicSourceMutations(documents, 9n, [2n, 3n])).toEqual([
            { kind: "detach", documentId: 1n },
            { kind: "attach", documentId: 2n },
        ]);
        expect(topicSourceMutations(documents, 9n, [1n, 3n])).toEqual([]);
    });

    test("applies the saved selection without changing other topic links", () => {
        const updated = applyTopicSourceIds(documents, 9n, [2n, 3n]);
        expect(updated.map((document) => document.topicIds)).toEqual([
            [],
            [4n, 9n],
            [4n, 9n],
        ]);
    });
});
