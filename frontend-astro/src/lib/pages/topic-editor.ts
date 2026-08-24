import { create } from "@bufbuild/protobuf";
import {
    UpdateTopicRequestSchema,
    type AppTopicInfo,
    type UpdateTopicRequest,
} from "@/generated/denpie_pb";

export interface TopicEditorDraft {
    promptTemplate: string;
    dailyCardCount: string;
    dailyTimeZone: string;
    dailyUpdateTime: string;
    compressionLevel: string;
    groundingModel: string;
    groundingReasoningEffort: string;
    groundingStrategy: string;
    imageStrategy: string;
}

export function topicEditorDraft(topic: AppTopicInfo): TopicEditorDraft {
    return {
        promptTemplate: topic.promptTemplate,
        dailyCardCount: topic.dailyCardCount.toString(),
        dailyTimeZone: topic.dailyTimeZone,
        dailyUpdateTime: topic.dailyUpdateTime,
        compressionLevel: topic.compressionLevel,
        groundingModel: topic.groundingModel,
        groundingReasoningEffort: topic.groundingReasoningEffort,
        groundingStrategy: topic.groundingStrategy,
        imageStrategy: topic.imageStrategy,
    };
}

function parseDailyCount(value: string): number {
    const normalized = value.trim();
    if (!/^\d+$/.test(normalized)) return 0;
    return Number.parseInt(normalized, 10);
}

/** Minimal update payload: unchanged topic fields never cross the wire. */
export function topicEditorPatch(
    topic: AppTopicInfo,
    draft: TopicEditorDraft,
): UpdateTopicRequest {
    const patch = create(UpdateTopicRequestSchema, { id: topic.id });
    if (draft.promptTemplate !== topic.promptTemplate)
        patch.promptTemplate = draft.promptTemplate;
    const dailyCardCount = parseDailyCount(draft.dailyCardCount);
    if (dailyCardCount !== topic.dailyCardCount)
        patch.dailyCardCount = dailyCardCount;
    if (draft.dailyTimeZone !== topic.dailyTimeZone)
        patch.dailyTimeZone = draft.dailyTimeZone;
    if (draft.dailyUpdateTime !== topic.dailyUpdateTime)
        patch.dailyUpdateTime = draft.dailyUpdateTime;
    if (draft.compressionLevel !== topic.compressionLevel)
        patch.compressionLevel = draft.compressionLevel;
    if (draft.groundingModel !== topic.groundingModel)
        patch.groundingModel = draft.groundingModel;
    if (draft.groundingReasoningEffort !== topic.groundingReasoningEffort)
        patch.groundingReasoningEffort = draft.groundingReasoningEffort;
    if (draft.groundingStrategy !== topic.groundingStrategy)
        patch.groundingStrategy = draft.groundingStrategy;
    if (draft.imageStrategy !== topic.imageStrategy)
        patch.imageStrategy = draft.imageStrategy;
    return patch;
}

export function hasTopicEditorPatch(patch: UpdateTopicRequest): boolean {
    return (
        patch.promptTemplate !== undefined ||
        patch.dailyCardCount !== undefined ||
        patch.dailyTimeZone !== undefined ||
        patch.dailyUpdateTime !== undefined ||
        patch.compressionLevel !== undefined ||
        patch.groundingModel !== undefined ||
        patch.groundingReasoningEffort !== undefined ||
        patch.groundingStrategy !== undefined ||
        patch.imageStrategy !== undefined
    );
}

export function applyTopicEditorDraft(
    topic: AppTopicInfo,
    draft: TopicEditorDraft,
): AppTopicInfo {
    return {
        ...topic,
        promptTemplate: draft.promptTemplate,
        dailyCardCount: parseDailyCount(draft.dailyCardCount),
        dailyTimeZone: draft.dailyTimeZone,
        dailyUpdateTime: draft.dailyUpdateTime,
        compressionLevel: draft.compressionLevel,
        groundingModel: draft.groundingModel,
        groundingReasoningEffort: draft.groundingReasoningEffort,
        groundingStrategy: draft.groundingStrategy,
        imageStrategy: draft.imageStrategy,
    };
}
