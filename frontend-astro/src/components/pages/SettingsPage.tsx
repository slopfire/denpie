import { useCallback, useEffect, useRef, useState } from "react";
import {
    CheckCircle2Icon,
    CircleAlertIcon,
    LoaderCircleIcon,
    SaveIcon,
    SparklesIcon,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card";
import {
    Field,
    FieldContent,
    FieldDescription,
    FieldGroup,
    FieldLabel,
    FieldTitle,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
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
import { newIdempotencyKey } from "@/lib/api-v1/transport";
import { t, tf } from "@/lib/i18n";
import {
    getSettings,
    testVisionModel,
    updateSettings,
} from "@/lib/api-v1/route-ops";
import {
    fetchAutoupdateStatus,
    isAutoupdateActive,
    triggerAutoupdate,
    type AutoupdateStatus,
} from "@/lib/dashboard-session";
import { useToast } from "@/islands/toast-context";
import { useViewRefresh } from "@/islands/use-view-refresh";
import type { MessageKey } from "@/lib/i18n";
import {
    hasSettingsPatch,
    settingsDraft,
    settingsPatch,
    settingsPatchCount,
} from "@/lib/pages/settings-page";
import type { SettingsDraft } from "@/lib/pages/settings-page";

type Feedback =
    | { kind: "idle" }
    | { kind: "loading"; message: string }
    | { kind: "saving"; message: string }
    | { kind: "success"; message: string }
    | { kind: "error"; message: string };

interface Option {
    value: string;
    label: string;
}

const REASONING_OPTIONS: readonly Option[] = [
    { value: "none", label: t("common.none") },
    { value: "minimal", label: t("settings.reasoning.minimal") },
    { value: "low", label: t("common.low") },
    { value: "medium", label: t("common.medium") },
    { value: "high", label: t("common.high") },
    { value: "xhigh", label: t("settings.reasoning.extra_high") },
];

const GROUNDING_REASONING_OPTIONS: readonly Option[] = [
    { value: "", label: t("settings.use_llm_setting") },
    ...REASONING_OPTIONS,
];

function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : t("toast.request_failed");
}

function SelectField({
    id,
    label,
    description,
    value,
    options,
    disabled,
    onValueChange,
    className,
}: {
    id: string;
    label: string;
    description?: string;
    value: string;
    options: readonly Option[];
    disabled: boolean;
    onValueChange: (value: string) => void;
    className?: string;
}) {
    return (
        <Field className={className}>
            <FieldLabel htmlFor={id}>{label}</FieldLabel>
            <Select
                value={value}
                disabled={disabled}
                onValueChange={(next) => onValueChange(next ?? "")}
            >
                <SelectTrigger id={id} className="w-full">
                    <SelectValue />
                </SelectTrigger>
                <SelectContent>
                    {options.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                            {option.label}
                        </SelectItem>
                    ))}
                </SelectContent>
            </Select>
            {description === undefined ? null : (
                <FieldDescription>{description}</FieldDescription>
            )}
        </Field>
    );
}

/**
 * Staged, explicit-save settings. It sends only the fields changed from the
 * latest settings snapshot, with a new idempotency key for each intentional
 * save.
 */
const AUTOUPDATE_PHASE_KEYS = {
    active: "settings.admin.phase.active",
    baseline: "settings.admin.phase.baseline",
    checking: "settings.admin.phase.checking",
    cloning: "settings.admin.phase.cloning",
    compiling: "settings.admin.phase.compiling",
    current: "settings.admin.phase.current",
    disabled: "settings.admin.phase.disabled",
    failed: "settings.admin.phase.failed",
    idle: "settings.admin.phase.idle",
    installing: "settings.admin.phase.installing",
    invalid: "settings.admin.phase.invalid",
    preparing: "settings.admin.phase.preparing",
    pulling: "settings.admin.phase.pulling",
    queued: "settings.admin.phase.queued",
    restarting: "settings.admin.phase.restarting",
    running: "settings.admin.phase.running",
    starting: "settings.admin.phase.starting",
} as const satisfies Record<string, MessageKey>;

