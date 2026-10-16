import { create } from "@bufbuild/protobuf";
import {
    GroundingSearchProfileSchema,
    UpdateTopicRequestSchema,
    type AppTopicInfo,
    type DocumentInfo,
    type UpdateTopicRequest,
} from "@/generated/denpie_pb";
import {
    formatDomainList,
    behaviorFromLegacyVerification,
    hasGroundingSearchOverride,
    legacyGroundingPolicy,
    legacyStrategyForPolicy,
    legacyVerificationForBehavior,
    parseDomainList,
    type GroundingFailurePolicy,
    type GroundingFreshness,
    type GroundingSearchDepth,
    type GroundingSourceMode,
    type GroundingBehavior,
} from "@/lib/grounding-policy";

type TopicSourceDocument = Pick<DocumentInfo, "id" | "topicIds">;

export type TopicSourceMutation =
    | { kind: "attach"; documentId: bigint }
    | { kind: "detach"; documentId: bigint };

export interface TopicEditorDraft {
    promptTemplate: string;
    dailyCardCount: string;
    dailyTimeZone: string;
    dailyUpdateTime: string;
    compressionLevel: string;
    groundingModel: string;
    groundingReasoningEffort: string;
    groundingStrategy: string;
    groundingSourceMode: GroundingSourceMode | "";
    groundingBehavior: GroundingBehavior | "";
    groundingSearchDepth: GroundingSearchDepth | "";
    groundingFreshness: GroundingFreshness | "";
    groundingSearchGuidance: string;
    groundingIncludeDomains: string;
    groundingFailurePolicy: GroundingFailurePolicy | "";
    imageStrategy: string;
}

