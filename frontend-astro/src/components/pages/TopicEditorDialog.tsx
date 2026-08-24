import { useState } from "react";
import type { AppTopicInfo } from "@/generated/denpie_pb";
import { Button } from "@/components/ui/button";
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { t, tf } from "@/lib/i18n";
import {
    applyTopicEditorDraft,
    hasTopicEditorPatch,
    topicEditorDraft,
    topicEditorPatch,
    type TopicEditorDraft,
} from "@/lib/pages/topic-editor";
import { tipTypeLabel } from "@/lib/tip-type";
import { updateTopic } from "@/lib/api-v1/route-ops";
import { newIdempotencyKey } from "@/lib/api-v1/transport";
import { useToast } from "@/islands/toast-context";

const INHERIT = "inherit";

const COMPRESSION = ["light", "balanced", "strong", "ultra"] as const;
const STRATEGIES = [
    INHERIT,
    "factual",
    "create_and_ground",
    "agentic",
    "rag",
] as const;
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

function strategyLabel(value: string): string {
    switch (value) {
        case INHERIT:
            return t("grounding.topic_editor.inherit");
        case "factual":
            return t("settings.grounding.strategy_factual");
        case "create_and_ground":
            return t("settings.grounding.strategy_fact_check");
        case "agentic":
            return t("settings.grounding.strategy_agentic");
        case "rag":
            return t("settings.grounding.strategy_documents");
        default:
            return value;
    }
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

export function TopicEditorDialog({
    topic,
    busy,
    onClose,
    onSaved,
}: {
    topic: AppTopicInfo | null;
    busy: boolean;
    onClose: () => void;
    onSaved: (topic: AppTopicInfo) => Promise<void> | void;
}) {
    const toast = useToast();
    const [draft, setDraft] = useState<TopicEditorDraft | null>(null);
    const [saving, setSaving] = useState(false);
    const open = topic !== null;
    const shown = topic === null ? null : (draft ?? topicEditorDraft(topic));

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

    const save = async () => {
        if (topic === null) return;
        const next = draft ?? topicEditorDraft(topic);
        const patch = topicEditorPatch(topic, next);
        setSaving(true);
        try {
            if (hasTopicEditorPatch(patch)) {
                await updateTopic({
                    patch,
                    idempotencyKey: newIdempotencyKey(),
                });
            }
            await onSaved(applyTopicEditorDraft(topic, next));
            setDraft(null);
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
                if (!nextOpen && !saving) {
                    setDraft(null);
                    onClose();
                }
            }}
        >
            <DialogContent
                className="max-h-[90dvh] overflow-y-auto sm:max-w-xl"
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
                                <FieldLabel htmlFor="topic-prompt">
                                    {t("grounding.topic_editor.prompt")}
                                </FieldLabel>
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
                            </Field>
                            <div className="grid gap-3 sm:grid-cols-2">
                                <Field>
                                    <FieldLabel htmlFor="topic-daily-count">
                                        {t("grounding.topic_editor.daily_count")}
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
                                            "grounding.topic_editor.grounding_strategy",
                                        )}
                                    </FieldLabel>
                                    <Select
                                        value={storedStrategy(
                                            shown.groundingStrategy,
                                        )}
                                        onValueChange={(value) =>
                                            value !== null &&
                                            setField(
                                                "groundingStrategy",
                                                selectedStrategy(value),
                                            )
                                        }
                                    >
                                        <SelectTrigger className="w-full">
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            {STRATEGIES.map((value) => (
                                                <SelectItem
                                                    key={value}
                                                    value={value}
                                                >
                                                    {strategyLabel(value)}
                                                </SelectItem>
                                            ))}
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
                                        {t("grounding.topic_editor.update_time")}
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
                                disabled={saving}
                                onClick={onClose}
                            >
                                {t("common.close")}
                            </Button>
                            <Button
                                type="button"
                                disabled={busy || saving}
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
