import { useCallback, useEffect, useState } from "react";
import { create } from "@bufbuild/protobuf";
import {
    AddDocumentRequestSchema,
    AddPoolImageRequestSchema,
    type AppSummary,
    type AppTopicInfo,
    type DocumentInfo,
    type PoolImageInfo,
} from "@/generated/denpie_pb";
import {
    BookOpenIcon,
    CalendarDaysIcon,
    DatabaseIcon,
    ImageIcon,
    LayersIcon,
    MoreHorizontalIcon,
    PencilIcon,
    RefreshCwIcon,
    SearchIcon,
    SparklesIcon,
    Trash2Icon,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
    AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
    Card,
    CardAction,
    CardContent,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card";
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog";
import {
    Empty,
    EmptyContent,
    EmptyDescription,
    EmptyHeader,
    EmptyMedia,
    EmptyTitle,
} from "@/components/ui/empty";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
    Select,
    SelectContent,
    SelectGroup,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { LoadedImage } from "@/components/content/LoadedImage";
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip";
import { TopicIcon, lookupTopicIconId } from "@/lib/topic-icons.generated";
import { newIdempotencyKey } from "@/lib/api-v1/transport";
import { t, tf } from "@/lib/i18n";
import { filterTopicsByName } from "@/lib/card-content";
import {
    fetchTokenSpend,
    type TokenSpend,
} from "@/lib/dashboard-session";
import { tipTypeLabel } from "@/lib/tip-type";
import { TopicEditorDialog } from "@/components/pages/TopicEditorDialog";
import { useViewRefresh } from "@/islands/use-view-refresh";
import {
    INITIAL_ICON_PICKER_STATE,
    applyTopicIcon,
    closeIconPicker,
    iconShortName,
    openIconPicker,
    pickerTopicFrom,
    pickFailed,
    pickSucceeded,
    rerollIconPicker,
    setTopicIcon,
    startPickingIcon,
    suggestTopicIcons,
    suggestionsFailed,
    suggestionsReceived,
    type IconPickerState,
} from "@/lib/topic-icon-picker";
import {
    createDocument,
    createPoolImage,
    deleteDocument,
    deletePoolImage,
    deleteTopic,
    forceDailyRefresh,
    getSummary,
    listAppTopics,
    listDocuments,
    listPoolImages,
    testVisionModel,
} from "@/lib/api-v1/route-ops";

interface GroundingData {
    summary: AppSummary;
    topics: AppTopicInfo[];
    documents: DocumentInfo[];
    images: PoolImageInfo[];
}

type GroundingState =
    | { kind: "loading" }
    | { kind: "ready"; data: GroundingData }
    | { kind: "error"; message: string };

type MutationState =
    | { kind: "idle" }
    | { kind: "saving"; label: string }
    | { kind: "success"; message: string }
    | { kind: "error"; message: string };

const EMPTY_MUTATION: MutationState = { kind: "idle" };

function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : t("toast.request_failed");
}

