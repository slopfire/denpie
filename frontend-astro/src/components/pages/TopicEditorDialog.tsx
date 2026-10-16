import { useEffect, useState } from "react";
import { create } from "@bufbuild/protobuf";
import {
    ExternalLinkIcon,
    RotateCcwIcon,
    SearchIcon,
    SparklesIcon,
} from "lucide-react";
import {
    GroundingSearchProfileSchema,
    type AppTopicInfo,
    type DocumentInfo,
    type PreviewGroundingResult,
} from "@/generated/denpie_pb";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { GroundingPolicyEditor } from "@/components/pages/GroundingPolicyEditor";
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog";
import {
    Field,
    FieldContent,
    FieldDescription,
    FieldGroup,
    FieldLabel,
    FieldLegend,
    FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemTitle,
} from "@/components/ui/item";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { t, tf } from "@/lib/i18n";
import {
    applyTopicEditorDraft,
    hasTopicEditorPatch,
    setTopicSourceAssignment,
    topicEditorDraft,
    topicEditorPatch,
    topicSourceIds,
    topicSourceMutations,
    type TopicEditorDraft,
} from "@/lib/pages/topic-editor";
import {
    applyPromptSuggestionToTopic,
    resetTopicPrompt,
} from "@/lib/pages/prompt-template";
import { tipTypeLabel } from "@/lib/tip-type";
import {
    attachDocumentTopic,
    detachDocumentTopic,
    enhancePromptTemplate,
    getSettings,
    previewGrounding,
    updateTopic,
} from "@/lib/api-v1/route-ops";
import { newIdempotencyKey } from "@/lib/api-v1/transport";
import { useToast } from "@/islands/toast-context";
import {
    legacyStrategyForPolicy,
    legacyVerificationForBehavior,
    parseDomainList,
    resolveGroundingPolicy,
    type GroundingPolicyDraft,
    type ResolvedGroundingPolicy,
} from "@/lib/grounding-policy";
import { settingsDraft } from "@/lib/pages/settings-page";
import { safeSourceUrl } from "@/lib/flow-detail-state";

const INHERIT = "inherit";

const COMPRESSION = ["light", "balanced", "strong", "ultra"] as const;
const IMAGE_STRATEGIES = [
    INHERIT,
    "none",
    "pool",
    "bing_html",
    "bing_playwright",
    "ddgs_text_og",
] as const;
const GROUNDING_REASONING_EFFORTS = [
    INHERIT,
    "none",
    "minimal",
    "low",
    "medium",
    "high",
    "xhigh",
] as const;

function compressionLabel(value: string): string {
    switch (value) {
        case "light":
            return t("common.light");
        case "balanced":
            return t("settings.compression.balanced");
        case "strong":
            return t("settings.compression.strong");
        case "ultra":
            return t("settings.compression.ultra");
        default:
            return value;
    }
}

function storedStrategy(value: string): string {
    return value === "" ? INHERIT : value;
}

function selectedStrategy(value: string): string {
    return value === INHERIT ? "" : value;
}

function imageStrategyLabel(value: string): string {
    switch (value) {
        case INHERIT:
            return t("grounding.topic_editor.inherit");
        case "none":
            return t("grounding.image_strategy.none");
        case "pool":
            return t("grounding.image_strategy.pool");
        case "bing_html":
            return t("grounding.image_strategy.bing_html");
        case "bing_playwright":
            return t("grounding.image_strategy.bing_playwright");
        case "ddgs_text_og":
            return t("grounding.image_strategy.ddgs_text_og");
        default:
            return value;
    }
}

function reasoningEffortLabel(value: string): string {
    switch (value) {
        case INHERIT:
            return t("grounding.topic_editor.inherit");
        case "none":
            return t("common.none");
        case "minimal":
            return t("settings.reasoning.minimal");
        case "low":
            return t("common.low");
        case "medium":
            return t("common.medium");
        case "high":
            return t("common.high");
        case "xhigh":
            return t("settings.reasoning.extra_high");
        default:
            return value;
    }
}

function sourceTypeLabel(sourceType: string): string {
    switch (sourceType) {
        case "document":
            return t("format.source_type.document");
        case "link":
            return t("format.source_type.link");
        default:
            return sourceType;
    }
}