function autoupdatePhaseLabel(phase: string): string {
    const key = AUTOUPDATE_PHASE_KEYS[phase as keyof typeof AUTOUPDATE_PHASE_KEYS];
    return key === undefined ? t("settings.admin.phase.unknown") : t(key);
}

function SaveChangesButton({
    saving,
    changed,
    onSave,
}: {
    saving: boolean;
    changed: boolean;
    onSave: () => void;
}) {
    return (
        <Button
            type="button"
            onClick={() => void onSave()}
            disabled={saving || !changed}
        >
            {saving ? (
                <Spinner data-icon="inline-start" />
            ) : (
                <SaveIcon data-icon="inline-start" />
            )}
            {saving ? t("settings.saving") : t("settings.save_changes")}
        </Button>
    );
}

export function SettingsPage({
    active = true,
    isAdmin = false,
}: {
    active?: boolean;
    isAdmin?: boolean;
}) {
    const toast = useToast();
    const [baseline, setBaseline] = useState<SettingsDraft | null>(null);
    const [draft, setDraft] = useState<SettingsDraft | null>(null);
    const [updateStatus, setUpdateStatus] = useState<AutoupdateStatus | null>(
        null,
    );
    const [feedback, setFeedback] = useState<Feedback>({
        kind: "loading",
        message: t("settings.loading"),
    });
    const [visionFeedback, setVisionFeedback] = useState<Feedback>({
        kind: "idle",
    });

    const loadSettings = useCallback(async () => {
        setFeedback({ kind: "loading", message: t("settings.loading") });
        try {
            const { settings } = await getSettings();
            const next = settingsDraft(settings);
            setBaseline(next);
            setDraft(next);
            setFeedback({ kind: "idle" });
        } catch (error) {
            setFeedback({ kind: "error", message: errorMessage(error) });
        }
    }, []);

    const refreshIfClean = useCallback(() => {
        if (dirtyRef.current) return;
        void loadSettings();
    }, [loadSettings]);
    useViewRefresh(active, refreshIfClean);

    useEffect(() => {
        if (!isAdmin || !active) return;
        let cancelled = false;
        const tick = () => {
            void fetchAutoupdateStatus()
                .then((status) => {
                    if (!cancelled) setUpdateStatus(status);
                })
                .catch(() => undefined);
        };
        tick();
        const interval = window.setInterval(tick, 4000);
        return () => {
            cancelled = true;
            window.clearInterval(interval);
        };
    }, [isAdmin, active, updateStatus?.phase]);

    useEffect(() => {
        if (draft === null || typeof document === "undefined") return;
        const root = document.documentElement;
        root.dataset.theme = draft.colorScheme;
        root.dataset.transparency = draft.transparency;
        root.dataset.blurIntensity = draft.blurIntensity;
        window.localStorage.setItem(
            "denpie-appearance",
            JSON.stringify({
                theme: draft.colorScheme,
                transparency: draft.transparency,
                blur: draft.blurIntensity,
            }),
        );
    }, [draft]);

    const dirtyRef = useRef(false);
    const setField = useCallback(
        <Key extends keyof SettingsDraft>(
            key: Key,
            value: SettingsDraft[Key],
        ) => {
            dirtyRef.current = true;
            setDraft((current) =>
                current === null ? current : { ...current, [key]: value },
            );
            if (feedback.kind === "success" || feedback.kind === "error") {
                setFeedback({ kind: "idle" });
            }
        },
        [feedback.kind],
    );

    const save = useCallback(async () => {
        if (baseline === null || draft === null) return;
        const patch = settingsPatch(baseline, draft);
        if (!hasSettingsPatch(patch)) {
            setFeedback({ kind: "success", message: t("settings.no_changes") });
            return;
        }

        const fieldCount = settingsPatchCount(patch);
        setFeedback({ kind: "saving", message: t("settings.saving") });
        try {
            await updateSettings({
                patch,
                idempotencyKey: newIdempotencyKey(),
            });
            dirtyRef.current = false;
            setBaseline(draft);
            setFeedback({
                kind: "success",
                message: tf("settings.saved_count", {
                    count: fieldCount,
                    suffix: fieldCount === 1 ? "" : "s",
                }),
            });
        } catch (error) {
            setFeedback({ kind: "error", message: errorMessage(error) });
        }
    }, [baseline, draft]);

    const testVision = useCallback(async () => {
        setVisionFeedback({
            kind: "saving",
            message: t("settings.vision.testing"),
        });
        try {
            const { result } = await testVisionModel();
            setVisionFeedback({
                kind: result.ok ? "success" : "error",
                message: tf("settings.vision.result", {
                    model: result.model,
                    message: result.message,
                }),
            });
        } catch (error) {
            setVisionFeedback({ kind: "error", message: errorMessage(error) });
        }
    }, []);

    if (draft === null || baseline === null) {
        return (
            <section className="mx-auto w-full max-w-6xl space-y-4">
                <div>
                    <h1 className="text-2xl font-semibold tracking-tight">
                        {t("settings.title")}
                    </h1>
                    <p className="text-sm text-muted-foreground">
                        {t("settings.loading_description")}
                    </p>
                </div>
                {feedback.kind === "error" ? (
                    <Alert variant="destructive">
                        <CircleAlertIcon />
                        <AlertTitle>{t("settings.load_failed")}</AlertTitle>
                        <AlertDescription className="flex items-center justify-between gap-3">
                            <span>{feedback.message}</span>
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={() => void loadSettings()}
                            >
                                {t("common.retry")}
                            </Button>
                        </AlertDescription>
                    </Alert>
                ) : (
                    <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Spinner />
                        {t("settings.loading")}
                    </div>
                )}
            </section>
        );
    }

    const saving = feedback.kind === "saving";
    const visionTesting = visionFeedback.kind === "saving";
    const changed = hasSettingsPatch(settingsPatch(baseline, draft));

    return (
        <section className="mx-auto w-full max-w-6xl space-y-5">
            <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
                <div>
                    <h1 className="text-2xl font-semibold tracking-tight">
                        {t("settings.title")}
                    </h1>
                    <p className="text-sm text-muted-foreground">
                        {t("settings.unsaved_description")}
                    </p>
                </div>
                <SaveChangesButton
                    saving={saving}
                    changed={changed}
                    onSave={() => void save()}
                />
            </div>

            {feedback.kind === "success" || feedback.kind === "error" ? (
                <Alert
                    variant={
                        feedback.kind === "error" ? "destructive" : "default"
                    }
                >
                    {feedback.kind === "error" ? (
                        <CircleAlertIcon />
                    ) : (
                        <CheckCircle2Icon />
                    )}
                    <AlertTitle>
                        {feedback.kind === "error"
                            ? t("settings.save_failed")
                            : t("settings.saved")}
                    </AlertTitle>
                    <AlertDescription>{feedback.message}</AlertDescription>
                </Alert>
            ) : null}

            <div className="flex flex-col gap-5">
                <section aria-labelledby="settings-models-heading">
                    <Card>
                        <CardHeader>
                            <CardTitle id="settings-models-heading">
                                {t("settings.models.title")}
                            </CardTitle>
                            <CardDescription>
                                {t("settings.models.description")}
                            </CardDescription>
                        </CardHeader>
                        <CardContent>
                            <FieldGroup>
                                <div className="grid gap-4 md:grid-cols-2">
                                    <Field>
                                        <FieldLabel htmlFor="settings-model">
                                            {t("settings.models.llm_model")}
                                        </FieldLabel>
                                        <Input
                                            id="settings-model"
                                            value={draft.model}
                                            onChange={(event) =>
                                                setField(
                                                    "model",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                    <Field>
                                        <FieldLabel htmlFor="settings-compress-model">
                                            {t(
                                                "settings.models.compression_model",
                                            )}
                                        </FieldLabel>
                                        <Input
                                            id="settings-compress-model"
                                            value={draft.compressModel}
                                            onChange={(event) =>
                                                setField(
                                                    "compressModel",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                    <SelectField
                                        id="settings-reasoning"
                                        label={t(
                                            "settings.models.llm_reasoning",
                                        )}
                                        value={draft.reasoningEffort}
                                        options={REASONING_OPTIONS}
                                        disabled={saving}
                                        onValueChange={(value) =>
                                            setField("reasoningEffort", value)
                                        }
                                    />
                                    <SelectField
                                        id="settings-compress-reasoning"
                                        label={t(
                                            "settings.models.compression_reasoning",
                                        )}
                                        value={draft.compressReasoningEffort}
                                        options={GROUNDING_REASONING_OPTIONS}
                                        disabled={saving}
                                        onValueChange={(value) =>
                                            setField(
                                                "compressReasoningEffort",
                                                value,
                                            )
                                        }
                                    />
                                    <SelectField
                                        id="settings-compression-level"
                                        className="md:col-span-2"
                                        label={t(
                                            "settings.models.compression_level",
                                        )}
                                        value={draft.compressionLevel}
                                        options={[
                                            {
                                                value: "light",
                                                label: t("common.light"),
                                            },
                                            {
                                                value: "balanced",
                                                label: t(
                                                    "settings.compression.balanced",
                                                ),
                                            },
                                            {
                                                value: "strong",
                                                label: t(
                                                    "settings.compression.strong",
                                                ),
                                            },
                                            {
                                                value: "ultra",
                                                label: t(
                                                    "settings.compression.ultra",
                                                ),
                                            },
                                        ]}
                                        disabled={saving}
                                        onValueChange={(value) =>
                                            setField("compressionLevel", value)
                                        }
                                    />
                                </div>
                                <Field>
                                    <FieldLabel htmlFor="settings-template">
                                        {t("settings.models.prompt_template")}
                                    </FieldLabel>
                                    <Textarea
                                        id="settings-template"
                                        value={draft.template}
                                        className="min-h-56 font-mono text-sm"
                                        onChange={(event) =>
                                            setField(
                                                "template",
                                                event.target.value,
                                            )
                                        }
                                    />
                                </Field>
                                <div className="grid gap-4 md:grid-cols-2">
                                    <Field>
                                        <FieldLabel htmlFor="settings-api-key">
                                            {t("settings.models.llm_api_key")}
                                        </FieldLabel>
                                        <Input
                                            id="settings-api-key"
                                            type="password"
                                            autoComplete="off"
                                            value={draft.apiKey}
                                            onChange={(event) =>
                                                setField(
                                                    "apiKey",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                    <Field>
                                        <FieldLabel htmlFor="settings-base-url">
                                            {t("settings.models.llm_base_url")}
                                        </FieldLabel>
                                        <Input
                                            id="settings-base-url"
                                            type="url"
                                            value={draft.baseUrl}
                                            onChange={(event) =>
                                                setField(
                                                    "baseUrl",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                    <Field className="md:col-span-2">
                                        <FieldLabel htmlFor="settings-compress-base-url">
                                            {t(
                                                "settings.models.compression_base_url",
                                            )}
                                        </FieldLabel>
                                        <Input
                                            id="settings-compress-base-url"
                                            type="url"
                                            value={draft.compressBaseUrl}
                                            onChange={(event) =>
                                                setField(
                                                    "compressBaseUrl",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                </div>
                            </FieldGroup>
                        </CardContent>
                    </Card>
                </section>

                <section aria-labelledby="settings-grounding-heading">
                    <Card>
                        <CardHeader>
                            <CardTitle id="settings-grounding-heading">
                                {t("settings.grounding.title")}
                            </CardTitle>
                            <CardDescription>
                                {t("settings.grounding.description")}
                            </CardDescription>
                        </CardHeader>
                        <CardContent>
                            <FieldGroup>
                                <div className="grid gap-4 md:grid-cols-2">
                                    <Field>
                                        <FieldLabel htmlFor="settings-grounding-model">
                                            {t("settings.grounding.model")}
                                        </FieldLabel>
                                        <Input
                                            id="settings-grounding-model"
                                            value={draft.groundingModel}
                                            placeholder={t(
                                                "settings.use_llm_model",
                                            )}
                                            onChange={(event) =>
                                                setField(
                                                    "groundingModel",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                    <SelectField
                                        id="settings-grounding-reasoning"
                                        label={t(
                                            "settings.grounding.reasoning",
                                        )}
                                        value={draft.groundingReasoningEffort}
                                        options={GROUNDING_REASONING_OPTIONS}
                                        disabled={saving}
                                        onValueChange={(value) =>
                                            setField(
                                                "groundingReasoningEffort",
                                                value,
                                            )
                                        }
                                    />
                                    <SelectField
                                        id="settings-grounding-strategy"
                                        label={t("settings.grounding.strategy")}
                                        value={draft.groundingStrategy}
                                        options={[
                                            {
                                                value: "factual",
                                                label: t(
                                                    "settings.grounding.strategy_factual",
                                                ),
                                            },
                                            {
                                                value: "create_and_ground",
                                                label: t(
                                                    "settings.grounding.strategy_fact_check",
                                                ),
                                            },
                                            {
                                                value: "agentic",
                                                label: t(
                                                    "settings.grounding.strategy_agentic",
                                                ),
                                            },
                                            {
                                                value: "rag",
                                                label: t(
                                                    "settings.grounding.strategy_documents",
                                                ),
                                            },
                                        ]}
                                        disabled={saving}
                                        onValueChange={(value) =>
                                            setField("groundingStrategy", value)
                                        }
                                    />
                                    <SelectField
                                        id="settings-image-strategy"
                                        label={t(
                                            "settings.grounding.image_strategy",
                                        )}
                                        value={draft.imageStrategy}
                                        options={[
                                            {
                                                value: "none",
                                                label: t(
                                                    "settings.images.none",
                                                ),
                                            },
                                            {
                                                value: "pool",
                                                label: t(
                                                    "settings.images.pool",
                                                ),
                                            },
                                            {
                                                value: "bing_html",
                                                label: t(
                                                    "settings.images.bing_metadata",
                                                ),
                                            },
                                            {
                                                value: "bing_playwright",
                                                label: t(
                                                    "settings.images.bing_playwright",
                                                ),
                                            },
                                            {
                                                value: "ddgs_text_og",
                                                label: t(
                                                    "settings.images.ddgs_page",
                                                ),
                                            },
                                        ]}
                                        disabled={saving}
                                        onValueChange={(value) =>
                                            setField("imageStrategy", value)
                                        }
                                    />
                                    <SelectField
                                        id="settings-search-provider"
                                        label={t(
                                            "settings.grounding.search_provider",
                                        )}
                                        value={draft.searchProvider}
                                        options={[
                                            {
                                                value: "tavily",
                                                label: t(
                                                    "settings.providers.tavily",
                                                ),
                                            },
                                            {
                                                value: "firecrawl",
                                                label: t(
                                                    "settings.providers.firecrawl",
                                                ),
                                            },
                                        ]}
                                        disabled={saving}
                                        onValueChange={(value) =>
                                            setField("searchProvider", value)
                                        }
                                    />
                                    <SelectField
                                        id="settings-scrape-provider"
                                        label={t(
                                            "settings.grounding.link_scraper",
                                        )}
                                        value={draft.scrapeProvider}
                                        options={[
                                            {
                                                value: "scrapling",
                                                label: t(
                                                    "settings.scraper.scrapling_local",
                                                ),
                                            },
                                            {
                                                value: "firecrawl",
                                                label: t(
                                                    "settings.scraper.firecrawl_cloud",
                                                ),
                                            },
                                            {
                                                value: "direct",
                                                label: t(
                                                    "settings.scraper.direct_http_legacy",
                                                ),
                                            },
                                        ]}
                                        disabled={saving}
                                        onValueChange={(value) =>
                                            setField("scrapeProvider", value)
                                        }
                                    />
                                    <Field>
                                        <FieldLabel htmlFor="settings-search-api-key">
                                            {t(
                                                "settings.grounding.search_api_key",
                                            )}
                                        </FieldLabel>
                                        <Input
                                            id="settings-search-api-key"
                                            type="password"
                                            autoComplete="off"
                                            value={draft.searchApiKey}
                                            onChange={(event) =>
                                                setField(
                                                    "searchApiKey",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                    <Field>
                                        <FieldLabel htmlFor="settings-search-base-url">
                                            {t(
                                                "settings.grounding.search_base_url",
                                            )}
                                        </FieldLabel>
                                        <Input
                                            id="settings-search-base-url"
                                            type="url"
                                            value={draft.searchBaseUrl}
                                            onChange={(event) =>
                                                setField(
                                                    "searchBaseUrl",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                </div>
                                <Field>
                                    <FieldLabel htmlFor="settings-vision-model">
                                        {t("settings.vision.model")}
                                    </FieldLabel>
                                    <div className="flex flex-col gap-2 sm:flex-row">
                                        <Input
                                            id="settings-vision-model"
                                            className="flex-1"
                                            value={draft.visionModel}
                                            placeholder={t(
                                                "settings.use_llm_model",
                                            )}
                                            onChange={(event) =>
                                                setField(
                                                    "visionModel",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                        <Button
                                            type="button"
                                            variant="outline"
                                            disabled={visionTesting}
                                            onClick={() => void testVision()}
                                        >
                                            {visionTesting ? (
                                                <Spinner data-icon="inline-start" />
                                            ) : (
                                                <SparklesIcon data-icon="inline-start" />
                                            )}
                                            {visionTesting
                                                ? t("settings.vision.testing")
                                                : t("settings.vision.test")}
                                        </Button>
                                    </div>
                                    <FieldDescription>
                                        {t("settings.vision.description")}
                                    </FieldDescription>
                                </Field>
                                {visionFeedback.kind === "success" ||
                                visionFeedback.kind === "error" ? (
                                    <Alert
                                        variant={
                                            visionFeedback.kind === "error"
                                                ? "destructive"
                                                : "default"
                                        }
                                    >
                                        {visionFeedback.kind === "error" ? (
                                            <CircleAlertIcon />
                                        ) : (
                                            <CheckCircle2Icon />
                                        )}
                                        <AlertTitle>
                                            {t("settings.vision.test_title")}
                                        </AlertTitle>
                                        <AlertDescription>
                                            {visionFeedback.message}
                                        </AlertDescription>
                                    </Alert>
                                ) : null}
                            </FieldGroup>
                        </CardContent>
                    </Card>
                </section>

                <section aria-labelledby="settings-schedule-heading">
                    <Card>
                        <CardHeader>
                            <CardTitle id="settings-schedule-heading">
                                {t("settings.schedule.title")}
                            </CardTitle>
                            <CardDescription>
                                {t("settings.schedule.description")}
                            </CardDescription>
                        </CardHeader>
                        <CardContent>
                            <FieldGroup className="grid gap-4 md:grid-cols-2">
                                <Field>
                                    <FieldLabel htmlFor="settings-time-zone">
                                        {t("settings.schedule.time_zone")}
                                    </FieldLabel>
                                    <Input
                                        id="settings-time-zone"
                                        value={draft.dailyTimeZone}
                                        placeholder={t("format.utc")}
                                        onChange={(event) =>
                                            setField(
                                                "dailyTimeZone",
                                                event.target.value,
                                            )
                                        }
                                    />
                                </Field>
                                <Field>
                                    <FieldLabel htmlFor="settings-refresh-time">
                                        {t("settings.schedule.refresh_time")}
                                    </FieldLabel>
                                    <Input
                                        id="settings-refresh-time"
                                        type="time"
                                        value={draft.dailyUpdateTime}
                                        onChange={(event) =>
                                            setField(
                                                "dailyUpdateTime",
                                                event.target.value,
                                            )
                                        }
                                    />
                                </Field>
                                <Field className="md:col-span-2">
                                    <FieldLabel htmlFor="settings-max-active-cards">
                                        {t(
                                            "settings.schedule.max_active_cards",
                                        )}
                                    </FieldLabel>
                                    <Input
                                        id="settings-max-active-cards"
                                        type="number"
                                        min="0"
                                        step="1"
                                        value={draft.maxActiveCards}
                                        onChange={(event) =>
                                            setField(
                                                "maxActiveCards",
                                                event.target.value,
                                            )
                                        }
                                    />
                                    <FieldDescription>
                                        {t(
                                            "settings.schedule.max_active_cards_description",
                                        )}
                                    </FieldDescription>
                                </Field>
                            </FieldGroup>
                        </CardContent>
                    </Card>
                </section>

                <section aria-labelledby="settings-appearance-heading">
                    <Card>
                        <CardHeader>
                            <CardTitle id="settings-appearance-heading">
                                {t("settings.appearance.title")}
                            </CardTitle>
                            <CardDescription>
                                {t("settings.appearance.description")}
                            </CardDescription>
                        </CardHeader>
                        <CardContent>
                            <FieldGroup className="grid gap-4 md:grid-cols-3">
                                <SelectField
                                    id="settings-color-scheme"
                                    label={t(
                                        "settings.appearance.color_scheme",
                                    )}
                                    value={draft.colorScheme}
                                    options={[
                                        {
                                            value: "shadcn",
                                            label: t(
                                                "settings.theme.shadcn_dark",
                                            ),
                                        },
                                        {
                                            value: "shadcn-light",
                                            label: t(
                                                "settings.theme.shadcn_light",
                                            ),
                                        },
                                        {
                                            value: "carbonfox",
                                            label: t(
                                                "settings.theme.carbonfox",
                                            ),
                                        },
                                        {
                                            value: "ayu",
                                            label: t("settings.theme.ayu"),
                                        },
                                        {
                                            value: "solarized-light",
                                            label: t(
                                                "settings.theme.solarized_light",
                                            ),
                                        },
                                        {
                                            value: "solarized-dark",
                                            label: t(
                                                "settings.theme.solarized_dark",
                                            ),
                                        },
                                        {
                                            value: "amoled",
                                            label: t("settings.theme.amoled"),
                                        },
                                        {
                                            value: "slate",
                                            label: t("settings.theme.slate"),
                                        },
                                    ]}
                                    disabled={saving}
                                    onValueChange={(value) =>
                                        setField("colorScheme", value)
                                    }
                                />
                                <SelectField
                                    id="settings-transparency"
                                    label={t(
                                        "settings.appearance.transparency",
                                    )}
                                    value={draft.transparency}
                                    options={[
                                        {
                                            value: "none",
                                            label: t("common.none"),
                                        },
                                        {
                                            value: "low",
                                            label: t("common.low"),
                                        },
                                        {
                                            value: "medium",
                                            label: t("common.medium"),
                                        },
                                        {
                                            value: "full",
                                            label: t("common.full"),
                                        },
                                    ]}
                                    disabled={saving}
                                    onValueChange={(value) =>
                                        setField("transparency", value)
                                    }
                                />
                                <SelectField
                                    id="settings-blur"
                                    label={t(
                                        "settings.appearance.blur_intensity",
                                    )}
                                    value={draft.blurIntensity}
                                    options={[
                                        {
                                            value: "none",
                                            label: t("common.none"),
                                        },
                                        {
                                            value: "low",
                                            label: t("common.low"),
                                        },
                                        {
                                            value: "medium",
                                            label: t("common.medium"),
                                        },
                                        {
                                            value: "full",
                                            label: t("common.full"),
                                        },
                                    ]}
                                    disabled={saving}
                                    onValueChange={(value) =>
                                        setField("blurIntensity", value)
                                    }
                                />
                            </FieldGroup>
                        </CardContent>
                    </Card>
                </section>

                <section aria-labelledby="settings-admin-heading">
                    <Card>
                        <CardHeader>
                            <CardTitle id="settings-admin-heading">
                                {t("settings.admin.title")}
                            </CardTitle>
                            <CardDescription>
                                {t("settings.admin.description")}
                            </CardDescription>
                        </CardHeader>
                        <CardContent>
                            <FieldGroup>
                                <Field orientation="horizontal">
                                    <FieldContent>
                                        <FieldTitle>
                                            {t("settings.admin.enable_updates")}
                                        </FieldTitle>
                                        <FieldDescription>
                                            {t(
                                                "settings.admin.enable_updates_description",
                                            )}
                                        </FieldDescription>
                                    </FieldContent>
                                    <Switch
                                        checked={draft.autoupdateEnabled}
                                        disabled={saving}
                                        onCheckedChange={(checked) =>
                                            setField(
                                                "autoupdateEnabled",
                                                checked,
                                            )
                                        }
                                        aria-label={t(
                                            "settings.admin.enable_updates",
                                        )}
                                    />
                                </Field>
                                <div className="grid gap-4 md:grid-cols-2">
                                    <Field>
                                        <FieldLabel htmlFor="settings-autoupdate-repo">
                                            {t(
                                                "settings.admin.github_repository",
                                            )}
                                        </FieldLabel>
                                        <Input
                                            id="settings-autoupdate-repo"
                                            value={draft.autoupdateRepo}
                                            placeholder="slopfire/denpie"
                                            onChange={(event) =>
                                                setField(
                                                    "autoupdateRepo",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                    <Field>
                                        <FieldLabel htmlFor="settings-autoupdate-branch">
                                            {t("settings.admin.branch")}
                                        </FieldLabel>
                                        <Input
                                            id="settings-autoupdate-branch"
                                            value={draft.autoupdateBranch}
                                            placeholder="main"
                                            onChange={(event) =>
                                                setField(
                                                    "autoupdateBranch",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                    <Field>
                                        <FieldLabel htmlFor="settings-autoupdate-interval">
                                            {t("settings.admin.check_interval")}
                                        </FieldLabel>
                                        <Input
                                            id="settings-autoupdate-interval"
                                            type="number"
                                            min="60"
                                            step="1"
                                            value={
                                                draft.autoupdateCheckIntervalSecs
                                            }
                                            onChange={(event) =>
                                                setField(
                                                    "autoupdateCheckIntervalSecs",
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </Field>
                                </div>
                                <Field>
                                    <FieldLabel htmlFor="settings-autoupdate-command">
                                        {t("settings.admin.update_command")}
                                    </FieldLabel>
                                    <Textarea
                                        id="settings-autoupdate-command"
                                        value={draft.autoupdateCommand}
                                        placeholder={t(
                                            "settings.admin.command_placeholder",
                                        )}
                                        onChange={(event) =>
                                            setField(
                                                "autoupdateCommand",
                                                event.target.value,
                                            )
                                        }
                                    />
                                    <FieldDescription>
                                        {t(
                                            "settings.admin.command_description",
                                        )}
                                    </FieldDescription>
                                </Field>
                                {isAdmin ? (
                                    <Field>
                                        <Button
                                            type="button"
                                            variant="outline"
                                            disabled={
                                                saving ||
                                                (updateStatus !== null &&
                                                    isAutoupdateActive(
                                                        updateStatus,
                                                    ))
                                            }
                                            data-testid="autoupdate-check"
                                            onClick={() => {
                                                setUpdateStatus({
                                                    phase: "checking",
                                                    message: t(
                                                        "settings.admin.checking",
                                                    ),
                                                    targetSha: "",
                                                    updatedAt: "",
                                                });
                                                void triggerAutoupdate()
                                                    .then((result) => {
                                                        toast.show(
                                                            result.message,
                                                            "info",
                                                        );
                                                        return fetchAutoupdateStatus();
                                                    })
                                                    .then((status) =>
                                                        setUpdateStatus(status),
                                                    )
                                                    .catch((error) => {
                                                        toast.show(
                                                            error instanceof
                                                                Error
                                                                ? error.message
                                                                : t(
                                                                      "settings.admin.check_failed",
                                                                  ),
                                                            "error",
                                                        );
                                                    });
                                            }}
                                        >
                                            {t("settings.admin.check_now")}
                                        </Button>
                                        {updateStatus === null ? null : (
                                            <FieldDescription data-testid="autoupdate-status">
                                                {autoupdatePhaseLabel(
                                                    updateStatus.phase,
                                                )}
                                                {updateStatus.message === ""
                                                    ? ""
                                                    : ` · ${updateStatus.message}`}
                                            </FieldDescription>
                                        )}
                                    </Field>
                                ) : null}
                            </FieldGroup>
                        </CardContent>
                    </Card>
                </section>
                <div className="flex justify-end border-t pt-5">
                    <SaveChangesButton
                        saving={saving}
                        changed={changed}
                        onSave={() => void save()}
                    />
                </div>
            </div>
        </section>
    );
}