function tipcardTypeLabel(tipcardType: string): string {
    return tipTypeLabel(tipcardType);
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

export function topicArchiveHref(
    status: "pending" | "scheduled",
    topic: string,
) {
    return `/archive?status=${status}&topic=${encodeURIComponent(topic)}`;
}

function navigateToArchive(event: React.MouseEvent<HTMLAnchorElement>) {
    if (
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.shiftKey ||
        event.altKey
    ) {
        return;
    }
    event.preventDefault();
    window.history.pushState({}, "", event.currentTarget.href);
    window.dispatchEvent(
        new PopStateEvent("popstate", { state: window.history.state }),
    );
    window.scrollTo(0, 0);
}

function SummaryCard({ label, value }: { label: string; value: string }) {
    return (
        <Card size="sm">
            <CardHeader>
                <CardDescription>{label}</CardDescription>
                <CardTitle className="text-2xl font-semibold tracking-tight tabular-nums">
                    {value}
                </CardTitle>
            </CardHeader>
        </Card>
    );
}

function ConfirmDelete({
    label,
    description,
    disabled,
    onConfirm,
    className,
}: {
    label: string;
    description: string;
    disabled: boolean;
    onConfirm: () => void;
    className?: string;
}) {
    return (
        <AlertDialog>
            <AlertDialogTrigger
                render={
                    <Button
                        variant="destructive"
                        size="sm"
                        className={className}
                    />
                }
                disabled={disabled}
            >
                <Trash2Icon data-icon="inline-start" />
                {t("common.delete")}
            </AlertDialogTrigger>
            <AlertDialogContent>
                <AlertDialogHeader>
                    <AlertDialogTitle>{label}</AlertDialogTitle>
                    <AlertDialogDescription>
                        {description}
                    </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                    <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
                    <AlertDialogAction
                        variant="destructive"
                        onClick={onConfirm}
                    >
                        {t("common.delete")}
                    </AlertDialogAction>
                </AlertDialogFooter>
            </AlertDialogContent>
        </AlertDialog>
    );
}

function TopicActionsMenu({
    topic,
    busy,
    onDelete,
}: {
    topic: AppTopicInfo;
    busy: boolean;
    onDelete: () => void;
}) {
    const [open, setOpen] = useState(false);
    return (
        <>
            <DropdownMenu>
                <DropdownMenuTrigger
                    render={<Button variant="ghost" size="icon-sm" />}
                    disabled={busy}
                    aria-label={tf("grounding.topics.more_actions", {
                        name: topic.name,
                    })}
                    data-testid={`topic-more-${topic.id}`}
                >
                    <MoreHorizontalIcon />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                    <DropdownMenuItem
                        variant="destructive"
                        disabled={busy}
                        data-testid={`topic-delete-${topic.id}`}
                        onClick={() => setOpen(true)}
                    >
                        <Trash2Icon data-icon="inline-start" />
                        {t("common.delete")}
                    </DropdownMenuItem>
                </DropdownMenuContent>
            </DropdownMenu>
            <AlertDialog open={open} onOpenChange={setOpen}>
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {tf("grounding.delete_topic_title", {
                                name: topic.name,
                            })}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {t("grounding.delete_topic_description")}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
                        <AlertDialogAction
                            variant="destructive"
                            onClick={() => {
                                setOpen(false);
                                onDelete();
                            }}
                        >
                            {t("common.delete")}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </>
    );
}

function TopicIconPicker({
    picker,
    onOpenChange,
    onReroll,
    onPick,
}: {
    picker: IconPickerState;
    onOpenChange: (open: boolean) => void;
    onReroll: () => void;
    onPick: (iconId: string) => void;
}) {
    const open = picker.kind !== "closed";
    const topic = picker.kind === "closed" ? null : picker.topic;
    const suggesting = picker.kind === "suggesting";
    const picking = picker.kind === "picking";
    const suggestions =
        picker.kind === "ready" || picker.kind === "picking"
            ? picker.suggestions
            : [];
    const error =
        picker.kind === "suggestError"
            ? picker.message
            : picker.kind === "ready"
              ? picker.error
              : undefined;
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="sm:max-w-md"
                data-testid="topic-icon-picker"
                aria-busy={suggesting || picking}
            >
                <DialogHeader>
                    <DialogTitle>
                        {t("grounding.icon_picker.title")}
                    </DialogTitle>
                    {topic ? (
                        <DialogDescription className="truncate">
                            {tf("grounding.icon_picker.topic", {
                                name: topic.name,
                            })}
                        </DialogDescription>
                    ) : null}
                </DialogHeader>
                <div className="flex flex-col gap-4">
                    <p className="text-sm font-medium">
                        {t("grounding.icon_picker.suggestions")}
                    </p>
                    {suggesting ? (
                        <div
                            className="flex items-center justify-center gap-2 py-8 text-sm text-muted-foreground"
                            role="status"
                        >
                            <Spinner />
                            {t("grounding.icon_picker.suggesting")}
                        </div>
                    ) : null}
                    {picker.kind === "empty" ? (
                        <p className="py-8 text-center text-sm text-muted-foreground">
                            {t("grounding.icon_picker.empty")}
                        </p>
                    ) : null}
                    {picker.kind === "suggestError" ? (
                        <p
                            className="py-8 text-center text-sm text-destructive"
                            role="alert"
                        >
                            {error ?? t("grounding.icon_picker.suggest_failed")}
                        </p>
                    ) : null}
                    {suggestions.length > 0 ? (
                        <div className="grid grid-cols-5 gap-2">
                            {suggestions.map((icon) => (
                                <Button
                                    key={icon}
                                    type="button"
                                    variant="outline"
                                    disabled={picking}
                                    className="h-auto flex-col gap-1 p-3"
                                    aria-label={tf(
                                        "grounding.icon_picker.choose",
                                        { icon: iconShortName(icon) },
                                    )}
                                    onClick={() => onPick(icon)}
                                >
                                    <TopicIcon
                                        icon={lookupTopicIconId(icon)}
                                        className="size-6"
                                        aria-hidden
                                    />
                                    <span className="w-full truncate text-center text-xs text-muted-foreground">
                                        {iconShortName(icon)}
                                    </span>
                                </Button>
                            ))}
                        </div>
                    ) : null}
                    {picker.kind === "ready" && error ? (
                        <p className="text-sm text-destructive" role="alert">
                            {error}
                        </p>
                    ) : null}
                    {topic ? (
                        <div className="flex items-center justify-between gap-2">
                            <div className="flex items-center gap-2 text-sm text-muted-foreground">
                                <span>
                                    {t("grounding.icon_picker.current")}
                                </span>
                                <TopicIcon
                                    icon={lookupTopicIconId(topic.iconId)}
                                    color={topic.topicColor || undefined}
                                    className="size-4"
                                    aria-hidden
                                />
                            </div>
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                disabled={suggesting || picking}
                                onClick={onReroll}
                            >
                                <RefreshCwIcon data-icon="inline-start" />
                                {t("grounding.icon_picker.reroll")}
                            </Button>
                        </div>
                    ) : null}
                </div>
            </DialogContent>
        </Dialog>
    );
}