function previewSourceLabel(sourceMode: string): string {
    switch (sourceMode) {
        case "model":
            return t("grounding.policy.source_model");
        case "web":
            return t("grounding.policy.source_web");
        case "documents":
            return t("grounding.policy.source_documents");
        case "web_and_documents":
            return t("grounding.policy.source_web_and_documents");
        default:
            return sourceMode;
    }
}

function previewBehaviorLabel(behavior: string): string {
    switch (behavior) {
        case "standard":
            return t("grounding.policy.behavior_standard");
        case "fact_check":
            return t("grounding.policy.behavior_fact_check");
        case "agentic":
            return t("grounding.policy.behavior_agentic");
        default:
            return behavior;
    }
}

interface TopicSourceDraft {
    topicId: bigint;
    sourceIds: bigint[];
}

type PreviewState =
    | { kind: "idle" }
    | { kind: "loading" }
    | { kind: "ready"; preview: PreviewGroundingResult }
    | { kind: "error"; message: string };

function editorPolicy(draft: TopicEditorDraft): GroundingPolicyDraft {
    return {
        sourceMode: draft.groundingSourceMode,
        behavior: draft.groundingBehavior,
        searchDepth: draft.groundingSearchDepth,
        freshness: draft.groundingFreshness,
        searchGuidance: draft.groundingSearchGuidance,
        includeDomains: draft.groundingIncludeDomains,
        failurePolicy: draft.groundingFailurePolicy,
    };
}

function inheritedPolicy(
    settings: ReturnType<typeof settingsDraft>,
): ResolvedGroundingPolicy {
    return {
        sourceMode: settings.groundingSourceMode,
        behavior: settings.groundingBehavior,
        searchDepth: settings.groundingSearchDepth,
        freshness: settings.groundingFreshness,
        searchGuidance: settings.groundingSearchGuidance,
        includeDomains: parseDomainList(settings.groundingIncludeDomains),
        failurePolicy: settings.groundingFailurePolicy,
    };
}

function errorText(error: unknown): string {
    return error instanceof Error ? error.message : t("toast.request_failed");
}

function GroundingPreviewPanel({ state }: { state: PreviewState }) {
    if (state.kind === "idle") return null;
    if (state.kind === "loading") {
        return (
            <Alert data-testid="grounding-preview-loading">
                <Spinner />
                <AlertTitle>{t("grounding.preview.loading")}</AlertTitle>
                <AlertDescription>
                    {t("grounding.preview.loading_description")}
                </AlertDescription>
            </Alert>
        );
    }
    if (state.kind === "error") {
        return (
            <Alert variant="destructive" data-testid="grounding-preview-error">
                <AlertTitle>{t("grounding.preview.failed")}</AlertTitle>
                <AlertDescription>{state.message}</AlertDescription>
            </Alert>
        );
    }

    const { preview } = state;
    return (
        <FieldSet data-testid="grounding-preview-ready">
            <FieldLegend>{t("grounding.preview.title")}</FieldLegend>
            <div className="flex flex-wrap gap-2">
                <Badge variant="secondary">
                    {tf("grounding.preview.backend", {
                        backend: preview.backend,
                    })}
                </Badge>
                <Badge variant="outline">
                    {tf("grounding.preview.source", {
                        source: previewSourceLabel(preview.groundingSourceMode),
                    })}
                </Badge>
                <Badge variant="outline">
                    {tf("grounding.preview.behavior", {
                        behavior: previewBehaviorLabel(
                            preview.groundingBehavior,
                        ),
                    })}
                </Badge>
            </div>
            {preview.queries.length === 0 ? null : (
                <FieldDescription>
                    {preview.queries.join(" · ")}
                </FieldDescription>
            )}
            {preview.warnings.map((warning) => (
                <Alert key={warning} variant="destructive">
                    <AlertTitle>{t("grounding.preview.warning")}</AlertTitle>
                    <AlertDescription>{warning}</AlertDescription>
                </Alert>
            ))}
            {preview.evidence.length === 0 ? (
                <FieldDescription>
                    {t("grounding.preview.no_evidence")}
                </FieldDescription>
            ) : (
                <ItemGroup>
                    {preview.evidence.map((evidence, index) => {
                        const href = safeSourceUrl(evidence.url);
                        const content = (
                            <>
                                <ItemContent>
                                    <ItemTitle>
                                        {evidence.title ||
                                            t("grounding.preview.untitled")}
                                    </ItemTitle>
                                    <ItemDescription>
                                        {evidence.excerpt ||
                                            evidence.sourceType}
                                    </ItemDescription>
                                </ItemContent>
                                {href === null ? null : (
                                    <ItemActions>
                                        <ExternalLinkIcon aria-hidden />
                                    </ItemActions>
                                )}
                            </>
                        );
                        return href === null ? (
                            <Item
                                key={`${evidence.sourceType}:${evidence.title}:${index}`}
                                variant="outline"
                                size="sm"
                            >
                                {content}
                            </Item>
                        ) : (
                            <Item
                                key={`${href}:${index}`}
                                variant="outline"
                                size="sm"
                                render={
                                    <a
                                        href={href}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                    />
                                }
                            >
                                {content}
                            </Item>
                        );
                    })}
                </ItemGroup>
            )}
        </FieldSet>
    );
}

