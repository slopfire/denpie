import { useCallback, useRef, useState } from "react";
import type { FormEvent, KeyboardEvent, ChangeEvent } from "react";
import { WandSparklesIcon } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
    Field,
    FieldError,
    FieldGroup,
    FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { AddLifecycle } from "@/lib/flow-add-state";
import {
    buildTipsRequest,
    parseAddCardKind,
    parseTopicsCsv,
    parseStoredCardKind,
    PREFILL_TOPIC_STORAGE_KEY,
    PREFILL_TYPE_STORAGE_KEY,
    selectImages,
} from "@/lib/flow-add-form";
import type { AddCardKind, AddTipsPayload } from "@/lib/flow-add-form";
import { newIdempotencyKey } from "@/lib/api-v1/transport";
import {
    browserImageDeps,
    compressFilesToDataUrls,
} from "@/lib/flow-add-images";
import { t, tf } from "@/lib/i18n";

const KIND_ITEMS: readonly { value: AddCardKind; label: string }[] = [
    { value: "casual", label: t("card.kind.casual") },
    { value: "repeatable", label: t("card.kind.repeat") },
    { value: "manual", label: t("card.kind.manual") },
];

export interface FlowAddFormProps {
    lifecycle: AddLifecycle;
    /** Keep the familiar form visible while the surrounding Flow is unavailable. */
    disabled?: boolean;
    /** Launch one submission with the exact normalized payload. */
    onAdd: (payload: AddTipsPayload) => void;
    /** Retry a failed mutation (key semantics owned by Flow). */
    onRetryMutation: () => void;
    /** Retry only detail resolution/reconciliation — never the mutation. */
    onRetryResolve: () => void;
}

/**
 * The add-card form below the Transmission heading: topics input,
 * Casual/Repeat/Manual switch, manual content and image controls. Full width
 * on mobile; shrink-wraps to the right from `sm`. While a submission runs,
 * only this form disables — Flow cards stay interactive.
 */