function TopicsPanel({
    topics,
    busy,
    run,
    onIconApplied,
    onEdit,
}: {
    topics: AppTopicInfo[];
    busy: boolean;
    run: (label: string, action: () => Promise<void>) => void;
    onIconApplied: (topicId: bigint, iconId: string) => void;
    onEdit: (topic: AppTopicInfo) => void;
}) {
    const [query, setQuery] = useState("");
    const visible = filterTopicsByName(topics, query);
    const [picker, setPicker] = useState<IconPickerState>(
        INITIAL_ICON_PICKER_STATE,
    );
    const suggestingRequest =
        picker.kind === "suggesting" ? picker.request : null;
    const excludedIcons =
        picker.kind === "suggesting" ? picker.excludedIcons : null;
    useEffect(() => {
        if (suggestingRequest === null || excludedIcons === null) return;
        const request = suggestingRequest;
        void suggestTopicIcons({
            id: request.topicId,
            excludedIcons,
        })
            .then((icons) => {
                setPicker((current) =>
                    suggestionsReceived(current, request, icons),
                );
            })
            .catch((error) => {
                setPicker((current) =>
                    suggestionsFailed(current, request, errorMessage(error)),
                );
            });
    }, [suggestingRequest, excludedIcons]);
    if (topics.length === 0) {
        return (
            <Empty>
                <EmptyHeader>
                    <EmptyMedia variant="icon">
                        <BookOpenIcon />
                    </EmptyMedia>
                    <EmptyTitle>{t("grounding.topics.empty_title")}</EmptyTitle>
                    <EmptyDescription>
                        {t("grounding.topics.empty_description")}
                    </EmptyDescription>
                </EmptyHeader>
                <EmptyContent>
                    <Button render={<a href="/" />} nativeButton={false}>
                        {t("grounding.topics.open_transmission")}
                    </Button>
                </EmptyContent>
            </Empty>
        );
    }
    return (
        <>
            <div className="mb-4">
                <label className="sr-only" htmlFor="topic-search">
                    {t("grounding.topics.find")}
                </label>
                <div className="relative max-w-sm">
                    <SearchIcon
                        className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <Input
                        id="topic-search"
                        value={query}
                        onChange={(event) => setQuery(event.target.value)}
                        placeholder={t("grounding.topics.find")}
                        className="pl-8"
                    />
                </div>
            </div>
            <div
                id="topics-grid"
                data-testid="topics-grid"
                className="grid grid-cols-1 gap-6 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4"
            >
                {visible.map((topic) => {
                    const iconButton = (
                        <Button
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            className="shrink-0"
                            aria-label={t("grounding.icon_picker.open")}
                            data-testid={`topic-icon-picker-${topic.id}`}
                            onClick={() =>
                                setPicker((current) =>
                                    openIconPicker(
                                        current,
                                        pickerTopicFrom({
                                            id: topic.id,
                                            name: topic.name,
                                            iconId: topic.iconId,
                                            topicColor: topic.topicColor,
                                        }),
                                    ),
                                )
                            }
                        >
                            <TopicIcon
                                icon={lookupTopicIconId(topic.iconId)}
                                color={topic.topicColor || undefined}
                                className="size-5"
                                aria-hidden
                            />
                        </Button>
                    );
                    return (
                        <Card
                            key={topic.id.toString()}
                            size="sm"
                            data-testid={`topic-card-${topic.id}`}
                        >
                            <CardHeader>
                                <CardTitle className="flex min-w-0 items-center gap-2 text-lg">
                                    <Tooltip>
                                        <TooltipTrigger render={iconButton} />
                                        <TooltipContent>
                                            {t("grounding.icon_picker.open")}
                                        </TooltipContent>
                                    </Tooltip>
                                    <span className="truncate">
                                        {topic.name}
                                    </span>
                                </CardTitle>
                                <CardAction className="flex items-center gap-1">
                                    <Badge variant="outline">
                                        {tipcardTypeLabel(
                                            topic.tipcardType || "casual_tip",
                                        )}
                                    </Badge>
                                    <TopicActionsMenu
                                        topic={topic}
                                        busy={busy}
                                        onDelete={() =>
                                            run(
                                                t(
                                                    "grounding.actions.deleting_topic",
                                                ),
                                                async () => {
                                                    await deleteTopic({
                                                        id: topic.id,
                                                        idempotencyKey:
                                                            newIdempotencyKey(),
                                                    });
                                                },
                                            )
                                        }
                                    />
                                </CardAction>
                            </CardHeader>
                            <CardContent className="flex flex-col gap-3">
                                <p
                                    className="text-sm text-muted-foreground"
                                    data-testid={`topic-due-total-${topic.id}`}
                                >
                                    {tf("grounding.topics.due_total", {
                                        due: topic.dueCards.toString(),
                                        total: topic.totalCards.toString(),
                                    })}
                                </p>
                                <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        className="w-full"
                                        disabled={
                                            topic.pendingCards === BigInt(0)
                                        }
                                        render={
                                            <a
                                                href={topicArchiveHref(
                                                    "pending",
                                                    topic.name,
                                                )}
                                                onClick={navigateToArchive}
                                            />
                                        }
                                        nativeButton={false}
                                        data-testid={`topic-${topic.id}-pending-archive`}
                                    >
                                        <LayersIcon data-icon="inline-start" />
                                        {tf("grounding.show_pending_cards", {
                                            count: topic.pendingCards.toString(),
                                        })}
                                    </Button>
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        className="w-full"
                                        render={
                                            <a
                                                href={topicArchiveHref(
                                                    "scheduled",
                                                    topic.name,
                                                )}
                                                onClick={navigateToArchive}
                                            />
                                        }
                                        nativeButton={false}
                                        data-testid={`topic-${topic.id}-scheduled-archive`}
                                    >
                                        <CalendarDaysIcon data-icon="inline-start" />
                                        {t("grounding.show_scheduled_cards")}
                                    </Button>
                                </div>
                                <div className="grid grid-cols-2 gap-2">
                                    <Button
                                        variant="secondary"
                                        size="sm"
                                        className="w-full"
                                        disabled={busy}
                                        onClick={() =>
                                            run(
                                                t(
                                                    "grounding.actions.loading_cards",
                                                ),
                                                async () => {
                                                    await forceDailyRefresh({
                                                        topics: topic.name,
                                                        tipcardType:
                                                            topic.tipcardType,
                                                        idempotencyKey:
                                                            newIdempotencyKey(),
                                                    });
                                                },
                                            )
                                        }
                                    >
                                        <RefreshCwIcon data-icon="inline-start" />
                                        {t("common.load")}
                                    </Button>
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        className="w-full"
                                        disabled={busy}
                                        data-testid={`topic-edit-${topic.id}`}
                                        onClick={() => onEdit(topic)}
                                    >
                                        <PencilIcon data-icon="inline-start" />
                                        {t("common.edit")}
                                    </Button>
                                </div>
                            </CardContent>
                        </Card>
                    );
                })}
            </div>
            <TopicIconPicker
                picker={picker}
                onOpenChange={(open) => {
                    if (!open) setPicker((current) => closeIconPicker(current));
                }}
                onReroll={() =>
                    setPicker((current) => rerollIconPicker(current))
                }
                onPick={(iconId) => {
                    const next = startPickingIcon(picker, iconId);
                    setPicker(next);
                    if (next.kind !== "picking") return;
                    const request = next.request;
                    void setTopicIcon({
                        id: next.topic.id,
                        iconId,
                    })
                        .then((applied) => {
                            setPicker((current) =>
                                pickSucceeded(current, request),
                            );
                            onIconApplied(next.topic.id, applied);
                        })
                        .catch((error) => {
                            setPicker((current) =>
                                pickFailed(
                                    current,
                                    request,
                                    errorMessage(error),
                                ),
                            );
                        });
                }}
            />
        </>
    );
}

