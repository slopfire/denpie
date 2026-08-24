import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { AppTopicInfoSchema } from "@/generated/denpie_pb";
import {
    applyTopicEditorDraft,
    hasTopicEditorPatch,
    topicEditorDraft,
    topicEditorPatch,
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
});