export function topicEditorDraft(topic: AppTopicInfo): TopicEditorDraft {
    const legacy =
        topic.groundingStrategy === ""
            ? null
            : legacyGroundingPolicy(topic.groundingStrategy);
    const profile = topic.groundingSearchProfile;
    return {
        promptTemplate: topic.promptTemplate,
        dailyCardCount: topic.dailyCardCount.toString(),
        dailyTimeZone: topic.dailyTimeZone,
        dailyUpdateTime: topic.dailyUpdateTime,
        compressionLevel: topic.compressionLevel,
        groundingModel: topic.groundingModel,
        groundingReasoningEffort: topic.groundingReasoningEffort,
        groundingStrategy: topic.groundingStrategy,
        groundingSourceMode:
            (topic.groundingSourceMode as GroundingSourceMode) ||
            legacy?.sourceMode ||
            "",
        groundingBehavior:
            (topic.groundingBehavior as GroundingBehavior) ||
            (topic.groundingVerificationMode
                ? behaviorFromLegacyVerification(topic.groundingVerificationMode)
                : legacy?.behavior) ||
            "",
        groundingSearchDepth: (profile?.depth as GroundingSearchDepth) || "",
        groundingFreshness: (profile?.freshness as GroundingFreshness) || "",
        groundingSearchGuidance: profile?.guidance ?? "",
        groundingIncludeDomains: formatDomainList(
            profile?.includeDomains ?? [],
        ),
        groundingFailurePolicy:
            (profile?.failurePolicy as GroundingFailurePolicy) || "",
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
    const currentBehavior = topicEditorDraft(topic).groundingBehavior;
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
    if (draft.groundingSourceMode !== topic.groundingSourceMode)
        patch.groundingSourceMode = draft.groundingSourceMode;
    if (draft.groundingBehavior !== currentBehavior) {
        patch.groundingBehavior = draft.groundingBehavior;
        patch.groundingVerificationMode = draft.groundingBehavior
            ? legacyVerificationForBehavior(draft.groundingBehavior)
            : "";
    }
    const currentProfile = topic.groundingSearchProfile;
    const nextPolicy = {
        sourceMode: draft.groundingSourceMode,
        behavior: draft.groundingBehavior,
        searchDepth: draft.groundingSearchDepth,
        freshness: draft.groundingFreshness,
        searchGuidance: draft.groundingSearchGuidance,
        includeDomains: draft.groundingIncludeDomains,
        failurePolicy: draft.groundingFailurePolicy,
    };
    if (
        draft.groundingSearchDepth !== (currentProfile?.depth ?? "") ||
        draft.groundingFreshness !== (currentProfile?.freshness ?? "") ||
        draft.groundingSearchGuidance !== (currentProfile?.guidance ?? "") ||
        draft.groundingIncludeDomains !==
            formatDomainList(currentProfile?.includeDomains ?? []) ||
        draft.groundingFailurePolicy !== (currentProfile?.failurePolicy ?? "")
    ) {
        if (hasGroundingSearchOverride(nextPolicy)) {
            patch.groundingSearchProfile = create(
                GroundingSearchProfileSchema,
                {
                    depth: draft.groundingSearchDepth,
                    freshness: draft.groundingFreshness,
                    guidance: draft.groundingSearchGuidance,
                    includeDomains: parseDomainList(
                        draft.groundingIncludeDomains,
                    ),
                    failurePolicy: draft.groundingFailurePolicy,
                },
            );
        } else if (currentProfile !== undefined) {
            patch.clearGroundingSearchProfile = true;
        }
    }
    if (
        draft.groundingSourceMode !== topic.groundingSourceMode ||
        draft.groundingBehavior !== currentBehavior
    ) {
        patch.groundingStrategy =
            draft.groundingSourceMode === ""
                ? ""
                : legacyStrategyForPolicy({
                      sourceMode: draft.groundingSourceMode,
                      behavior: draft.groundingBehavior || "standard",
                  });
    }
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
        patch.groundingSourceMode !== undefined ||
        patch.groundingVerificationMode !== undefined ||
        patch.groundingBehavior !== undefined ||
        patch.groundingSearchProfile !== undefined ||
        patch.clearGroundingSearchProfile !== undefined ||
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
        groundingSourceMode: draft.groundingSourceMode,
        groundingBehavior: draft.groundingBehavior,
        groundingVerificationMode: draft.groundingBehavior
            ? legacyVerificationForBehavior(draft.groundingBehavior)
            : "",
        groundingSearchProfile: hasGroundingSearchOverride({
            sourceMode: draft.groundingSourceMode,
            behavior: draft.groundingBehavior,
            searchDepth: draft.groundingSearchDepth,
            freshness: draft.groundingFreshness,
            searchGuidance: draft.groundingSearchGuidance,
            includeDomains: draft.groundingIncludeDomains,
            failurePolicy: draft.groundingFailurePolicy,
        })
            ? create(GroundingSearchProfileSchema, {
                  depth: draft.groundingSearchDepth,
                  freshness: draft.groundingFreshness,
                  guidance: draft.groundingSearchGuidance,
                  includeDomains: parseDomainList(
                      draft.groundingIncludeDomains,
                  ),
                  failurePolicy: draft.groundingFailurePolicy,
              })
            : undefined,
        imageStrategy: draft.imageStrategy,
    };
}

export function topicSourceIds(
    documents: readonly TopicSourceDocument[],
    topicId: bigint,
): bigint[] {
    return documents
        .filter((document) => document.topicIds.includes(topicId))
        .map((document) => document.id);
}

export function setTopicSourceAssignment(
    sourceIds: readonly bigint[],
    documentId: bigint,
    assigned: boolean,
): bigint[] {
    if (assigned) {
        return sourceIds.includes(documentId)
            ? [...sourceIds]
            : [...sourceIds, documentId];
    }
    return sourceIds.filter((sourceId) => sourceId !== documentId);
}

export function topicSourceMutations(
    documents: readonly TopicSourceDocument[],
    topicId: bigint,
    sourceIds: readonly bigint[],
): TopicSourceMutation[] {
    return documents.flatMap((document) => {
        const assigned = document.topicIds.includes(topicId);
        const selected = sourceIds.includes(document.id);
        if (assigned === selected) return [];
        return [
            selected
                ? { kind: "attach", documentId: document.id }
                : { kind: "detach", documentId: document.id },
        ];
    });
}

export function applyTopicSourceIds(
    documents: readonly DocumentInfo[],
    topicId: bigint,
    sourceIds: readonly bigint[],
): DocumentInfo[] {
    return documents.map((document) => {
        const topicIds = setTopicSourceAssignment(
            document.topicIds,
            topicId,
            sourceIds.includes(document.id),
        );
        const unchanged =
            topicIds.length === document.topicIds.length &&
            topicIds.every((id, index) => id === document.topicIds[index]);
        return unchanged ? document : { ...document, topicIds };
    });
}
