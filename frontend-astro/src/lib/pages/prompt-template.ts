import type { EnhancePromptTemplateResult } from "@/generated/denpie_pb";
import type { SettingsDraft } from "./settings-page";
import type { TopicEditorDraft } from "./topic-editor";

/** Keep in lockstep with `DEFAULT_PROMPT_TEMPLATE` in `src/llm/cards.rs`. */
export const DEFAULT_PROMPT_TEMPLATE = `Write useful daily tip cards about {topic}.

Each card should be practical, specific, and worth saving:
- the core idea in plain language
- why it matters
- one concrete example, command, checklist, or mini workflow
- one caveat or common mistake when useful

Keep each card focused. Markdown is allowed. Avoid filler, hype, and invented facts.`;

export function applyPromptSuggestionToSettings(
    draft: SettingsDraft,
    suggestion: EnhancePromptTemplateResult,
): SettingsDraft {
    return {
        ...draft,
        template: suggestion.promptTemplate,
        groundingStrategy:
            suggestion.groundingStrategy || draft.groundingStrategy,
        groundingModel: suggestion.groundingModel || draft.groundingModel,
        groundingReasoningEffort:
            suggestion.groundingReasoningEffort ||
            draft.groundingReasoningEffort,
        imageStrategy: suggestion.imageStrategy || draft.imageStrategy,
    };
}

export function applyPromptSuggestionToTopic(
    draft: TopicEditorDraft,
    suggestion: EnhancePromptTemplateResult,
): TopicEditorDraft {
    return {
        ...draft,
        promptTemplate: suggestion.promptTemplate,
        groundingStrategy:
            suggestion.groundingStrategy || draft.groundingStrategy,
        groundingModel: suggestion.groundingModel || draft.groundingModel,
        groundingReasoningEffort:
            suggestion.groundingReasoningEffort ||
            draft.groundingReasoningEffort,
        imageStrategy: suggestion.imageStrategy || draft.imageStrategy,
    };
}

export function resetSettingsPrompt(draft: SettingsDraft): SettingsDraft {
    return { ...draft, template: DEFAULT_PROMPT_TEMPLATE };
}

/** Paste the current global template into the topic field. */
export function resetTopicPrompt(
    draft: TopicEditorDraft,
    currentPrompt: string,
): TopicEditorDraft {
    const prompt = currentPrompt.trim() || DEFAULT_PROMPT_TEMPLATE;
    return { ...draft, promptTemplate: prompt };
}