export function FlowAddForm({
    lifecycle,
    disabled = false,
    onAdd,
    onRetryMutation,
    onRetryResolve,
}: FlowAddFormProps) {
    const [topics, setTopics] = useState(() =>
        typeof window === "undefined"
            ? ""
            : (window.localStorage.getItem(PREFILL_TOPIC_STORAGE_KEY) ?? ""),
    );
    // Unknown stored kinds fall back to Casual.
    const [kind, setKind] = useState<AddCardKind>(() =>
        typeof window === "undefined"
            ? "casual"
            : parseStoredCardKind(
                  window.localStorage.getItem(PREFILL_TYPE_STORAGE_KEY),
              ),
    );
    const [manualContent, setManualContent] = useState("");
    const [images, setImages] = useState<string[]>([]);
    const [imagesProcessing, setImagesProcessing] = useState(false);
    const [imageError, setImageError] = useState<string | null>(null);
    const [validationError, setValidationError] = useState<string | null>(null);
    const formRef = useRef<HTMLFormElement>(null);

    const pending =
        lifecycle.kind === "submitting" || lifecycle.kind === "resolving";
    const controlsDisabled = disabled || pending || imagesProcessing;

    const onSubmit = useCallback(
        (event: FormEvent<HTMLFormElement>) => {
            event.preventDefault();
            if (controlsDisabled) return;
            setValidationError(null);
            try {
                const payload: AddTipsPayload = {
                    kind,
                    topics: parseTopicsCsv(topics),
                    manualContent,
                    manualImageData: images,
                    idempotencyKey: newIdempotencyKey(),
                };
                // Deterministic pre-fetch rejection (empty topics, blank manual).
                buildTipsRequest(payload);
                onAdd(payload);
            } catch (error) {
                setValidationError(
                    error instanceof Error ? error.message : String(error),
                );
            }
        },
        [controlsDisabled, images, kind, manualContent, onAdd, topics],
    );

    const onTextareaKeyDown = useCallback((event: KeyboardEvent) => {
        // Shift+Enter submits; normal Enter inserts a newline.
        if (event.key === "Enter" && event.shiftKey) {
            event.preventDefault();
            formRef.current?.requestSubmit();
        }
    }, []);

    const onImagesSelected = useCallback(
        async (event: ChangeEvent<HTMLInputElement>) => {
            const files = Array.from(event.target.files ?? []);
            event.target.value = "";
            const selection = selectImages(
                files.map((file) => ({ type: file.type, size: file.size })),
                images.length,
            );
            if (selection.kind === "rejected") {
                setImageError(selection.reason);
                return;
            }
            setImageError(null);
            setImagesProcessing(true);
            try {
                const urls = await compressFilesToDataUrls(
                    files,
                    browserImageDeps(),
                );
                setImages((current) => [...current, ...urls]);
            } catch (error) {
                setImageError(
                    error instanceof Error
                        ? error.message
                        : t("images.process_error"),
                );
            } finally {
                setImagesProcessing(false);
            }
        },
        [images.length],
    );

    const onClearImages = useCallback(() => {
        setImages([]);
        setImageError(null);
    }, []);

    const onKindChange = useCallback((value: readonly string[]) => {
        const next = value.at(0);
        // Base UI emits an empty array when toggling off; keep a valid kind.
        if (next === undefined) return;
        setKind(parseAddCardKind(next));
    }, []);

    return (
        <div className="flex w-full min-w-0 max-w-full justify-end">
            <Card
                className="w-full max-w-full sm:w-auto"
                size="sm"
                data-testid="transmission-form-surface"
            >
                <CardContent>
                    <form
                        ref={formRef}
                        id="tips-form"
                        data-testid="tips-form"
                        onSubmit={onSubmit}
                        aria-busy={pending || imagesProcessing}
                        data-disabled={disabled || undefined}
                        className="flex min-w-0 max-w-full flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-center"
                    >
                        <FieldGroup className="contents">
                            <Field
                                data-testid="add-topics-field"
                                data-invalid={validationError !== null}
                                className="w-full min-w-0 sm:w-64 sm:max-w-full sm:flex-none"
                            >
                                <FieldLabel
                                    htmlFor="tips-topics"
                                    className="sr-only"
                                >
                                    {t("flow.add.topics")}
                                </FieldLabel>
                                <Input
                                    id="tips-topics"
                                    data-testid="tips-topics"
                                    placeholder={t(
                                        "flow.add.topics_placeholder",
                                    )}
                                    value={topics}
                                    onChange={(event) =>
                                        setTopics(event.target.value)
                                    }
                                    disabled={controlsDisabled}
                                    aria-invalid={validationError !== null}
                                    required
                                />
                                {validationError !== null ? (
                                    <FieldError role="alert">
                                        {validationError}
                                    </FieldError>
                                ) : null}
                            </Field>
                            <Field
                                data-testid="add-kind-field"
                                className="min-w-0 w-full sm:w-auto sm:flex-none"
                            >
                                <FieldLabel
                                    id="tips-kind-label"
                                    className="sr-only"
                                >
                                    {t("card.kind.label")}
                                </FieldLabel>
                                <ToggleGroup
                                    variant="outline"
                                    spacing={0}
                                    aria-labelledby="tips-kind-label"
                                    value={[kind]}
                                    onValueChange={onKindChange}
                                    data-testid="tips-kind"
                                    disabled={controlsDisabled}
                                    className="w-full sm:w-auto"
                                >
                                    {KIND_ITEMS.map(({ value, label }) => (
                                        <ToggleGroupItem
                                            key={value}
                                            value={value}
                                            className="flex-1"
                                        >
                                            {label}
                                        </ToggleGroupItem>
                                    ))}
                                </ToggleGroup>
                            </Field>
                            <Button
                                id="tips-submit-btn"
                                data-testid="tips-submit"
                                type="submit"
                                disabled={controlsDisabled}
                                className="w-full sm:w-auto sm:flex-none"
                            >
                                {pending ? (
                                    <Spinner data-icon="inline-start" />
                                ) : (
                                    <WandSparklesIcon
                                        data-icon="inline-start"
                                        aria-hidden
                                    />
                                )}
                                {pending
                                    ? t("flow.add.adding")
                                    : t("common.add")}
                            </Button>
                            {kind === "manual" ? (
                                <>
                                    <Field className="sm:basis-full">
                                        <FieldLabel htmlFor="manual-card-content">
                                            {t("flow.add.manual_content")}
                                        </FieldLabel>
                                        <Textarea
                                            id="manual-card-content"
                                            data-testid="manual-content"
                                            placeholder={t(
                                                "flow.add.manual_content",
                                            )}
                                            value={manualContent}
                                            onChange={(event) =>
                                                setManualContent(
                                                    event.target.value,
                                                )
                                            }
                                            onKeyDown={onTextareaKeyDown}
                                            disabled={controlsDisabled}
                                            aria-invalid={
                                                validationError !== null
                                            }
                                            className="h-20 resize-y"
                                        />
                                    </Field>
                                    <Field className="sm:basis-full">
                                        <FieldLabel htmlFor="manual-card-images">
                                            {t("images.label")}
                                        </FieldLabel>
                                        <div className="flex flex-wrap items-center gap-3">
                                            <Input
                                                id="manual-card-images"
                                                data-testid="manual-images-input"
                                                type="file"
                                                multiple
                                                accept="image/png,image/jpeg,image/webp,image/gif"
                                                className="max-w-sm"
                                                onChange={onImagesSelected}
                                                disabled={controlsDisabled}
                                                aria-invalid={
                                                    imageError !== null
                                                }
                                            />
                                            <span
                                                className="text-sm text-muted-foreground"
                                                role="status"
                                            >
                                                {imagesProcessing
                                                    ? t("images.processing")
                                                    : tf("images.count", {
                                                          count: images.length,
                                                      })}
                                            </span>
                                            {images.length > 0 ? (
                                                <Button
                                                    type="button"
                                                    variant="outline"
                                                    size="sm"
                                                    onClick={onClearImages}
                                                    data-testid="manual-images-clear"
                                                    disabled={controlsDisabled}
                                                >
                                                    {t("common.clear")}
                                                </Button>
                                            ) : null}
                                        </div>
                                        {imageError !== null ? (
                                            <FieldError role="alert">
                                                {imageError}
                                            </FieldError>
                                        ) : null}
                                    </Field>
                                </>
                            ) : null}
                        </FieldGroup>
                        {lifecycle.kind === "mutationError" ? (
                            <>
                                <Alert
                                    variant="destructive"
                                    className="mt-3 sm:basis-full"
                                >
                                    <AlertTitle>
                                        {t("flow.add.error")}
                                    </AlertTitle>
                                    <AlertDescription role="alert">
                                        {lifecycle.message}
                                    </AlertDescription>
                                </Alert>
                                <Button
                                    type="button"
                                    variant="outline"
                                    className="mt-3"
                                    onClick={onRetryMutation}
                                    data-testid="add-retry-mutation"
                                >
                                    {t("common.retry")}
                                </Button>
                            </>
                        ) : null}
                        {lifecycle.kind === "resolutionError" ? (
                            <>
                                <Alert
                                    variant="destructive"
                                    className="mt-3 sm:basis-full"
                                >
                                    <AlertTitle>
                                        {t("flow.add.created")}
                                    </AlertTitle>
                                    <AlertDescription role="alert">
                                        {tf("flow.add.refresh_error", {
                                            message: lifecycle.message,
                                        })}
                                    </AlertDescription>
                                </Alert>
                                <Button
                                    type="button"
                                    variant="outline"
                                    className="mt-3"
                                    onClick={onRetryResolve}
                                    data-testid="add-retry-resolve"
                                >
                                    {t("flow.add.retry_refresh")}
                                </Button>
                            </>
                        ) : null}
                    </form>
                </CardContent>
            </Card>
        </div>
    );
}