function SourcesPanel({
    documents,
    topics,
    busy,
    run,
}: {
    documents: DocumentInfo[];
    topics: AppTopicInfo[];
    busy: boolean;
    run: (label: string, action: () => Promise<void>) => void;
}) {
    const [title, setTitle] = useState("");
    const [url, setUrl] = useState("");
    const [content, setContent] = useState("");
    const [topicId, setTopicId] = useState("");
    const topicItems = [
        { label: t("grounding.sources.unassigned"), value: "" },
        ...topics.map((topic) => ({
            label: topic.name,
            value: topic.id.toString(),
        })),
    ];
    const submit = (event: React.FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        run(t("grounding.actions.adding_source"), async () => {
            const selectedTopic = topicId === "" ? [] : [BigInt(topicId)];
            await createDocument({
                request: create(AddDocumentRequestSchema, {
                    topicIdOpt: topicId,
                    sourceType: url.trim() === "" ? "document" : "link",
                    title,
                    url,
                    content,
                    topicIds: selectedTopic,
                }),
                idempotencyKey: newIdempotencyKey(),
            });
            setTitle("");
            setUrl("");
            setContent("");
        });
    };
    return (
        <div className="grid gap-6 xl:grid-cols-[minmax(18rem,24rem)_1fr]">
            <Card>
                <CardHeader>
                    <CardTitle>{t("grounding.sources.add_title")}</CardTitle>
                    <CardDescription>
                        {t("grounding.sources.add_description")}
                    </CardDescription>
                </CardHeader>
                <form onSubmit={submit}>
                    <CardContent>
                        <FieldGroup>
                            <Field>
                                <FieldLabel htmlFor="source-title">
                                    {t("common.title")}
                                </FieldLabel>
                                <Input
                                    id="source-title"
                                    required
                                    value={title}
                                    onChange={(event) =>
                                        setTitle(event.target.value)
                                    }
                                />
                            </Field>
                            <Field>
                                <FieldLabel htmlFor="source-url">
                                    {t("grounding.sources.url_optional")}
                                </FieldLabel>
                                <Input
                                    id="source-url"
                                    type="url"
                                    value={url}
                                    onChange={(event) =>
                                        setUrl(event.target.value)
                                    }
                                />
                            </Field>
                            <Field>
                                <FieldLabel htmlFor="source-content">
                                    {t("common.content")}
                                </FieldLabel>
                                <Textarea
                                    id="source-content"
                                    rows={6}
                                    value={content}
                                    onChange={(event) =>
                                        setContent(event.target.value)
                                    }
                                />
                            </Field>
                            <Field>
                                <FieldLabel>{t("common.topic")}</FieldLabel>
                                <Select
                                    items={topicItems}
                                    value={topicId}
                                    onValueChange={(value) =>
                                        setTopicId(value ?? "")
                                    }
                                >
                                    <SelectTrigger className="w-full">
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent alignItemWithTrigger={false}>
                                        <SelectGroup>
                                            {topicItems.map((item) => (
                                                <SelectItem
                                                    key={item.value || "none"}
                                                    value={item.value}
                                                >
                                                    {item.label}
                                                </SelectItem>
                                            ))}
                                        </SelectGroup>
                                    </SelectContent>
                                </Select>
                            </Field>
                        </FieldGroup>
                    </CardContent>
                    <CardFooter>
                        <Button type="submit" disabled={busy || !title.trim()}>
                            {t("grounding.sources.add")}
                        </Button>
                    </CardFooter>
                </form>
            </Card>
            <Card>
                <CardHeader>
                    <CardTitle>{t("grounding.sources.title")}</CardTitle>
                    <CardDescription>
                        {tf("grounding.sources.stored_count", {
                            count: documents.length,
                        })}
                    </CardDescription>
                </CardHeader>
                <CardContent className="flex flex-col gap-3">
                    {documents.length === 0 ? (
                        <Empty>
                            <EmptyHeader>
                                <EmptyMedia variant="icon">
                                    <DatabaseIcon />
                                </EmptyMedia>
                                <EmptyTitle>
                                    {t("grounding.sources.empty_title")}
                                </EmptyTitle>
                                <EmptyDescription>
                                    {t("grounding.sources.empty_description")}
                                </EmptyDescription>
                            </EmptyHeader>
                        </Empty>
                    ) : (
                        documents.map((document) => (
                            <Card key={document.id.toString()} size="sm">
                                <CardHeader className="flex-row items-start gap-3">
                                    <div className="min-w-0 flex-1">
                                        <CardTitle>{document.title}</CardTitle>
                                        <CardDescription>
                                            {sourceTypeLabel(
                                                document.sourceType,
                                            )}
                                            {document.topicIds.length > 0
                                                ? tf(
                                                      "grounding.sources.topic_assignments",
                                                      {
                                                          count: document
                                                              .topicIds.length,
                                                      },
                                                  )
                                                : t(
                                                      "grounding.sources.unassigned_suffix",
                                                  )}
                                        </CardDescription>
                                    </div>
                                    <ConfirmDelete
                                        label={tf(
                                            "grounding.delete_source_title",
                                            {
                                                title: document.title,
                                            },
                                        )}
                                        description={t(
                                            "grounding.delete_source_description",
                                        )}
                                        disabled={busy}
                                        onConfirm={() =>
                                            run(
                                                t(
                                                    "grounding.actions.deleting_source",
                                                ),
                                                async () => {
                                                    await deleteDocument({
                                                        id: document.id,
                                                        idempotencyKey:
                                                            newIdempotencyKey(),
                                                    });
                                                },
                                            )
                                        }
                                    />
                                </CardHeader>
                            </Card>
                        ))
                    )}
                </CardContent>
            </Card>
        </div>
    );
}