export function TopicEditorDialog({
    topic,
    documents,
    busy,
    onClose,
    onSaved,
}: {
    topic: AppTopicInfo | null;
    documents: DocumentInfo[];
    busy: boolean;
    onClose: () => void;
    onSaved: (saved: {
        topic: AppTopicInfo;
        sourceIds: bigint[];
    }) => Promise<void> | void;
}) {
    const toast = useToast();
    const [draft, setDraft] = useState<TopicEditorDraft | null>(null);
    const [saving, setSaving] = useState(false);
    const [enhancing, setEnhancing] = useState(false);
    const [promptRationale, setPromptRationale] = useState("");
    const [groundingDefaults, setGroundingDefaults] =
        useState<ResolvedGroundingPolicy | null>(null);
    const [searchProvider, setSearchProvider] = useState("native");
    const [previewState, setPreviewState] = useState<PreviewState>({
        kind: "idle",
    });
    const [sourceDraft, setSourceDraft] = useState<TopicSourceDraft | null>(
        null,
    );
    const open = topic !== null;
    const shown = topic === null ? null : (draft ?? topicEditorDraft(topic));
    const shownSourceIds =
        topic === null
            ? []
            : sourceDraft?.topicId === topic.id
              ? sourceDraft.sourceIds
              : topicSourceIds(documents, topic.id);

    useEffect(() => {
        if (!open) return;
        let cancelled = false;
        void getSettings()
            .then(({ settings }) => {
                if (!cancelled) {
                    const defaults = settingsDraft(settings);
                    setGroundingDefaults(inheritedPolicy(defaults));
                    setSearchProvider(defaults.searchProvider);
                }
            })
            .catch(() => {
                if (!cancelled) setGroundingDefaults(null);
            });
        return () => {
            cancelled = true;
        };
    }, [open, topic?.id]);

    const setField = <Key extends keyof TopicEditorDraft>(
        key: Key,
        value: TopicEditorDraft[Key],
    ) => {
        if (topic === null) return;
        setDraft((current) => ({
            ...(current ?? topicEditorDraft(topic)),
            [key]: value,
        }));
    };

    const setPolicy = (policy: GroundingPolicyDraft) => {
        if (topic === null) return;
        setDraft((current) => {
            const next = current ?? topicEditorDraft(topic);
            return {
                ...next,
                groundingSourceMode: policy.sourceMode,
                groundingBehavior: policy.behavior,
                groundingSearchDepth: policy.searchDepth,
                groundingFreshness: policy.freshness,
                groundingSearchGuidance: policy.searchGuidance,
                groundingIncludeDomains: policy.includeDomains,
                groundingFailurePolicy: policy.failurePolicy,
                groundingStrategy:
                    policy.sourceMode === ""
                        ? ""
                        : legacyStrategyForPolicy({
                              sourceMode: policy.sourceMode,
                              behavior: policy.behavior || "standard",
                          }),
            };
        });
        setPreviewState({ kind: "idle" });
    };

    const runPreview = async () => {
        if (topic === null) return;
        const next = draft ?? topicEditorDraft(topic);
        const effective = resolveGroundingPolicy(
            editorPolicy(next),
            groundingDefaults ?? undefined,
        );
        setPreviewState({ kind: "loading" });
        try {
            const { preview } = await previewGrounding({
                topicId: topic.id,
                groundingSourceMode: effective.sourceMode,
                groundingBehavior: effective.behavior,
                groundingVerificationMode: legacyVerificationForBehavior(
                    effective.behavior,
                ),
                groundingSearchProfile: create(GroundingSearchProfileSchema, {
                    depth: effective.searchDepth,
                    freshness: effective.freshness,
                    guidance: effective.searchGuidance,
                    includeDomains: effective.includeDomains,
                    failurePolicy: effective.failurePolicy,
                }),
                documentIds: shownSourceIds,
            });
            setPreviewState({ kind: "ready", preview });
        } catch (error) {
            setPreviewState({ kind: "error", message: errorText(error) });
        }
    };

    const resetPrompt = async () => {
        if (topic === null) return;
        try {
            const { settings } = await getSettings();
            setDraft((current) =>
                resetTopicPrompt(
                    current ?? topicEditorDraft(topic),
                    settings.template,
                ),
            );
            setPromptRationale("");
        } catch (error) {
            toast.show(
                error instanceof Error
                    ? error.message
                    : t("toast.request_failed"),
                "error",
            );
        }
    };

    const enhancePrompt = async () => {
        if (topic === null) return;
        setEnhancing(true);
        try {
            const { suggestion } = await enhancePromptTemplate({
                topicId: topic.id,
            });
            setDraft((current) =>
                applyPromptSuggestionToTopic(
                    current ?? topicEditorDraft(topic),
                    suggestion,
                ),
            );
            setPromptRationale(suggestion.rationale);
            toast.show(t("grounding.topic_editor.prompt_enhanced"), "success");
        } catch (error) {
            toast.show(
                error instanceof Error
                    ? error.message
                    : t("toast.request_failed"),
                "error",
            );
        } finally {
            setEnhancing(false);
        }
    };

    const save = async () => {
        if (topic === null) return;
        const next = draft ?? topicEditorDraft(topic);
        const patch = topicEditorPatch(topic, next);
        const sourceMutations = topicSourceMutations(
            documents,
            topic.id,
            shownSourceIds,
        );
        setSaving(true);
        try {
            if (hasTopicEditorPatch(patch)) {
                await updateTopic({
                    patch,
                    idempotencyKey: newIdempotencyKey(),
                });
            }
            await Promise.all(
                sourceMutations.map((mutation) => {
                    const options = {
                        documentId: mutation.documentId,
                        topicId: topic.id,
                        idempotencyKey: newIdempotencyKey(),
                    };
                    switch (mutation.kind) {
                        case "attach":
                            return attachDocumentTopic(options);
                        case "detach":
                            return detachDocumentTopic(options);
                    }
                }),
            );
            await onSaved({
                topic: applyTopicEditorDraft(topic, next),
                sourceIds: shownSourceIds,
            });
            setDraft(null);
            setSourceDraft(null);
            setPromptRationale("");
            setPreviewState({ kind: "idle" });
            toast.show(t("grounding.topic_editor.saved"), "success");
        } catch (error) {
            toast.show(
                error instanceof Error
                    ? error.message
                    : t("toast.request_failed"),
                "error",
            );
        } finally {
            setSaving(false);
        }
    };

    return (
        <Dialog
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen && !saving && !enhancing) {
                    setDraft(null);
                    setSourceDraft(null);
                    setPromptRationale("");
                    setPreviewState({ kind: "idle" });
                    onClose();
                }
            }}
        >
            <DialogContent
                className="max-h-[calc(100dvh-1rem)] max-w-[calc(100%-1rem)] overflow-y-auto sm:max-h-[90dvh] sm:max-w-xl"
                data-testid="topic-editor"
            >
                {topic === null || shown === null ? null : (
                    <>
                        <DialogHeader>
                            <DialogTitle>
                                {tf("grounding.topic_editor.title", {
                                    name: topic.name,
                                })}
                            </DialogTitle>
                            <DialogDescription>
                                {tipTypeLabel(topic.tipcardType)}
                            </DialogDescription>
                        </DialogHeader>
                        <FieldGroup>
                            <Field>
                                <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                                    <FieldLabel htmlFor="topic-prompt">
                                        {t("grounding.topic_editor.prompt")}
                                    </FieldLabel>
                                    <div className="flex gap-2">
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={saving || enhancing}
                                            onClick={() => void resetPrompt()}
                                            data-testid="topic-prompt-reset"
                                        >
                                            <RotateCcwIcon data-icon="inline-start" />
                                            {t("common.reset")}
                                        </Button>
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={saving || enhancing}
                                            onClick={() => void enhancePrompt()}
                                            data-testid="topic-prompt-enhance"
                                        >
                                            {enhancing ? (
                                                <Spinner data-icon="inline-start" />
                                            ) : (
                                                <SparklesIcon data-icon="inline-start" />
                                            )}
                                            {enhancing
                                                ? t("common.enhancing")
                                                : t("common.enhance")}
                                        </Button>
                                    </div>
                                </div>
                                <Textarea
                                    id="topic-prompt"
                                    rows={5}
                                    value={shown.promptTemplate}
                                    onChange={(event) =>
                                        setField(
                                            "promptTemplate",
                                            event.target.value,
                                        )
                                    }
                                />
                                <FieldDescription>
                                    {promptRationale ||
                                        t(
                                            "grounding.topic_editor.prompt_description",
                                        )}
                                </FieldDescription>
                            </Field>
                            <GroundingPolicyEditor
                                value={editorPolicy(shown)}
                                inherited={groundingDefaults ?? undefined}
                                allowInherit
                                disabled={busy || saving || enhancing}
                                onChange={setPolicy}
                                searchProvider={searchProvider}
                                documentControls={
                                    <FieldSet>
                                        <FieldLegend>
                                            {t(
                                                "grounding.topic_editor.sources",
                                            )}
                                        </FieldLegend>
                                        <FieldDescription>
                                            {t(
                                                "grounding.topic_editor.sources_description",
                                            )}
                                        </FieldDescription>
                                        {documents.length === 0 ? (
                                            <FieldDescription>
                                                {t(
                                                    "grounding.topic_editor.sources_empty",
                                                )}
                                            </FieldDescription>
                                        ) : (
                                            <ScrollArea className="max-h-56">
                                                <FieldGroup className="gap-3 pr-3">
                                                    {documents.map(
                                                        (document) => {
                                                            const controlId = `topic-source-${document.id}`;
                                                            const assigned =
                                                                shownSourceIds.includes(
                                                                    document.id,
                                                                );
                                                            const disabled =
                                                                busy ||
                                                                saving ||
                                                                enhancing;
                                                            return (
                                                                <Field
                                                                    key={document.id.toString()}
                                                                    orientation="horizontal"
                                                                    data-disabled={
                                                                        disabled ||
                                                                        undefined
                                                                    }
                                                                >
                                                                    <FieldContent>
                                                                        <FieldLabel
                                                                            htmlFor={
                                                                                controlId
                                                                            }
                                                                        >
                                                                            {
                                                                                document.title
                                                                            }
                                                                        </FieldLabel>
                                                                        <FieldDescription>
                                                                            {sourceTypeLabel(
                                                                                document.sourceType,
                                                                            )}
                                                                        </FieldDescription>
                                                                    </FieldContent>
                                                                    <Switch
                                                                        id={
                                                                            controlId
                                                                        }
                                                                        checked={
                                                                            assigned
                                                                        }
                                                                        disabled={
                                                                            disabled
                                                                        }
                                                                        onCheckedChange={(
                                                                            checked,
                                                                        ) =>
                                                                            setSourceDraft(
                                                                                {
                                                                                    topicId:
                                                                                        topic.id,
                                                                                    sourceIds:
                                                                                        setTopicSourceAssignment(
                                                                                            shownSourceIds,
                                                                                            document.id,
                                                                                            checked,
                                                                                        ),
                                                                                },
                                                                            )
                                                                        }
                                                                    />
                                                                </Field>
                                                            );
                                                        },
                                                    )}
                                                </FieldGroup>
                                            </ScrollArea>
                                        )}
                                    </FieldSet>
                                }
                            />
                            <GroundingPreviewPanel state={previewState} />
                            <div className="grid gap-3 sm:grid-cols-2">
                                <Field>
                                    <FieldLabel htmlFor="topic-daily-count">
                                        {t(
                                            "grounding.topic_editor.daily_count",
                                        )}
                                    </FieldLabel>
                                    <Input
                                        id="topic-daily-count"
                                        type="number"
                                        min="0"
                                        value={shown.dailyCardCount}
                                        onChange={(event) =>
                                            setField(
                                                "dailyCardCount",
                                                event.target.value,
                                            )
                                        }
                                    />
                                </Field>
                                <Field>
                                    <FieldLabel>
                                        {t(
                                            "grounding.topic_editor.compression",
                                        )}
                                    </FieldLabel>
                                    <Select
                                        value={shown.compressionLevel}
                                        onValueChange={(value) =>
                                            value !== null &&
                                            setField("compressionLevel", value)
                                        }
                                    >
                                        <SelectTrigger className="w-full">
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            {COMPRESSION.map((value) => (
                                                <SelectItem
                                                    key={value}
                                                    value={value}
                                                >
                                                    {compressionLabel(value)}
                                                </SelectItem>
                                            ))}
                                        </SelectContent>
                                    </Select>
                                </Field>
                                <Field>
                                    <FieldLabel htmlFor="topic-grounding-model">
                                        {t(
                                            "grounding.topic_editor.grounding_model",
                                        )}
                                    </FieldLabel>
                                    <Input
                                        id="topic-grounding-model"
                                        value={shown.groundingModel}
                                        placeholder={t(
                                            "grounding.topic_editor.inherit",
                                        )}
                                        onChange={(event) =>
                                            setField(
                                                "groundingModel",
                                                event.target.value,
                                            )
                                        }
                                    />
                                </Field>
                                <Field>
                                    <FieldLabel>
                                        {t(
                                            "grounding.topic_editor.grounding_reasoning",
                                        )}
                                    </FieldLabel>
                                    <Select
                                        value={storedStrategy(
                                            shown.groundingReasoningEffort,
                                        )}
                                        onValueChange={(value) =>
                                            value !== null &&
                                            setField(
                                                "groundingReasoningEffort",
                                                selectedStrategy(value),
                                            )
                                        }
                                    >
                                        <SelectTrigger className="w-full">
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            {GROUNDING_REASONING_EFFORTS.map(
                                                (value) => (
                                                    <SelectItem
                                                        key={value}
                                                        value={value}
                                                    >
                                                        {reasoningEffortLabel(
                                                            value,
                                                        )}
                                                    </SelectItem>
                                                ),
                                            )}
                                        </SelectContent>
                                    </Select>
                                </Field>
                                <Field>
                                    <FieldLabel>
                                        {t(
                                            "grounding.topic_editor.image_strategy",
                                        )}
                                    </FieldLabel>
                                    <Select
                                        value={storedStrategy(
                                            shown.imageStrategy,
                                        )}
                                        onValueChange={(value) =>
                                            value !== null &&
                                            setField(
                                                "imageStrategy",
                                                selectedStrategy(value),
                                            )
                                        }
                                    >
                                        <SelectTrigger className="w-full">
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            {IMAGE_STRATEGIES.map((value) => (
                                                <SelectItem
                                                    key={value}
                                                    value={value}
                                                >
                                                    {imageStrategyLabel(value)}
                                                </SelectItem>
                                            ))}
                                        </SelectContent>
                                    </Select>
                                </Field>
                                <Field>
                                    <FieldLabel htmlFor="topic-timezone">
                                        {t("grounding.topic_editor.time_zone")}
                                    </FieldLabel>
                                    <Input
                                        id="topic-timezone"
                                        value={shown.dailyTimeZone}
                                        onChange={(event) =>
                                            setField(
                                                "dailyTimeZone",
                                                event.target.value,
                                            )
                                        }
                                    />
                                </Field>
                                <Field>
                                    <FieldLabel htmlFor="topic-update-time">
                                        {t(
                                            "grounding.topic_editor.update_time",
                                        )}
                                    </FieldLabel>
                                    <Input
                                        id="topic-update-time"
                                        type="time"
                                        value={shown.dailyUpdateTime}
                                        onChange={(event) =>
                                            setField(
                                                "dailyUpdateTime",
                                                event.target.value,
                                            )
                                        }
                                    />
                                </Field>
                            </div>
                        </FieldGroup>
                        <DialogFooter>
                            <Button
                                type="button"
                                variant="outline"
                                disabled={saving || enhancing}
                                onClick={onClose}
                            >
                                {t("common.close")}
                            </Button>
                            <Button
                                type="button"
                                variant="outline"
                                disabled={
                                    busy ||
                                    saving ||
                                    enhancing ||
                                    previewState.kind === "loading"
                                }
                                onClick={() => void runPreview()}
                                data-testid="grounding-preview"
                            >
                                {previewState.kind === "loading" ? (
                                    <Spinner data-icon="inline-start" />
                                ) : (
                                    <SearchIcon data-icon="inline-start" />
                                )}
                                {t("grounding.preview.action")}
                            </Button>
                            <Button
                                type="button"
                                disabled={busy || saving || enhancing}
                                onClick={() => void save()}
                            >
                                {saving
                                    ? t("common.saving")
                                    : t("grounding.topic_editor.save")}
                            </Button>
                        </DialogFooter>
                    </>
                )}
            </DialogContent>
        </Dialog>
    );
}
