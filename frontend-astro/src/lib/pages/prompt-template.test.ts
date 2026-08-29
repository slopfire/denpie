import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { EnhancePromptTemplateResultSchema } from "@/generated/denpie_pb";
import type { SettingsDraft } from "./settings-page";
import type { TopicEditorDraft } from "./topic-editor";
import {
    DEFAULT_PROMPT_TEMPLATE,
    applyPromptSuggestionToSettings,
    applyPromptSuggestionToTopic,
    resetSettingsPrompt,
    resetTopicPrompt,
} from "./prompt-template";

function settingsDraft(): SettingsDraft {
    return {
        model: "gpt-5",
        compressModel: "",
        template: "old {topic}",
        apiKey: "",
        baseUrl: "",
        compressBaseUrl: "",
        reasoningEffort: "none",
        compressReasoningEffort: "none",
        compressionLevel: "strong",
        groundingModel: "keep-me",
        groundingReasoningEffort: "low",
        groundingStrategy: "factual",
        imageStrategy: "none",
        searchProvider: "tavily",
        scrapeProvider: "scrapling",
        searchApiKey: "",
        searchBaseUrl: "",
        visionModel: "",
        dailyTimeZone: "UTC",
        dailyUpdateTime: "07:00",
        maxActiveCards: "0",
        colorScheme: "shadcn",
        transparency: "solid",
        blurIntensity: "medium",
        autoupdateEnabled: false,
        autoupdateRepo: "",
        autoupdateBranch: "",
        autoupdateCheckIntervalSecs: "0",
        autoupdateCommand: "",
    };
}

function topicDraft(): TopicEditorDraft {
    return {
        promptTemplate: "Be terse about {topic}",
        dailyCardCount: "3",
        dailyTimeZone: "UTC",
        dailyUpdateTime: "07:00",
        compressionLevel: "balanced",
        groundingModel: "",
        groundingReasoningEffort: "",
        groundingStrategy: "factual",
        imageStrategy: "",
    };
}

describe("prompt template helpers", () => {
    test("reset restores the built-in global template", () => {
        expect(resetSettingsPrompt(settingsDraft()).template).toBe(
            DEFAULT_PROMPT_TEMPLATE,
        );
        expect(DEFAULT_PROMPT_TEMPLATE).toContain("{topic}");
    });

    test("topic reset pastes the current global prompt", () => {
        expect(
            resetTopicPrompt(topicDraft(), "Write about {topic} today.").promptTemplate,
        ).toBe("Write about {topic} today.");
        expect(resetTopicPrompt(topicDraft(), "   ").promptTemplate).toBe(
            DEFAULT_PROMPT_TEMPLATE,
        );
    });

    test("empty grounding fields keep the current draft values", () => {
        const suggestion = create(EnhancePromptTemplateResultSchema, {
            promptTemplate: "Write useful daily tip cards about {topic}.",
            rationale: "too many known titles",
        });
        const settings = applyPromptSuggestionToSettings(
            settingsDraft(),
            suggestion,
        );
        expect(settings.template).toBe(suggestion.promptTemplate);
        expect(settings.groundingStrategy).toBe("factual");
        expect(settings.groundingModel).toBe("keep-me");
        expect(settings.imageStrategy).toBe("none");

        const topic = applyPromptSuggestionToTopic(topicDraft(), suggestion);
        expect(topic.promptTemplate).toBe(suggestion.promptTemplate);
        expect(topic.groundingStrategy).toBe("factual");
        expect(topic.groundingModel).toBe("");
    });

    test("filled grounding fields replace the draft", () => {
        const suggestion = create(EnhancePromptTemplateResultSchema, {
            promptTemplate: "Write useful daily tip cards about {topic}.",
            groundingStrategy: "rag",
            groundingModel: "gpt-5.4-mini",
            groundingReasoningEffort: "high",
            imageStrategy: "bing_html",
            rationale: "assigned notes",
        });
        const settings = applyPromptSuggestionToSettings(
            settingsDraft(),
            suggestion,
        );
        expect(settings.groundingStrategy).toBe("rag");
        expect(settings.groundingModel).toBe("gpt-5.4-mini");
        expect(settings.groundingReasoningEffort).toBe("high");
        expect(settings.imageStrategy).toBe("bing_html");
    });
});