function ImagesPanel({
    images,
    busy,
    run,
}: {
    images: PoolImageInfo[];
    busy: boolean;
    run: (label: string, action: () => Promise<void>) => void;
}) {
    const [name, setName] = useState("");
    const [description, setDescription] = useState("");
    const [imageData, setImageData] = useState("");
    const selectFile = (event: React.ChangeEvent<HTMLInputElement>) => {
        const file = event.target.files?.item(0);
        if (file === null || file === undefined) return;
        const reader = new FileReader();
        reader.addEventListener("load", () => {
            if (typeof reader.result === "string") setImageData(reader.result);
        });
        reader.readAsDataURL(file);
    };
    return (
        <div className="flex flex-col gap-6">
            <Card>
                <CardHeader>
                    <CardTitle>{t("grounding.images.add_title")}</CardTitle>
                    <CardDescription>
                        {t("grounding.images.add_description")}
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    <FieldGroup>
                        <Field>
                            <FieldLabel htmlFor="pool-image">
                                {t("common.image")}
                            </FieldLabel>
                            <Input
                                id="pool-image"
                                type="file"
                                accept="image/*"
                                onChange={selectFile}
                            />
                        </Field>
                        <Field>
                            <FieldLabel htmlFor="pool-name">
                                {t("common.name")}
                            </FieldLabel>
                            <Input
                                id="pool-name"
                                value={name}
                                onChange={(event) =>
                                    setName(event.target.value)
                                }
                            />
                        </Field>
                        <Field>
                            <FieldLabel htmlFor="pool-description">
                                {t("common.description")}
                            </FieldLabel>
                            <Input
                                id="pool-description"
                                value={description}
                                onChange={(event) =>
                                    setDescription(event.target.value)
                                }
                            />
                        </Field>
                    </FieldGroup>
                </CardContent>
                <CardFooter>
                    <Button
                        disabled={busy || !imageData || !name.trim()}
                        onClick={() =>
                            run(
                                t("grounding.actions.adding_image"),
                                async () => {
                                    await createPoolImage({
                                        request: create(
                                            AddPoolImageRequestSchema,
                                            {
                                                imageData,
                                                name,
                                                description,
                                            },
                                        ),
                                        idempotencyKey: newIdempotencyKey(),
                                    });
                                    setImageData("");
                                    setName("");
                                    setDescription("");
                                },
                            )
                        }
                    >
                        {t("grounding.images.add")}
                    </Button>
                </CardFooter>
            </Card>
            {images.length === 0 ? (
                <Empty>
                    <EmptyHeader>
                        <EmptyMedia variant="icon">
                            <ImageIcon />
                        </EmptyMedia>
                        <EmptyTitle>
                            {t("grounding.images.empty_title")}
                        </EmptyTitle>
                        <EmptyDescription>
                            {t("grounding.images.empty_description")}
                        </EmptyDescription>
                    </EmptyHeader>
                </Empty>
            ) : (
                <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
                    {images.map((image) => (
                        <Card key={image.id.toString()} size="sm">
                            <CardContent className="pt-4">
                                <LoadedImage
                                    src={`/api/v1/pool-images/${image.id}`}
                                    alt={image.name}
                                    className="aspect-video w-full rounded-md object-cover"
                                    fallback={
                                        <div
                                            className="flex aspect-video w-full items-center justify-center rounded-md bg-muted text-muted-foreground"
                                            role="img"
                                            aria-label={t("images.unavailable")}
                                        >
                                            <ImageIcon
                                                className="size-8"
                                                aria-hidden="true"
                                            />
                                        </div>
                                    }
                                />
                            </CardContent>
                            <CardHeader>
                                <CardTitle>{image.name}</CardTitle>
                                <CardDescription>
                                    {image.description ||
                                        t("common.no_description")}
                                </CardDescription>
                            </CardHeader>
                            <CardFooter>
                                <ConfirmDelete
                                    label={tf("grounding.delete_image_title", {
                                        name: image.name,
                                    })}
                                    description={t(
                                        "grounding.delete_image_description",
                                    )}
                                    disabled={busy}
                                    onConfirm={() =>
                                        run(
                                            t(
                                                "grounding.actions.deleting_image",
                                            ),
                                            async () => {
                                                await deletePoolImage({
                                                    id: image.id,
                                                    idempotencyKey:
                                                        newIdempotencyKey(),
                                                });
                                            },
                                        )
                                    }
                                />
                            </CardFooter>
                        </Card>
                    ))}
                </div>
            )}
        </div>
    );
}

