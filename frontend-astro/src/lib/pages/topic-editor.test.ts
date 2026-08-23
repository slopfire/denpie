import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { AppTopicInfoSchema } from "@/generated/denpie_pb";
import {
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
        draft.imageStrategy = "pool";
        const patch = topicEditorPatch(current, draft);
        expect(patch.id).toBe(9n);
        expect(patch.dailyCardCount).toBe(5);
        expect(patch.imageStrategy).toBe("pool");
        expect(patch.promptTemplate).toBeUndefined();
    });

    test("treats a blank daily count as zero", () => {
        const current = topic();
        const draft = topicEditorDraft(current);
        draft.dailyCardCount = "";
        expect(topicEditorPatch(current, draft).dailyCardCount).toBe(0);
    });
});
