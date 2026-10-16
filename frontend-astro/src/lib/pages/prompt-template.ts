import type { EnhancePromptTemplateResult } from "@/generated/denpie_pb";
import type { SettingsDraft } from "./settings-page";
import type { TopicEditorDraft } from "./topic-editor";
import { legacyGroundingPolicy } from "@/lib/grounding-policy";

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
    const suggestedPolicy = suggestion.groundingStrategy
        ? legacyGroundingPolicy(suggestion.groundingStrategy)
        : null;
    return {
        ...draft,
        template: suggestion.promptTemplate,
        groundingStrategy:
            suggestion.groundingStrategy || draft.groundingStrategy,
        groundingSourceMode:
            suggestedPolicy?.sourceMode ?? draft.groundingSourceMode,
        groundingBehavior: suggestedPolicy?.behavior ?? draft.groundingBehavior,
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
    const suggestedPolicy = suggestion.groundingStrategy
        ? legacyGroundingPolicy(suggestion.groundingStrategy)
        : null;
    return {
        ...draft,
        promptTemplate: suggestion.promptTemplate,
        groundingStrategy:
            suggestion.groundingStrategy || draft.groundingStrategy,
        groundingSourceMode:
            suggestedPolicy?.sourceMode ?? draft.groundingSourceMode,
        groundingBehavior: suggestedPolicy?.behavior ?? draft.groundingBehavior,
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