export function GroundingPage({ active = true }: { active?: boolean }) {
    const [state, setState] = useState<GroundingState>({ kind: "loading" });
    const [mutation, setMutation] = useState<MutationState>(EMPTY_MUTATION);
    const [tokenSpend, setTokenSpend] = useState<TokenSpend | null>(null);
    const [editing, setEditing] = useState<AppTopicInfo | null>(null);
    const load = useCallback(async () => {
        setState({ kind: "loading" });
        try {
            const [summary, topics, documents, images, spend] =
                await Promise.all([
                    getSummary(),
                    listAppTopics(),
                    listDocuments(),
                    listPoolImages(),
                    fetchTokenSpend().catch(() => null),
                ]);
            setTokenSpend(spend);
            setState({
                kind: "ready",
                data: {
                    summary: summary.summary,
                    topics: topics.topics,
                    documents: documents.documents,
                    images: images.images,
                },
            });
        } catch (error) {
            setState({ kind: "error", message: errorMessage(error) });
        }
    }, []);
    useViewRefresh(active, load);
    const run = useCallback(
        (label: string, action: () => Promise<void>) => {
            setMutation({ kind: "saving", label });
            void action()
                .then(async () => {
                    setMutation({
                        kind: "success",
                        message: tf("grounding.action_complete", {
                            action: label,
                        }),
                    });
                    await load();
                })
                .catch((error) =>
                    setMutation({
                        kind: "error",
                        message: errorMessage(error),
                    }),
                );
        },
        [load],
    );
    const busy = mutation.kind === "saving";

    return (
        <section className="flex flex-col gap-5" data-testid="grounding-page">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                    <h1 className="text-xl font-semibold tracking-tight">
                        {t("grounding.title")}
                    </h1>
                    <p className="mt-2 text-muted-foreground">
                        {t("grounding.description")}
                    </p>
                </div>
                <Button variant="outline" onClick={() => void load()}>
                    <RefreshCwIcon data-icon="inline-start" />
                    {t("common.refresh")}
                </Button>
            </div>
            {mutation.kind === "saving" ? (
                <Alert>
                    <Spinner />
                    <AlertTitle>{mutation.label}</AlertTitle>
                </Alert>
            ) : null}
            {mutation.kind === "success" ? (
                <Alert>
                    <AlertTitle role="status">{mutation.message}</AlertTitle>
                </Alert>
            ) : null}
            {mutation.kind === "error" ? (
                <Alert variant="destructive">
                    <AlertTitle>{t("grounding.action_failed")}</AlertTitle>
                    <AlertDescription role="alert">
                        {mutation.message}
                    </AlertDescription>
                </Alert>
            ) : null}
            {state.kind === "loading" ? (
                <div
                    className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4"
                    role="status"
                >
                    <span className="sr-only">{t("grounding.loading")}</span>
                    {Array.from({ length: 4 }, (_, index) => (
                        <Skeleton key={index} className="h-28 w-full" />
                    ))}
                </div>
            ) : null}
            {state.kind === "error" ? (
                <Alert variant="destructive">
                    <AlertTitle>{t("grounding.load_failed")}</AlertTitle>
                    <AlertDescription className="flex flex-col items-start gap-3">
                        <span role="alert">{state.message}</span>
                        <Button variant="outline" onClick={() => void load()}>
                            {t("common.retry")}
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : null}
            {state.kind === "ready" ? (
                <div className="flex flex-col gap-8">
                    <section
                        className="flex flex-col gap-5"
                        aria-labelledby="grounding-overview-heading"
                    >
                        <h2
                            id="grounding-overview-heading"
                            className="text-lg font-semibold tracking-tight"
                        >
                            {t("grounding.tabs.overview")}
                        </h2>
                        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
                            <SummaryCard
                                label={t("grounding.summary.topics")}
                                value={state.data.summary.topics.toString()}
                            />
                            <SummaryCard
                                label={t("grounding.summary.total_cards")}
                                value={state.data.summary.totalCards.toString()}
                            />
                            <SummaryCard
                                label={t("grounding.summary.due_cards")}
                                value={state.data.summary.dueCards.toString()}
                            />
                            <SummaryCard
                                label={t("grounding.summary.active_cards")}
                                value={state.data.summary.activeCards.toString()}
                            />
                            {tokenSpend === null ? null : (
                                <div
                                    id="token-spend-row"
                                    className="contents"
                                    data-testid="token-spend"
                                >
                                    <SummaryCard
                                        label={t("grounding.token_spend.daily")}
                                        value={tf("format.tokens", {
                                            count: tokenSpend.daily,
                                        })}
                                    />
                                    <SummaryCard
                                        label={t(
                                            "grounding.token_spend.monthly",
                                        )}
                                        value={tf("format.tokens", {
                                            count: tokenSpend.monthly,
                                        })}
                                    />
                                    <SummaryCard
                                        label={t("grounding.token_spend.total")}
                                        value={tf("format.tokens", {
                                            count: tokenSpend.total,
                                        })}
                                    />
                                </div>
                            )}
                        </div>
                        <Card>
                            <CardHeader>
                                <CardTitle>
                                    {t("grounding.vision.title")}
                                </CardTitle>
                                <CardDescription>
                                    {t("grounding.vision.description")}
                                </CardDescription>
                            </CardHeader>
                            <CardFooter>
                                <Button
                                    variant="outline"
                                    disabled={busy}
                                    onClick={() =>
                                        run(
                                            t(
                                                "grounding.actions.testing_vision_model",
                                            ),
                                            async () => {
                                                const result =
                                                    await testVisionModel();
                                                if (!result.result.ok) {
                                                    throw new Error(
                                                        result.result.message,
                                                    );
                                                }
                                            },
                                        )
                                    }
                                >
                                    <SparklesIcon data-icon="inline-start" />
                                    {t("grounding.vision.test")}
                                </Button>
                            </CardFooter>
                        </Card>
                    </section>
                    <section aria-labelledby="grounding-topics-heading">
                        <h2
                            id="grounding-topics-heading"
                            className="mb-4 text-lg font-semibold tracking-tight"
                        >
                            {t("grounding.tabs.topics")}
                        </h2>
                        <TopicsPanel
                            topics={state.data.topics}
                            busy={busy}
                            run={run}
                            onEdit={setEditing}
                            onIconApplied={(topicId, iconId) => {
                                setState((current) => {
                                    if (current.kind !== "ready")
                                        return current;
                                    return {
                                        kind: "ready",
                                        data: {
                                            ...current.data,
                                            topics: applyTopicIcon(
                                                current.data.topics,
                                                topicId,
                                                iconId,
                                            ),
                                        },
                                    };
                                });
                                setMutation({
                                    kind: "success",
                                    message: t("grounding.icon_picker.updated"),
                                });
                            }}
                        />
                    </section>
                    <section aria-labelledby="grounding-sources-heading">
                        <h2
                            id="grounding-sources-heading"
                            className="mb-4 text-lg font-semibold tracking-tight"
                        >
                            {t("grounding.tabs.sources")}
                        </h2>
                        <SourcesPanel
                            documents={state.data.documents}
                            topics={state.data.topics}
                            busy={busy}
                            run={run}
                        />
                    </section>
                    <section aria-labelledby="grounding-images-heading">
                        <h2
                            id="grounding-images-heading"
                            className="mb-4 text-lg font-semibold tracking-tight"
                        >
                            {t("grounding.tabs.images")}
                        </h2>
                        <ImagesPanel
                            images={state.data.images}
                            busy={busy}
                            run={run}
                        />
                    </section>
                </div>
            ) : null}
            <TopicEditorDialog
                topic={editing}
                busy={busy}
                onClose={() => setEditing(null)}
                onSaved={(topic) => {
                    setEditing(null);
                    setState((current) => {
                        if (current.kind !== "ready") return current;
                        return {
                            kind: "ready",
                            data: {
                                ...current.data,
                                topics: current.data.topics.map((item) =>
                                    item.id === topic.id ? topic : item,
                                ),
                            },
                        };
                    });
                }}
            />
        </section>
    );
}
