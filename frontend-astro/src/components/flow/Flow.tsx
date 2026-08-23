import {
    useCallback,
    useEffect,
    useMemo,
    useRef,
    useState,
    lazy,
    memo,
    Suspense,
} from "react";
import {
    seedFromSnapshot,
    useFlowPager,
    type FlowState,
} from "./use-flow-pager";
import { usePinMutations, useDeleteMutations } from "./use-flow-pin-mutations";
import { useFlowReviewLifecycle } from "./use-flow-review-lifecycle";
import { useFlowAddLifecycle } from "./use-flow-add-lifecycle";
import { saveFlowSnapshot, type SavedFlowPage } from "@/lib/flow-snapshot";

export type { FlowState };
import {
    CheckIcon,
    ChevronUpIcon,
    CircleAlertIcon,
    CopyIcon,
    GripVerticalIcon,
    LayoutGridIcon,
    ListIcon,
    Maximize2Icon,
    Minimize2Icon,
    MoreHorizontalIcon,
    PinIcon,
    PinOffIcon,
    Trash2Icon,
    XIcon,
} from "lucide-react";
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
    Card,
    CardContent,
    CardFooter,
    CardHeader,
} from "@/components/ui/card";
import { LoadedImage } from "@/components/content/LoadedImage";
import { LIST_IMAGE_MAX_EDGE_PX } from "@/lib/image-thumbnail";
import {
    FlowCardDetail,
    FlowCardDetailTrigger,
} from "@/components/flow/FlowCardDetail";
import { Dialog } from "@/components/ui/dialog";
import { ImageLightbox } from "@/components/content/ImageLightbox";

/** Lazy markdown: prismjs/react-markdown/remark-gfm leave the critical path. */
const LazyMarkdownContent = lazy(() =>
    import("@/components/content/MarkdownContent").then((m) => ({
        default: m.MarkdownContent,
    })),
);

function MarkdownFallback() {
    return (
        <div className="space-y-2 animate-pulse" aria-hidden="true">
            <div className="h-4 rounded bg-muted" />
            <div className="h-4 w-5/6 rounded bg-muted" />
            <div className="h-4 w-2/3 rounded bg-muted" />
        </div>
    );
}
import {
    humanDetailDate,
    isUserFullscreenDismiss,
} from "@/lib/flow-detail-state";
import { t, tf } from "@/lib/i18n";
import {
    cardErrorDetail,
    detectCardContentKind,
} from "@/lib/card-content";
import { useViewRefresh } from "@/islands/use-view-refresh";
import { cn } from "@/lib/utils";
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuRadioGroup,
    DropdownMenuRadioItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import {
    Popover,
    PopoverContent,
    PopoverHeader,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip";
import { ReviewActionValue, type FlowCardInfo } from "@/generated/denpie_pb";
import { lookupTopicIconId, TopicIcon } from "@/lib/topic-icons.generated";
import { listFlowCards } from "@/lib/api-v1/ops";
import { FlowAddForm } from "@/components/flow/FlowAddForm";
import {
    reviewActionsFor,
    type ReviewChoice,
} from "@/lib/flow-review-actions";
import type { FlowCursor } from "@/lib/flow-state";
import {
    refillPollFound,
    refillPollMiss,
    flowSlotKey,
    continueElapsedSeconds,
    continuingStatusText,
    type ReviewSlot,
} from "@/lib/flow-review-state";
import {
    pinCardState,
    type PinCardState,
    type PinState,
} from "@/lib/flow-pin-state";
import {
    deleteCardState,
    type DeleteCardState,
    type DeleteState,
} from "@/lib/flow-delete-state";
import { repeatableStackLayers, toFlowCardViews } from "@/lib/flow-view";
import {
    FLOW_SORT_STORAGE_KEY,
    organizeFlowSlots,
    parseFlowSortMode,
    slotMetadata,
    type FlowSortMode,
} from "@/lib/flow-organization";
import {
    FLOW_GRID_COLUMNS_STORAGE_KEY,
    FLOW_LAYOUT_STORAGE_KEY,
    FLOW_LIST_CLASSES,
    gridClassesForColumns,
    parseFlowLayout,
    parseGridColumns,
    type FlowLayout,
    type GridColumns,
} from "@/lib/flow-layout";
import {
    PINNED_CARD_ORDER_STORAGE_KEY,
    movePinnedCard,
    normalizeCardOrder,
    parsePinnedCardId,
    parsePinnedCardOrder,
    replacePinnedCard,
    serializePinnedCardOrder,
} from "@/lib/flow-pinned-order";
import { splitTopicPicks } from "@/lib/flow-transmission";

/** Delay before each bounded refill poll after an awaiting-refill slot. */
const REFILL_POLL_DELAY_MS = 2000;
/** Miss budget per awaiting-refill slot before it becomes completed. */
const REFILL_MAX_ATTEMPTS = 4;

const NO_SLOTS: ReviewSlot[] = [];

function cardTypeLabel(type: string): string {
    switch (type) {
        case "casual_tip":
            return t("card.type.casual_tip");
        case "repeatable_tip":
            return t("card.type.repeatable_tip");
        case "manual_tip":
            return t("card.type.manual_tip");
        case "custom_tip":
            return t("card.type.custom_tip");
        case "":
            return t("common.unspecified");
        default:
            return type;
    }
}

function cardStatusLabel(status: string): string {
    switch (status) {
        case "active":
            return t("card.status.active");
        case "pending":
            return t("card.status.pending");
        case "custom":
            return t("card.status.custom");
        case "learned":
            return t("card.status.learned");
        case "reviewed":
            return t("card.status.reviewed");
        case "dismissed":
            return t("card.status.dismissed");
        case "":
            return t("common.unspecified");
        default:
            return status;
    }
}

function reviewActionLabel(choice: ReviewChoice): string {
    switch (choice.id) {
        case "dismiss":
            return t("flow.review_action.dismiss");
        case "acknowledge":
            return t("flow.review_action.acknowledge");
        case "again":
            return t("flow.review_action.again");
        case "learned":
            return t("flow.review_action.learned");
        case "known":
            return t("flow.review_action.known");
        case "not-interested":
            return t("flow.review_action.not_interested");
        case "too-difficult":
            return t("flow.review_action.too_difficult");
        case "good":
            return t("flow.review_action.good");
        case "easy":
            return t("flow.review_action.easy");
        default:
            return t("common.unspecified");
    }
}

type ReviewSwipeDirection = "left" | "right";

function repeatableReviewSwipe(
    slot: Extract<ReviewSlot, { kind: "idle" | "reviewing" | "error" }>,
): ReviewSwipeDirection | undefined {
    if (
        slot.kind !== "reviewing" ||
        slot.card.tipcardType !== "repeatable_tip"
    ) {
        return undefined;
    }
    return slot.attempt.action === ReviewActionValue.AGAIN ? "left" : "right";
}


function PinnedDragHandle({
    cardId,
    dragging,
    disabled,
    onDragStart,
    onDragEnd,
}: {
    cardId: bigint;
    dragging: boolean;
    disabled: boolean;
    onDragStart: (cardId: bigint) => void;
    onDragEnd: (cardId: bigint) => void;
}) {
    const id = cardId.toString();
    const button = (
        <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("flow.pinned.drag_aria")}
            data-testid={`pinned-drag-${id}`}
            draggable
            disabled={disabled}
            aria-pressed={dragging}
            onDragStart={(event) => {
                event.dataTransfer.setData("text/plain", id);
                event.dataTransfer.effectAllowed = "move";
                const card =
                    event.currentTarget.closest<HTMLElement>(
                        '[data-slot="card"]',
                    );
                if (card !== null)
                    event.dataTransfer.setDragImage(card, 20, 20);
                onDragStart(cardId);
            }}
            onDragEnd={() => onDragEnd(cardId)}
        >
            <GripVerticalIcon data-icon="inline-start" />
        </Button>
    );
    return (
        <Tooltip>
            <TooltipTrigger render={button} />
            <TooltipContent>{t("flow.pinned.drag_aria")}</TooltipContent>
        </Tooltip>
    );
}

export function CardBodies({
    card,
    leading,
    detailActions,
    detailTrigger,
    fullscreen = false,
    onToggleFullscreen,
}: {
    card: FlowCardInfo;
    leading?: React.ReactNode;
    detailActions?: React.ReactNode;
    detailTrigger?: React.ReactNode;
    fullscreen?: boolean;
    onToggleFullscreen?: () => void;
}) {
    const [expanded, setExpanded] = useState(false);
    const [errorExpanded, setErrorExpanded] = useState(false);
    const [errorCopied, setErrorCopied] = useState(false);
    const [lightboxIndex, setLightboxIndex] = useState<number | null>(null);
    const [compactScrollable, setCompactScrollable] = useState(false);
    const contentKind = detectCardContentKind(
        `${card.fullContent}\n${card.compressedContent}`,
    );
    const errorDetail = cardErrorDetail(
        contentKind,
        card.fullContent,
        card.compressedContent,
    );
    const bodyRef = useRef<HTMLDivElement>(null);
    const view = toFlowCardViews([card])[0];
    const hasCompact =
        card.compressedContent.trim() !== "" &&
        card.fullContent.trim() !== "" &&
        card.compressedContent !== card.fullContent;
    const content = expanded ? card.fullContent : view.content;
    const known =
        card.status !== "pending" &&
        (card.repeatCount > 0 ||
            card.status === "reviewed" ||
            card.status === "dismissed");
    const badgeLabel =
        card.tipcardType === "manual_tip"
            ? t("card.kind.manual")
            : card.tipcardType === "custom_tip"
              ? t("card.kind.custom")
              : known
                ? t("card.state.known")
                : t("card.state.new");
    const created = humanDetailDate(card.createdAt);
    const scheduledRepeat =
        card.status === "active" && card.repeatCount > 0
            ? humanDetailDate(card.nextReviewAt)
            : null;

    useEffect(() => {
        setExpanded(false);
        setLightboxIndex(null);
    }, [card.id]);

    useEffect(() => {
        if (expanded || !hasCompact) {
            setCompactScrollable(false);
            return;
        }
        const body = bodyRef.current;
        if (body === null) return;
        const measure = () => {
            setCompactScrollable(body.scrollHeight > body.clientHeight + 1);
        };
        measure();
        const observer = new ResizeObserver(measure);
        observer.observe(body);
        return () => observer.disconnect();
    }, [card.id, content, expanded, hasCompact]);

    return (
        <>
            <CardHeader
                className="flex flex-row items-center gap-2 overflow-hidden rounded-none border-b px-4 py-3"
                data-testid={`card-title-bar-${card.id}`}
            >
                <div className="flex min-w-0 flex-1 items-center gap-2">
                    {leading}
                    <TopicIcon
                        aria-hidden
                        icon={lookupTopicIconId(card.topicIcon)}
                        className="size-7 shrink-0"
                        color={card.topicColor || undefined}
                        data-testid={`topic-icon-${card.id}`}
                    />
                    {card.pinned ? (
                        <PinIcon
                            aria-hidden
                            data-testid={`card-pinned-${card.id}`}
                            className="size-4 shrink-0 text-primary"
                        />
                    ) : null}
                    <span className="truncate text-base font-semibold tracking-tight">
                        {card.topicName}
                    </span>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                    <Popover>
                        <PopoverTrigger
                            render={<Button variant="outline" size="xs" />}
                            aria-label={tf("card.info_aria", {
                                id: card.id.toString(),
                            })}
                        >
                            {badgeLabel}
                        </PopoverTrigger>
                        <PopoverContent align="end" className="w-60">
                            <PopoverHeader>
                                <PopoverTitle>
                                    {t("card.info.title")}
                                </PopoverTitle>
                            </PopoverHeader>
                            <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 px-2 py-1.5 text-xs">
                                <dt className="text-muted-foreground">
                                    {t("card.info.type")}
                                </dt>
                                <dd className="text-right font-medium">
                                    {cardTypeLabel(card.tipcardType)}
                                </dd>
                                <dt className="text-muted-foreground">
                                    {t("card.info.created")}
                                </dt>
                                <dd className="text-right font-medium">
                                    {created ?? t("common.unknown")}
                                </dd>
                                <dt className="text-muted-foreground">
                                    {t("card.info.reviews")}
                                </dt>
                                <dd className="text-right font-medium">
                                    {card.repeatCount}
                                </dd>
                                <dt className="text-muted-foreground">
                                    {t("card.info.scheduled_repeat")}
                                </dt>
                                <dd className="text-right font-medium">
                                    {scheduledRepeat ?? t("card.not_scheduled")}
                                </dd>
                                <dt className="text-muted-foreground">
                                    {t("card.info.state")}
                                </dt>
                                <dd className="text-right font-medium">
                                    {known
                                        ? t("card.state.known")
                                        : t("card.state.new")}
                                </dd>
                            </dl>
                        </PopoverContent>
                    </Popover>
                    {detailTrigger}
                    {onToggleFullscreen === undefined ? null : (
                        <Button
                            type="button"
                            variant="outline"
                            size="icon-xs"
                            aria-label={
                                fullscreen
                                    ? t("card.exit_fullscreen")
                                    : t("card.fullscreen")
                            }
                            onClick={onToggleFullscreen}
                        >
                            {fullscreen ? (
                                <Minimize2Icon />
                            ) : (
                                <Maximize2Icon />
                            )}
                        </Button>
                    )}
                </div>
            </CardHeader>
            <CardContent className="flex flex-col gap-4 px-4 py-4">
                <span className="sr-only">{card.title}</span>
                {view.imageUrls.length === 0 ? null : (
                    <div
                        className={cn(
                            "grid gap-2",
                            view.imageUrls.length > 1 && "grid-cols-2",
                        )}
                    >
                        {view.imageUrls.map((url, index) => (
                            <LoadedImage
                                key={url}
                                src={url}
                                maxDecodeEdge={LIST_IMAGE_MAX_EDGE_PX}
                                alt={tf("images.illustration_for_card", {
                                    title: card.title,
                                })}
                                className="size-full rounded-md border border-border object-cover"
                                render={(image) => (
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        aria-label={tf("images.open_for_card", {
                                            index: index + 1,
                                            title: card.title,
                                        })}
                                        onClick={() => setLightboxIndex(index)}
                                        className="aspect-video h-auto w-full max-h-[220px] overflow-hidden rounded-md bg-muted p-0"
                                    >
                                        {image}
                                    </Button>
                                )}
                            />
                        ))}
                    </div>
                )}
                {contentKind === "normal" ? (
                <div
                    ref={bodyRef}
                    data-testid={`card-body-${card.id}`}
                    className={cn(
                        "min-w-0 text-base leading-7",
                        !expanded && hasCompact && "max-h-72",
                        !expanded &&
                            hasCompact &&
                            (compactScrollable
                                ? "overflow-y-auto"
                                : "overflow-hidden"),
                    )}
                >
                    <Suspense fallback={<MarkdownFallback />}>
                        <LazyMarkdownContent content={content} />
                    </Suspense>
                    {hasCompact ? (
                        <div className="mt-2">
                            <Button
                                type="button"
                                variant="outline"
                                size="icon-xs"
                                aria-label={
                                    expanded
                                        ? tf("card.show_compact_aria", {
                                              id: card.id.toString(),
                                          })
                                        : tf("card.expand_aria", {
                                              id: card.id.toString(),
                                          })
                                }
                                aria-expanded={expanded}
                                onClick={() =>
                                    setExpanded((current) => !current)
                                }
                            >
                                <ChevronUpIcon
                                    data-icon="inline-start"
                                    className={cn(
                                        "transition-transform",
                                        !expanded && "rotate-180",
                                    )}
                                />
                            </Button>
                        </div>
                    ) : null}
                </div>
                ) : (
                    <div
                        className="flex flex-1 flex-col gap-3 rounded-md border border-destructive/40 bg-destructive/10 p-3"
                        data-card-error={
                            contentKind === "api_key_missing"
                                ? "api-key"
                                : "llm"
                        }
                        data-testid={`card-error-${card.id}`}
                        role="alert"
                    >
                        <div className="flex items-start gap-3">
                            <CircleAlertIcon className="mt-0.5 size-4 text-destructive" />
                            <div className="min-w-0 flex-1 space-y-1">
                                <p className="text-sm font-semibold text-destructive">
                                    {contentKind === "api_key_missing"
                                        ? t("card.error.api_key_title")
                                        : t("card.error.llm_title")}
                                </p>
                                <p className="text-sm text-muted-foreground">
                                    {contentKind === "api_key_missing"
                                        ? t("card.error.api_key_summary")
                                        : t("card.error.llm_summary")}
                                </p>
                            </div>
                        </div>
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                variant="outline"
                                size="xs"
                                onClick={() =>
                                    setErrorExpanded((current) => !current)
                                }
                            >
                                {errorExpanded
                                    ? t("card.error.collapse")
                                    : t("card.error.expand")}
                            </Button>
                            <Button
                                type="button"
                                variant="outline"
                                size="xs"
                                onClick={() => {
                                    void navigator.clipboard
                                        ?.writeText(errorDetail)
                                        .then(() => {
                                            setErrorCopied(true);
                                            window.setTimeout(
                                                () => setErrorCopied(false),
                                                1200,
                                            );
                                        });
                                }}
                            >
                                <CopyIcon data-icon="inline-start" />
                                {errorCopied
                                    ? t("common.copied")
                                    : t("card.error.copy")}
                            </Button>
                        </div>
                        {errorExpanded ? (
                            <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words font-mono text-xs">
                                {errorDetail}
                            </pre>
                        ) : null}
                    </div>
                )}
                <ImageLightbox
                    open={lightboxIndex !== null}
                    images={view.imageUrls.map((url) => ({
                        src: url,
                        alt: tf("images.illustration_for_card", {
                            title: card.title,
                        }),
                    }))}
                    initialIndex={lightboxIndex ?? 0}
                    onOpenChange={(open) => {
                        if (!open) setLightboxIndex(null);
                    }}
                />
            </CardContent>
        </>
    );
}

function ReviewButton({
    cardId,
    choice,
    disabled,
    onPick,
}: {
    cardId: bigint;
    choice: ReviewChoice;
    disabled: boolean;
    onPick: (cardId: bigint, choice: ReviewChoice) => void;
}) {
    const ChoiceIcon =
        choice.id === "dismiss"
            ? XIcon
            : choice.id === "acknowledge"
              ? CheckIcon
              : null;
    return (
        <Button
            variant={
                choice.id === "acknowledge" ||
                choice.id === "learned" ||
                choice.id === "easy"
                    ? "default"
                    : "outline"
            }
            className="min-w-0 flex-1"
            disabled={disabled}
            aria-label={tf("flow.review_action_aria", {
                action: reviewActionLabel(choice),
                id: cardId.toString(),
            })}
            data-testid={`review-${choice.id}-${cardId}`}
            onClick={() => onPick(cardId, choice)}
        >
            {ChoiceIcon === null ? null : (
                <ChoiceIcon data-icon="inline-start" />
            )}
            {reviewActionLabel(choice)}
        </Button>
    );
}

function PinControl({
    card,
    pinState,
    disabled,
    onToggle,
    onRetry,
}: {
    card: FlowCardInfo;
    pinState: ReturnType<typeof pinCardState>;
    disabled: boolean;
    onToggle: (cardId: bigint, targetPinned: boolean) => void;
    onRetry: (cardId: bigint) => void;
}) {
    const id = card.id;
    const label = tf("card.pin_aria", {
        action: card.pinned ? t("card.unpin") : t("card.pin"),
        id: id.toString(),
    });
    const buttonDisabled = disabled || pinState.kind !== "idle";
    const button = (
        <Button
            variant="outline"
            size="icon"
            disabled={buttonDisabled}
            aria-label={label}
            aria-pressed={card.pinned}
            data-testid={`pin-${id}`}
            onClick={() => onToggle(id, !card.pinned)}
        >
            {card.pinned ? (
                <PinOffIcon data-icon="inline-start" />
            ) : (
                <PinIcon data-icon="inline-start" />
            )}
        </Button>
    );
    return (
        <>
            {pinState.kind === "error" ? (
                <Alert variant="destructive" data-testid={`pin-error-${id}`}>
                    <AlertTitle>{t("card.pin_error")}</AlertTitle>
                    <AlertDescription
                        role="alert"
                        className="flex flex-col items-start gap-3"
                    >
                        {pinState.message}
                        <Button
                            size="sm"
                            variant="outline"
                            aria-label={tf("card.pin_retry_aria", {
                                action: pinState.attempt.targetPinned
                                    ? t("card.pinning")
                                    : t("card.unpinning"),
                                id: id.toString(),
                            })}
                            data-testid={`pin-retry-${id}`}
                            onClick={() => onRetry(id)}
                        >
                            {t("common.retry")}
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : (
                <>
                    {buttonDisabled ? (
                        button
                    ) : (
                        <Tooltip>
                            <TooltipTrigger render={button} />
                            <TooltipContent>
                                {card.pinned ? t("card.unpin") : t("card.pin")}
                            </TooltipContent>
                        </Tooltip>
                    )}
                    {pinState.kind === "saving" ? (
                        <span
                            role="status"
                            data-testid={`pin-saving-${id}`}
                            className="text-xs text-muted-foreground"
                        >
                            {t("common.saving")}
                        </span>
                    ) : null}
                </>
            )}
        </>
    );
}

/**
 * Card delete control in the control area: a destructive dropdown item that
 * opens an explicit accessible confirmation. While a delete is in flight the
 * trigger disables; a persistent failure renders an Alert with Retry.
 */
function DeleteControl({
    cardId,
    deleteState,
    disabled,
    onConfirm,
    onRetry,
}: {
    cardId: bigint;
    deleteState: DeleteCardState;
    disabled: boolean;
    onConfirm: (cardId: bigint) => void;
    onRetry: (cardId: bigint) => void;
}) {
    const [confirmOpen, setConfirmOpen] = useState(false);
    if (deleteState.kind === "error") {
        return (
            <Alert variant="destructive" data-testid={`delete-error-${cardId}`}>
                <AlertTitle>{t("card.delete_error")}</AlertTitle>
                <AlertDescription
                    role="alert"
                    className="flex flex-col items-start gap-3"
                >
                    {deleteState.message}
                    <Button
                        size="sm"
                        variant="outline"
                        aria-label={tf("card.delete_retry_aria", {
                            id: cardId.toString(),
                        })}
                        data-testid={`delete-retry-${cardId}`}
                        onClick={() => onRetry(cardId)}
                    >
                        {t("common.retry")}
                    </Button>
                </AlertDescription>
            </Alert>
        );
    }
    const busy = deleteState.kind === "deleting";
    return (
        <>
            <DropdownMenu>
                <DropdownMenuTrigger
                    render={<Button variant="outline" size="icon" />}
                    disabled={disabled}
                    aria-label={tf("card.more_actions_aria", {
                        id: cardId.toString(),
                    })}
                    data-testid={`card-more-${cardId}`}
                >
                    <MoreHorizontalIcon data-icon="inline-start" />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start">
                    <DropdownMenuGroup>
                        <DropdownMenuItem
                            variant="destructive"
                            aria-label={tf("card.delete_aria", {
                                id: cardId.toString(),
                            })}
                            data-testid={`delete-card-${cardId}`}
                            onClick={() => setConfirmOpen(true)}
                        >
                            <Trash2Icon data-icon="inline-start" />
                            {t("card.delete")}
                        </DropdownMenuItem>
                    </DropdownMenuGroup>
                </DropdownMenuContent>
            </DropdownMenu>
            {busy ? (
                <span
                    role="status"
                    data-testid={`delete-saving-${cardId}`}
                    className="text-xs text-muted-foreground"
                >
                    {t("common.deleting")}
                </span>
            ) : null}
            <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {t("confirm.delete_card_title")}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {t("confirm.delete_card_description")}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <AlertDialogCancel
                            aria-label={tf("confirm.cancel_delete_aria", {
                                id: cardId.toString(),
                            })}
                            data-testid={`delete-cancel-${cardId}`}
                        >
                            {t("common.cancel")}
                        </AlertDialogCancel>
                        <AlertDialogAction
                            variant="destructive"
                            aria-label={tf("confirm.delete_card_aria", {
                                id: cardId.toString(),
                            })}
                            data-testid={`delete-confirm-${cardId}`}
                            onClick={() => {
                                setConfirmOpen(false);
                                onConfirm(cardId);
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

function SlotReviewControls({
    slot,
    onReview,
    controlsBusy,
}: {
    slot: Extract<ReviewSlot, { kind: "idle" | "error" }>;
    onReview: (cardId: bigint, choice: ReviewChoice) => void;
    controlsBusy: boolean;
}) {
    const card = slot.card;
    const id = card.id;
    const actions = reviewActionsFor(card);
    const disabled = controlsBusy;
    return (
        <div className="flex min-w-0 flex-1 gap-2">
            {actions.primary.map((choice) => (
                <ReviewButton
                    key={choice.id}
                    cardId={id}
                    choice={choice}
                    disabled={disabled}
                    onPick={onReview}
                />
            ))}
            {actions.skipGroup ? (
                <DropdownMenu>
                    <DropdownMenuTrigger
                        render={
                            <Button
                                variant="outline"
                                size="icon"
                                className="[&_svg]:transition-transform aria-expanded:[&_svg]:rotate-180"
                            />
                        }
                        disabled={disabled}
                        openOnHover
                        delay={0}
                        closeDelay={150}
                        aria-label={tf("flow.skip_aria", {
                            id: id.toString(),
                        })}
                        data-testid={`review-skip-${id}`}
                    >
                        <ChevronUpIcon data-icon="inline-start" />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" side="top">
                        <DropdownMenuGroup>
                            <DropdownMenuLabel>
                                {t("flow.skip_reasons")}
                            </DropdownMenuLabel>
                            {actions.skipGroup.map((choice) => (
                                <DropdownMenuItem
                                    key={choice.id}
                                    aria-label={tf("flow.skip_action_aria", {
                                        action: reviewActionLabel(choice),
                                        id: id.toString(),
                                    })}
                                    data-testid={`review-skip-${choice.id}-${id}`}
                                    onClick={() => onReview(id, choice)}
                                >
                                    {reviewActionLabel(choice)}
                                </DropdownMenuItem>
                            ))}
                        </DropdownMenuGroup>
                    </DropdownMenuContent>
                </DropdownMenu>
            ) : null}
        </div>
    );
}

function LiveCardControls({
    slot,
    pinState,
    deleteState,
    controlsBusy,
    pinBusy,
    deleteBusy,
    onReview,
    onPinToggle,
    onPinRetry,
    onDeleteConfirm,
    onDeleteRetry,
}: {
    slot: Extract<ReviewSlot, { kind: "idle" | "reviewing" | "error" }>;
    pinState: ReturnType<typeof pinCardState>;
    deleteState: DeleteCardState;
    controlsBusy: boolean;
    pinBusy: boolean;
    deleteBusy: boolean;
    onReview: (cardId: bigint, choice: ReviewChoice) => void;
    onPinToggle: (cardId: bigint, targetPinned: boolean) => void;
    onPinRetry: (cardId: bigint) => void;
    onDeleteConfirm: (cardId: bigint) => void;
    onDeleteRetry: (cardId: bigint) => void;
}) {
    const id = slot.card.id;
    return (
        <>
            {slot.kind === "idle" && slot.card.status === "active" ? (
                <SlotReviewControls
                    slot={slot}
                    onReview={onReview}
                    controlsBusy={controlsBusy}
                />
            ) : (
                <span className="min-w-0 flex-1 text-sm font-medium text-muted-foreground">
                    {t("flow.review_saved")}
                </span>
            )}
            <PinControl
                card={slot.card}
                pinState={pinState}
                disabled={slot.kind !== "idle" || deleteBusy}
                onToggle={onPinToggle}
                onRetry={onPinRetry}
            />
            <DeleteControl
                cardId={id}
                deleteState={deleteState}
                disabled={slot.kind !== "idle" || pinBusy || deleteBusy}
                onConfirm={onDeleteConfirm}
                onRetry={onDeleteRetry}
            />
        </>
    );
}

function isLiveReviewSlot(
    slot: ReviewSlot,
): slot is Extract<ReviewSlot, { kind: "idle" | "reviewing" | "error" }> {
    return (
        slot.kind === "idle" ||
        slot.kind === "reviewing" ||
        slot.kind === "error"
    );
}

function slotIsRepeatable(slot: ReviewSlot): boolean {
    return isLiveReviewSlot(slot)
        ? slot.card.tipcardType === "repeatable_tip"
        : slot.tipcardType === "repeatable_tip";
}

function useContinueElapsedSeconds(startedAt: number): number {
    const [now, setNow] = useState(() => Date.now());
    useEffect(() => {
        const timer = setInterval(() => setNow(Date.now()), 1000);
        return () => clearInterval(timer);
    }, []);
    return continueElapsedSeconds(startedAt, now);
}

function ContinuingAlertBody({
    topicName,
    startedAt,
}: {
    topicName: string;
    startedAt: number;
}) {
    const elapsed = useContinueElapsedSeconds(startedAt);
    return (
        <AlertDescription role="status">
            {continuingStatusText(elapsed, topicName)}
        </AlertDescription>
    );
}

function ContinuingInlineStatus({
    topicName,
    startedAt,
}: {
    topicName: string;
    startedAt: number;
}) {
    const elapsed = useContinueElapsedSeconds(startedAt);
    return (
        <span
            className="min-w-0 flex-1 text-sm font-medium text-muted-foreground"
            role="status"
        >
            {continuingStatusText(elapsed, topicName)}
        </span>
    );
}

function ReviewSlotFollowUp({
    slot,
    onContinue,
}: {
    slot: Extract<
        ReviewSlot,
        {
            kind:
                | "completed"
                | "awaitingRefill"
                | "continuing"
                | "continueError";
        }
    >;
    onContinue: (reviewedCardId: bigint) => void;
}) {
    if (slot.kind === "completed") {
        return (
            <Alert data-testid={`review-completed-${slot.reviewedCardId}`}>
                <AlertTitle>{t("flow.card_completed")}</AlertTitle>
                <AlertDescription className="flex flex-col items-start gap-3">
                    {t("flow.card_completed_description")}
                    {slot.tipcardType === "repeatable_tip" ? (
                        <Button
                            size="sm"
                            aria-label={tf("flow.continue_aria", {
                                topic: slot.topicName,
                            })}
                            data-testid={`continue-${slot.reviewedCardId}`}
                            onClick={() => onContinue(slot.reviewedCardId)}
                        >
                            {t("common.continue")}
                        </Button>
                    ) : null}
                </AlertDescription>
            </Alert>
        );
    }
    if (slot.kind === "awaitingRefill") {
        return (
            <Alert
                data-testid={`review-awaiting-refill-${slot.reviewedCardId}`}
            >
                <AlertTitle>{t("flow.preparing_next_title")}</AlertTitle>
                <AlertDescription>
                    {t("flow.preparing_next_description")}
                </AlertDescription>
            </Alert>
        );
    }
    if (slot.kind === "continuing") {
        return (
            <Alert
                aria-busy="true"
                data-testid={`continue-saving-${slot.reviewedCardId}`}
            >
                <AlertTitle>{t("flow.continuing_title")}</AlertTitle>
                <ContinuingAlertBody
                    topicName={slot.topicName}
                    startedAt={slot.startedAt}
                />
            </Alert>
        );
    }
    return (
        <Alert
            variant="destructive"
            data-testid={`continue-error-${slot.reviewedCardId}`}
        >
            <AlertTitle>{t("flow.continue_retry_title")}</AlertTitle>
            <AlertDescription
                role="alert"
                className="flex flex-col items-start gap-3"
            >
                {slot.message}
                <Button
                    size="sm"
                    variant="outline"
                    aria-label={tf("flow.continue_retry_aria", {
                        topic: slot.topicName,
                    })}
                    data-testid={`continue-retry-${slot.reviewedCardId}`}
                    onClick={() => onContinue(slot.reviewedCardId)}
                >
                    {t("common.retry")}
                </Button>
            </AlertDescription>
        </Alert>
    );
}

function FollowUpActions({
    slot,
    onContinue,
}: {
    slot: Extract<
        ReviewSlot,
        {
            kind:
                | "completed"
                | "awaitingRefill"
                | "continuing"
                | "continueError";
        }
    >;
    onContinue: (reviewedCardId: bigint) => void;
}) {
    if (slot.kind === "completed" && slot.tipcardType === "repeatable_tip") {
        return (
            <Button
                size="sm"
                aria-label={tf("flow.continue_aria", {
                    topic: slot.topicName,
                })}
                data-testid={`continue-${slot.reviewedCardId}`}
                onClick={() => onContinue(slot.reviewedCardId)}
            >
                {t("common.continue")}
            </Button>
        );
    }
    if (slot.kind === "continueError") {
        return (
            <Button
                size="sm"
                variant="outline"
                aria-label={tf("flow.continue_retry_aria", {
                    topic: slot.topicName,
                })}
                data-testid={`continue-retry-${slot.reviewedCardId}`}
                onClick={() => onContinue(slot.reviewedCardId)}
            >
                {t("common.retry")}
            </Button>
        );
    }
    if (slot.kind === "continuing") {
        return (
            <ContinuingInlineStatus
                topicName={slot.topicName}
                startedAt={slot.startedAt}
            />
        );
    }
    if (slot.kind === "awaitingRefill") {
        return (
            <span className="min-w-0 flex-1 text-sm font-medium text-muted-foreground">
                {t("flow.preparing_next_description")}
            </span>
        );
    }
    return null;
}

const ReviewSlotCard = memo(function ReviewSlotCard({
    slot,
    onReview,
    onRetry,
    onContinue,
    pinCard,
    deleteCard,
    onPinToggle,
    onPinRetry,
    onDeleteConfirm,
    onDeleteRetry,
    enableDrag = false,
    draggingCardId = null,
    onPinnedDragStart,
    onPinnedDragEnd,
    flowActive = true,
}: {
    slot: ReviewSlot;
    onReview: (cardId: bigint, choice: ReviewChoice) => void;
    onRetry: (cardId: bigint) => void;
    onContinue: (reviewedCardId: bigint) => void;
    pinCard: PinCardState;
    deleteCard: DeleteCardState;
    onPinToggle: (cardId: bigint, targetPinned: boolean) => void;
    onPinRetry: (cardId: bigint) => void;
    onDeleteConfirm: (cardId: bigint) => void;
    onDeleteRetry: (cardId: bigint) => void;
    enableDrag?: boolean;
    draggingCardId?: bigint | null;
    onPinnedDragStart: (cardId: bigint) => void;
    onPinnedDragEnd: (cardId: bigint) => void;
    flowActive?: boolean;
}) {
    const live = isLiveReviewSlot(slot);
    const repeatable = slotIsRepeatable(slot);
    const liveCard = live ? slot.card : null;
    const [detailOpen, setDetailOpen] = useState(false);
    const [card, setCard] = useState<FlowCardInfo | null>(liveCard);

    useEffect(() => {
        if (liveCard !== null) setCard(liveCard);
    }, [liveCard]);

    useEffect(() => {
        if (!live && !repeatable) setDetailOpen(false);
    }, [live, repeatable]);

    const onDetailOpenChange = useCallback(
        (next: boolean, reason?: string) => {
            if (
                !next &&
                reason !== undefined &&
                !isUserFullscreenDismiss(reason)
            ) {
                return;
            }
            setDetailOpen(next);
        },
        [],
    );

    const id = live ? slot.card.id : slot.reviewedCardId;
    const pinBusy = pinCard.kind !== "idle";
    const deleteBusy = deleteCard.kind === "deleting";
    const controlsBusy = pinBusy || deleteBusy;
    const dragEnabled = live && enableDrag && slot.card.pinned;
    const stackLayers = live ? repeatableStackLayers(slot.card) : 0;
    const reviewSwipe = live ? repeatableReviewSwipe(slot) : undefined;
    const liveControls = live ? (
        <LiveCardControls
            slot={slot}
            pinState={pinCard}
            deleteState={deleteCard}
            controlsBusy={controlsBusy}
            pinBusy={pinBusy}
            deleteBusy={deleteBusy}
            onReview={onReview}
            onPinToggle={onPinToggle}
            onPinRetry={onPinRetry}
            onDeleteConfirm={onDeleteConfirm}
            onDeleteRetry={onDeleteRetry}
        />
    ) : null;
    const showDetail = card !== null && (live || repeatable);
    const detailActions = live ? (
        liveControls
    ) : (
        <FollowUpActions slot={slot} onContinue={onContinue} />
    );

    return (
        <Dialog
            modal={false}
            open={detailOpen && flowActive}
            onOpenChange={(next, details) =>
                onDetailOpenChange(next, details.reason)
            }
        >
            {live ? (
                <div
                    className={cn(
                        "relative isolate flex flex-col",
                        stackLayers > 0 && "mr-3 mb-3",
                    )}
                    data-repeatable-stack={
                        stackLayers > 0 ? stackLayers : undefined
                    }
                >
                    {Array.from({ length: stackLayers }, (_, index) => {
                        const layer = index + 1;
                        return (
                            <div
                                key={layer}
                                aria-hidden="true"
                                data-stack-layer={layer}
                                className={cn(
                                    "absolute inset-0 rounded-md border border-border bg-card shadow-sm",
                                    layer === 1 &&
                                        "translate-x-1 translate-y-1 opacity-85",
                                    layer === 2 &&
                                        "translate-x-2 translate-y-2 opacity-70",
                                    layer === 3 &&
                                        "translate-x-3 translate-y-3 opacity-55",
                                )}
                                style={{ zIndex: -layer }}
                            />
                        );
                    })}
                    <Card
                        className={cn(
                            "relative z-10 flex flex-col gap-0 overflow-hidden rounded-xl py-0 ring-border",
                            reviewSwipe !== undefined &&
                                "repeatable-review-swipe",
                        )}
                        data-review-swipe={reviewSwipe}
                    >
                        <CardBodies
                            card={card ?? slot.card}
                            detailTrigger={
                                <FlowCardDetailTrigger
                                    cardId={(card ?? slot.card).id}
                                />
                            }
                            detailActions={liveControls}
                            leading={
                                dragEnabled ? (
                                    <PinnedDragHandle
                                        cardId={id}
                                        dragging={draggingCardId === id}
                                        onDragStart={onPinnedDragStart}
                                        onDragEnd={onPinnedDragEnd}
                                        disabled={deleteBusy}
                                    />
                                ) : undefined
                            }
                        />
                        {slot.kind === "error" ? (
                            <CardContent className="flex flex-col gap-3 px-4 pb-4">
                                <Alert variant="destructive">
                                    <AlertTitle>
                                        {t("flow.review_error")}
                                    </AlertTitle>
                                    <AlertDescription role="alert">
                                        {slot.message}
                                    </AlertDescription>
                                </Alert>
                                <Button
                                    size="sm"
                                    aria-label={tf("flow.review_retry_aria", {
                                        id: id.toString(),
                                    })}
                                    data-testid={`review-retry-${id}`}
                                    onClick={() => onRetry(id)}
                                >
                                    {t("common.retry")}
                                </Button>
                            </CardContent>
                        ) : null}
                        <CardFooter
                            className="gap-2"
                            data-testid={`card-actions-${id}`}
                        >
                            {liveControls}
                        </CardFooter>
                    </Card>
                </div>
            ) : (
                <ReviewSlotFollowUp slot={slot} onContinue={onContinue} />
            )}
            {showDetail && card !== null ? (
                <FlowCardDetail
                    card={card}
                    open={detailOpen}
                    onOpenChange={(next) => onDetailOpenChange(next)}
                    onCardChanged={setCard}
                    actions={detailActions}
                />
            ) : null}
        </Dialog>
    );
});

const SlotList = memo(function SlotList({
    slots,
    testId,
    labelledBy,
    layoutClasses,
    onReview,
    onRetry,
    onContinue,
    pinStates,
    deleteStates,
    onPinToggle,
    onPinRetry,
    onDeleteConfirm,
    onDeleteRetry,
    enableDrag = false,
    draggingCardId = null,
    onPinnedDragStart,
    onPinnedDragEnd,
    onPinnedDrop,
    flowActive = true,
}: {
    slots: ReviewSlot[];
    testId: string;
    labelledBy?: string;
    layoutClasses: string;
    onReview: (cardId: bigint, choice: ReviewChoice) => void;
    onRetry: (cardId: bigint) => void;
    onContinue: (reviewedCardId: bigint) => void;
    pinStates: PinState;
    deleteStates: DeleteState;
    onPinToggle: (cardId: bigint, targetPinned: boolean) => void;
    onPinRetry: (cardId: bigint) => void;
    onDeleteConfirm: (cardId: bigint) => void;
    onDeleteRetry: (cardId: bigint) => void;
    enableDrag?: boolean;
    draggingCardId?: bigint | null;
    onPinnedDragStart: (cardId: bigint) => void;
    onPinnedDragEnd: (cardId: bigint) => void;
    onPinnedDrop?: (
        sourceId: bigint,
        targetId: bigint,
    ) => readonly bigint[] | null;
    flowActive?: boolean;
}) {
    return (
        <ul
            className={layoutClasses}
            data-testid={testId}
            aria-labelledby={labelledBy}
        >
            {slots.map((slot, index) => {
                const id = slotMetadata(slot).id;
                return (
                    <li
                        key={flowSlotKey(slot)}
                        data-testid={`flow-slot-${id}`}
                        style={enableDrag ? { order: index } : undefined}
                        aria-posinset={enableDrag ? index + 1 : undefined}
                        aria-setsize={enableDrag ? slots.length : undefined}
                        className={cn(
                            "min-w-0",
                            draggingCardId === id && "opacity-50",
                        )}
                        onDragOver={
                            onPinnedDrop
                                ? (event) => {
                                      event.preventDefault();
                                      event.dataTransfer.dropEffect = "move";
                                  }
                                : undefined
                        }
                        onDrop={
                            onPinnedDrop
                                ? (event) => {
                                      event.preventDefault();
                                      const source = parsePinnedCardId(
                                          event.dataTransfer.getData(
                                              "text/plain",
                                          ),
                                      );
                                      if (source === null) return;
                                      const next = onPinnedDrop(source, id);
                                      const list =
                                          event.currentTarget.parentElement;
                                      if (next === null || list === null)
                                          return;
                                      const items = new Map(
                                          Array.from(list.children).map(
                                              (item) => [
                                                  item.getAttribute(
                                                      "data-testid",
                                                  ),
                                                  item,
                                              ],
                                          ),
                                      );
                                      for (const [
                                          index,
                                          nextId,
                                      ] of next.entries()) {
                                          const item = items.get(
                                              `flow-slot-${nextId}`,
                                          );
                                          if (item === undefined) continue;
                                          item.setAttribute(
                                              "style",
                                              `order: ${index}`,
                                          );
                                          item.setAttribute(
                                              "aria-posinset",
                                              String(index + 1),
                                          );
                                          item.setAttribute(
                                              "aria-setsize",
                                              String(next.length),
                                          );
                                      }
                                  }
                                : undefined
                        }
                    >
                        <ReviewSlotCard
                            slot={slot}
                            onReview={onReview}
                            onRetry={onRetry}
                            onContinue={onContinue}
                            pinCard={pinCardState(pinStates, id)}
                            deleteCard={deleteCardState(deleteStates, id)}
                            onPinToggle={onPinToggle}
                            onPinRetry={onPinRetry}
                            onDeleteConfirm={onDeleteConfirm}
                            onDeleteRetry={onDeleteRetry}
                            enableDrag={enableDrag}
                            draggingCardId={draggingCardId}
                            onPinnedDragStart={onPinnedDragStart}
                            onPinnedDragEnd={onPinnedDragEnd}
                            flowActive={flowActive}
                        />
                    </li>
                );
            })}
        </ul>
    );
});

function TransmissionSections({
    pinned,
    unpinned,
    layoutClasses,
    onReview,
    onRetry,
    onContinue,
    pinStates,
    deleteStates,
    onPinToggle,
    onPinRetry,
    onDeleteConfirm,
    onDeleteRetry,
    draggingCardId = null,
    onPinnedDragStart,
    onPinnedDragEnd,
    onPinnedDrop,
    flowActive = true,
}: {
    pinned: ReviewSlot[];
    unpinned: ReviewSlot[];
    layoutClasses: string;
    onReview: (cardId: bigint, choice: ReviewChoice) => void;
    onRetry: (cardId: bigint) => void;
    onContinue: (reviewedCardId: bigint) => void;
    pinStates: PinState;
    deleteStates: DeleteState;
    onPinToggle: (cardId: bigint, targetPinned: boolean) => void;
    onPinRetry: (cardId: bigint) => void;
    onDeleteConfirm: (cardId: bigint) => void;
    onDeleteRetry: (cardId: bigint) => void;
    draggingCardId?: bigint | null;
    onPinnedDragStart: (cardId: bigint) => void;
    onPinnedDragEnd: (cardId: bigint) => void;
    onPinnedDrop: (
        sourceId: bigint,
        targetId: bigint,
    ) => readonly bigint[] | null;
    flowActive?: boolean;
}) {
    const { picks, remaining } = splitTopicPicks(unpinned);
    const list = {
        layoutClasses,
        onReview,
        onRetry,
        onContinue,
        pinStates,
        deleteStates,
        onPinToggle,
        onPinRetry,
        onDeleteConfirm,
        onDeleteRetry,
        flowActive,
        draggingCardId,
        onPinnedDragStart,
        onPinnedDragEnd,
    };
    return (
        <>
            {pinned.length > 0 ? (
                <section id="flow-pins" className="mb-8">
                    <div className="mb-4 flex items-baseline gap-2">
                        <h2
                            id="flow-pinned-heading"
                            className="text-lg font-semibold tracking-tight"
                        >
                            {t("card.pinned")}
                        </h2>
                        <span className="text-sm text-muted-foreground">
                            {pinned.length}
                        </span>
                    </div>
                    <SlotList
                        slots={pinned}
                        testId="flow-pinned-grid"
                        labelledBy="flow-pinned-heading"
                        enableDrag
                        onPinnedDrop={onPinnedDrop}
                        {...list}
                    />
                </section>
            ) : null}
            <section aria-labelledby="flow-picks-heading">
                <div className="mb-4">
                    <h2
                        id="flow-picks-heading"
                        className="text-lg font-semibold tracking-tight"
                    >
                        {t("flow.picks")}
                    </h2>
                    <div className="mt-1 text-sm text-muted-foreground">
                        <span id="flow-count">
                            {tf(
                                picks.length === 1
                                    ? "flow.picks_showing_one"
                                    : "flow.picks_showing_other",
                                { count: picks.length },
                            )}
                        </span>
                    </div>
                    {picks.length === 0 ? null : (
                        <p className="mt-1 text-sm text-muted-foreground">
                            {t("flow.picks_advance_hint")}
                        </p>
                    )}
                </div>
                <SlotList
                    slots={picks}
                    testId="flow-grid"
                    labelledBy="flow-picks-heading"
                    {...list}
                />
            </section>
            {remaining.length > 0 ? (
                <section
                    className="mt-8"
                    aria-labelledby="flow-other-cards-heading"
                    data-testid="flow-other-cards"
                >
                    <div className="mb-4 flex items-center justify-between gap-3">
                        <h2
                            id="flow-other-cards-heading"
                            className="text-lg font-semibold tracking-tight"
                        >
                            {t("flow.other_cards")}
                        </h2>
                        <span className="text-sm text-muted-foreground">
                            {remaining.length}
                        </span>
                    </div>
                    <SlotList
                        slots={remaining}
                        testId="flow-other-grid"
                        labelledBy="flow-other-cards-heading"
                        {...list}
                    />
                </section>
            ) : null}
        </>
    );
}

function LoadingSkeletons({
    count,
    layoutClasses,
}: {
    count: number;
    layoutClasses: string;
}) {
    return (
        <ul
            className={layoutClasses}
            data-testid="flow-loading-skeletons"
            aria-hidden="true"
        >
            {Array.from({ length: count }, (_, index) => (
                <li key={index}>
                    <Skeleton className="h-64 w-full rounded-xl" />
                </li>
            ))}
        </ul>
    );
}

/** Transmission heading above the add form. */
function TransmissionHeader() {
    return (
        <div className="mb-4">
            <h1 className="text-xl font-semibold tracking-tight">
                {t("flow.transmission")}
            </h1>
            <p className="text-muted-foreground mt-2">
                {t("flow.transmission_description")}
            </p>
        </div>
    );
}

function TransmissionToolbar({
    children,
    ...formProps
}: React.ComponentProps<typeof FlowAddForm> & {
    children?: React.ReactNode;
}) {
    return (
        <div
            className="flow-toolbar relative z-30 mb-4 flex w-full min-w-0 max-w-full flex-col items-end gap-3 lg:sticky lg:top-3"
            data-testid="flow-toolbar"
        >
            <FlowAddForm {...formProps} />
            {children}
        </div>
    );
}

/** Honest success feedback once created cards are integrated/visible. */
function AddedNotice() {
    return (
        <Alert className="mb-4" data-testid="add-success">
            <AlertTitle role="status">{t("toast.cards_added")}</AlertTitle>
        </Alert>
    );
}

export function Flow({ active = true }: { active?: boolean }) {
    const [state, setState] = useState<FlowState>(
        () => seedFromSnapshot() ?? { kind: "initial-loading" },
    );
    // Ref mirror of the latest state so event handlers read current data
    // without launching work inside a setState updater (updaters must stay
    // pure — React may invoke them more than once).
    const stateRef = useRef<FlowState>(state);
    // Component lifetime marker shared by every async handler; the pager owns
    // its own generation/in-flight guards internally.
    const mountedRef = useRef(true);
    // Layout preferences: SSR-safe lazy init reads localStorage only in the
    // browser; valid user changes persist under the canonical shared keys.
    // Sort preference: SSR-safe lazy init reads localStorage only in the
    // browser; valid user changes persist under the canonical shared key.
    const [sortMode, setSortMode] = useState<FlowSortMode>(() =>
        parseFlowSortMode(
            typeof window === "undefined"
                ? null
                : window.localStorage.getItem(FLOW_SORT_STORAGE_KEY),
        ),
    );
    const [layout, setLayout] = useState<FlowLayout>(() =>
        parseFlowLayout(
            typeof window === "undefined"
                ? null
                : window.localStorage.getItem(FLOW_LAYOUT_STORAGE_KEY),
        ),
    );
    const [gridColumns, setGridColumns] = useState<GridColumns>(() =>
        parseGridColumns(
            typeof window === "undefined"
                ? null
                : window.localStorage.getItem(FLOW_GRID_COLUMNS_STORAGE_KEY),
        ),
    );
    const [columnsMenuOpen, setColumnsMenuOpen] = useState(false);
    // Saved pinned drag order (raw bigint IDs). The ref mirrors the latest
    // value so reconciliation effects and drop handlers read current data.
    const [pinnedOrder, setPinnedOrder] = useState<readonly bigint[]>(() =>
        typeof window === "undefined"
            ? []
            : (parsePinnedCardOrder(
                  window.localStorage.getItem(PINNED_CARD_ORDER_STORAGE_KEY),
              ) ?? []),
    );
    const pinnedOrderRef = useRef<readonly bigint[]>(pinnedOrder);
    const persistPinnedOrder = useCallback((next: readonly bigint[]) => {
        pinnedOrderRef.current = next;
        window.localStorage.setItem(
            PINNED_CARD_ORDER_STORAGE_KEY,
            serializePinnedCardOrder([...next]),
        );
    }, []);
    const applyPinnedOrder = useCallback(
        (next: readonly bigint[]) => {
            if (next === pinnedOrderRef.current) return;
            persistPinnedOrder(next);
            setPinnedOrder(next);
        },
        [persistPinnedOrder],
    );
    const onSortChange = useCallback((value: readonly string[]) => {
        const next = value.at(0);
        // Base UI emits an empty array when the active item is toggled off.
        // Reject it so this controlled single-selection group always has a mode.
        if (next === undefined) return;
        const mode = parseFlowSortMode(next);
        window.localStorage.setItem(FLOW_SORT_STORAGE_KEY, mode);
        setSortMode(mode);
    }, []);
    const onGridMenuOpenChange = useCallback((open: boolean) => {
        setColumnsMenuOpen(open);
        if (!open) return;
        // The grid button both selects Grid and toggles its column menu.
        window.localStorage.setItem(FLOW_LAYOUT_STORAGE_KEY, "grid");
        setLayout("grid");
    }, []);
    const onLayoutChange = useCallback((value: readonly string[]) => {
        const next = value.at(0);
        // Base UI emits an empty array when the active item is toggled off.
        // Reject it so one valid layout always remains selected.
        if (next === undefined) return;
        const parsed = parseFlowLayout(next);
        window.localStorage.setItem(FLOW_LAYOUT_STORAGE_KEY, parsed);
        setLayout(parsed);
        if (parsed === "list") setColumnsMenuOpen(false);
    }, []);

    const onColumnsSelect = useCallback((value: string) => {
        const columns = parseGridColumns(value);
        window.localStorage.setItem(
            FLOW_GRID_COLUMNS_STORAGE_KEY,
            String(columns),
        );
        window.localStorage.setItem(FLOW_LAYOUT_STORAGE_KEY, "grid");
        setGridColumns(columns);
        setLayout("grid");
        setColumnsMenuOpen(false);
    }, []);

    // Honest dragging indicator; cleared on drag end and drop.
    const [draggingCardId, setDraggingCardId] = useState<bigint | null>(null);
    const onPinnedDragStart = useCallback((cardId: bigint) => {
        setDraggingCardId(cardId);
    }, []);
    const onPinnedDragEnd = useCallback(() => {
        setDraggingCardId(null);
    }, []);
    /**
     * Reorder: normalize against the currently pinned IDs, then remove
     * source and insert at the original target index. Unknown or equal IDs
     * are no-ops. Runs outside any state updater via refs.
     */
    const onPinnedDrop = useCallback(
        (sourceId: bigint, targetId: bigint) => {
            setDraggingCardId(null);
            const current = stateRef.current;
            if (
                current.kind !== "ready" &&
                current.kind !== "loading-more" &&
                current.kind !== "load-error"
            )
                return null;
            const pinnedIds = current.slots
                .filter((slot) => slotMetadata(slot).pinned)
                .map((slot) => slotMetadata(slot).id);
            const base = normalizeCardOrder(pinnedOrderRef.current, pinnedIds);
            const next = movePinnedCard(base, sourceId, targetId);
            if (next === pinnedOrderRef.current) return null;
            applyPinnedOrder(next);
            return next;
        },
        [applyPinnedOrder],
    );
    const apply = useCallback((next: FlowState) => {
        stateRef.current = next;
        setState(next);
    }, []);

    // ---------------------------------------------------------------------------
    // Extracted lifecycles. The component keeps sole ownership of the Flow
    // state union (`apply`); every hook commits through it atomically.
    // ---------------------------------------------------------------------------
    const getState = useCallback(() => stateRef.current, []);
    const produce = useCallback(
        (produceState: (current: FlowState) => FlowState) => {
            const next = produceState(stateRef.current);
            if (next !== stateRef.current) apply(next);
        },
        [apply],
    );
    const mutationHost = useMemo(
        () => ({
            getState,
            produce,
            getPinnedOrder: () => pinnedOrderRef.current,
            applyPinnedOrder,
            mounted: () => mountedRef.current,
        }),
        [applyPinnedOrder, getState, produce],
    );
    const { pinStates, pinStatesRef, onPinToggle, onPinRetry } =
        usePinMutations(mutationHost);
    const { deleteStates, deleteStatesRef, onDeleteConfirm, onDeleteRetry } =
        useDeleteMutations(mutationHost);
    const pinIdle = useCallback(
        (cardId: bigint) =>
            pinCardState(pinStatesRef.current, cardId).kind === "idle",
        [pinStatesRef],
    );
    const reviewHost = useMemo(
        () => ({
            getState,
            apply,
            mounted: () => mountedRef.current,
            pinIdle,
        }),
        [apply, getState, pinIdle],
    );
    const { onReview, onRetry, onContinue } =
        useFlowReviewLifecycle(reviewHost);

    // ---------------------------------------------------------------------------
    // Add-card form lifecycle.
    /** Card IDs currently owned by pin/delete mutations must not be replaced. */
    const busyCardIds = useCallback((): bigint[] => {
        const ids: bigint[] = [];
        for (const cardState of pinStatesRef.current.values()) {
            if (cardState.kind === "saving") ids.push(cardState.attempt.cardId);
        }
        for (const cardState of deleteStatesRef.current.values()) {
            if (cardState.kind === "deleting")
                ids.push(cardState.attempt.cardId);
        }
        return ids;
    }, []);
    const {
        addState,
        addedNotice,
        isInFlight: addIsInFlight,
        onAddSubmit,
        onAddRetryMutation,
        onAddRetryResolve,
    } = useFlowAddLifecycle({
        getState,
        apply,
        busyCardIds,
        getPinnedOrder: () => pinnedOrderRef.current,
        applyPinnedOrder,
    });




    // ---------------------------------------------------------------------------
    // Flow pager: prefetch-consuming cold start, session snapshot seeding, and
    // silent background refresh. Owns its own generation/in-flight guards;
    // commits flow through the shared `apply`.
    // ---------------------------------------------------------------------------
    const mutationsInFlight = useCallback((): boolean => {
        if (addIsInFlight()) return true;
        for (const pin of pinStatesRef.current.values()) {
            if (pin.kind !== "idle") return true;
        }
        for (const del of deleteStatesRef.current.values()) {
            if (del.kind !== "idle") return true;
        }
        // A pending review/continue owns its slot's optimistic state; a
        // concurrent merge would replace the slot and strand the result.
        const current = stateRef.current;
        if (
            current.kind === "ready" ||
            current.kind === "loading-more" ||
            current.kind === "load-error"
        ) {
            for (const slot of current.slots) {
                if (slot.kind === "reviewing" || slot.kind === "continuing")
                    return true;
            }
        }
        return false;
        // Depends only on stable identities: an unstable dep here would give
        // loadInitial a new identity per render and re-trigger useViewRefresh
        // (which invokes refresh on every identity change) in a hot loop.
    }, [addIsInFlight]);
    const saveSnapshot = useCallback(
        (page: SavedFlowPage) => saveFlowSnapshot(page),
        [],
    );
    const { loadInitial, loadMoreFrom } = useFlowPager({
        getState,
        apply,
        setStateOnly: setState,
        mutationsInFlight,
        saveSnapshot,
    });

    const loadMore = useCallback(() => {
        // Reads current data from the ref; no request launches inside any updater.
        const current = stateRef.current;
        if (current.kind !== "ready" || current.cursor.kind !== "more") return;
        loadMoreFrom(current.slots, current.cursor.pageToken);
    }, [loadMoreFrom]);

    /**
     * Declarative bounded refill polling. Keyed by the current awaiting-refill
     * slots (identity + token), so any state change reschedules exactly one
     * fresh timer per awaiting slot; cleanup clears pending timers and marks
     * in-flight poll results stale so they never commit.
     */
    const trackedSlots =
        state.kind === "ready" ||
        state.kind === "loading-more" ||
        state.kind === "load-error"
            ? state.slots
            : NO_SLOTS;
    const awaitingSlots = useMemo(
        () =>
            trackedSlots.filter(
                (
                    slot,
                ): slot is Extract<ReviewSlot, { kind: "awaitingRefill" }> =>
                    slot.kind === "awaitingRefill",
            ),
        [trackedSlots],
    );
    const organizedSlots = useMemo(
        () => organizeFlowSlots(trackedSlots, sortMode, pinnedOrder),
        [trackedSlots, sortMode, pinnedOrder],
    );
    useEffect(() => {
        if (awaitingSlots.length === 0) return;
        let cancelled = false;
        const timers = awaitingSlots.map((slot) =>
            setTimeout(() => {
                void (async () => {
                    try {
                        const page = await listFlowCards({ pageSize: 100 });
                        if (cancelled || !mountedRef.current) return;
                        const candidate = page.cards.find(
                            (card) =>
                                card.topicName === slot.topicName &&
                                card.tipcardType === "repeatable_tip" &&
                                card.status === "active" &&
                                card.id !== slot.reviewedCardId,
                        );
                        const current = stateRef.current;
                        if (
                            current.kind !== "ready" &&
                            current.kind !== "loading-more" &&
                            current.kind !== "load-error"
                        )
                            return;
                        const stillAwaiting = current.slots.find(
                            (candidateSlot) =>
                                candidateSlot.kind === "awaitingRefill" &&
                                candidateSlot.reviewedCardId ===
                                    slot.reviewedCardId &&
                                candidateSlot.refillToken === slot.refillToken,
                        );
                        if (stillAwaiting === undefined) return;
                        const updated =
                            candidate !== undefined
                                ? refillPollFound(
                                      current.slots,
                                      slot.reviewedCardId,
                                      slot.refillToken,
                                      candidate,
                                  )
                                : refillPollMiss(
                                      current.slots,
                                      slot.reviewedCardId,
                                      slot.refillToken,
                                      REFILL_MAX_ATTEMPTS,
                                  );
                        if (updated === current.slots) return;
                        apply({ ...current, slots: updated });
                    } catch {
                        // A failed read counts as a miss; never crashes the Flow.
                        if (cancelled || !mountedRef.current) return;
                        const current = stateRef.current;
                        if (
                            current.kind !== "ready" &&
                            current.kind !== "loading-more" &&
                            current.kind !== "load-error"
                        )
                            return;
                        const updated = refillPollMiss(
                            current.slots,
                            slot.reviewedCardId,
                            slot.refillToken,
                            REFILL_MAX_ATTEMPTS,
                        );
                        if (updated === current.slots) return;
                        apply({ ...current, slots: updated });
                    }
                })();
            }, REFILL_POLL_DELAY_MS),
        );
        return () => {
            cancelled = true;
            timers.forEach(clearTimeout);
        };
    }, [awaitingSlots, apply]);

    /**
     * Reconcile the saved pinned order with the currently pinned card IDs on
     * every slots change: a pinned live card replaced in its stable
     * review/Continue/refill slot swaps IDs at the same saved-order position;
     * pinning appends; unpinning removes. Initial loads, pagination, stale
     * async results, and unknown IDs cannot corrupt the order.
     */
    useEffect(() => {
        // Do not erase a saved order while the first page is still loading or
        // temporarily unavailable. Reconcile only once the Flow result is known.
        if (state.kind === "initial-loading" || state.kind === "error") return;
        const previous = pinnedOrderRef.current;
        const pinnedIds = trackedSlots
            .filter((slot) => slotMetadata(slot).pinned)
            .map((slot) => slotMetadata(slot).id);
        const removed = previous.filter((id) => !pinnedIds.includes(id));
        const added = pinnedIds.filter((id) => !previous.includes(id));
        const next =
            removed.length === 1 && added.length === 1
                ? replacePinnedCard(previous, removed[0], added[0])
                : previous;
        applyPinnedOrder(normalizeCardOrder(next, pinnedIds));
    }, [state.kind, trackedSlots, applyPinnedOrder]);

    useEffect(() => {
        mountedRef.current = true;
        return () => {
            mountedRef.current = false;
        };
    }, []);
    // Warm the lazy markdown chunk during idle time so expanded cards and
    // detail dialogs never render the Suspense fallback in practice.
    useEffect(() => {
        const warm = () => {
            void import("@/components/content/MarkdownContent");
        };
        if (typeof window.requestIdleCallback === "function") {
            const id = window.requestIdleCallback(warm, { timeout: 5000 });
            return () => window.cancelIdleCallback(id);
        }
        const timer = window.setTimeout(warm, 2000);
        return () => window.clearTimeout(timer);
    }, []);
    useViewRefresh(active, loadInitial);

    switch (state.kind) {
        case "initial-loading":
            return (
                <div data-testid="flow-loading">
                    <TransmissionHeader />
                    <TransmissionToolbar
                        lifecycle={addState}
                        disabled
                        onAdd={onAddSubmit}
                        onRetryMutation={onAddRetryMutation}
                        onRetryResolve={onAddRetryResolve}
                    />
                    <p
                        className="text-sm text-muted-foreground mb-4"
                        role="status"
                    >
                        {t("flow.loading")}
                    </p>
                    <LoadingSkeletons
                        count={8}
                        layoutClasses={
                            layout === "list"
                                ? FLOW_LIST_CLASSES
                                : gridClassesForColumns(gridColumns)
                        }
                    />
                </div>
            );
        case "empty":
            return (
                <div data-testid="flow-empty">
                    <TransmissionHeader />
                    <TransmissionToolbar
                        lifecycle={addState}
                        onAdd={onAddSubmit}
                        onRetryMutation={onAddRetryMutation}
                        onRetryResolve={onAddRetryResolve}
                    />
                    {addedNotice ? <AddedNotice /> : null}
                    <Alert>
                        <AlertTitle>{t("flow.empty_title")}</AlertTitle>
                        <AlertDescription>
                            {t("flow.empty_description")}
                        </AlertDescription>
                    </Alert>
                </div>
            );
        case "error":
            return (
                <div data-testid="flow-error">
                    <TransmissionHeader />
                    <TransmissionToolbar
                        lifecycle={addState}
                        disabled
                        onAdd={onAddSubmit}
                        onRetryMutation={onAddRetryMutation}
                        onRetryResolve={onAddRetryResolve}
                    />
                    <Alert variant="destructive">
                        <AlertTitle>{t("flow.load_error")}</AlertTitle>
                        <AlertDescription role="alert">
                            {state.message}
                        </AlertDescription>
                    </Alert>
                    <Button className="mt-4" onClick={() => void loadInitial()}>
                        {t("common.retry")}
                    </Button>
                </div>
            );
        case "ready":
        case "loading-more":
        case "load-error":
            return (
                    <div
                        data-testid="flow-ready"
                        className={
                            state.kind === "ready" && state.revalidating
                                ? "opacity-80 transition-opacity"
                                : undefined
                        }
                    >
                    <TransmissionHeader />
                    <TransmissionToolbar
                        lifecycle={addState}
                        onAdd={onAddSubmit}
                        onRetryMutation={onAddRetryMutation}
                        onRetryResolve={onAddRetryResolve}
                    >
                        <div className="flex flex-wrap items-center justify-end gap-2">
                            <div className="flex items-center gap-2">
                                <span id="flow-sort-label" className="sr-only">
                                    {t("flow.sort")}
                                </span>
                                <ToggleGroup
                                    variant="outline"
                                    size="sm"
                                    aria-labelledby="flow-sort-label"
                                    value={[sortMode]}
                                    onValueChange={onSortChange}
                                    data-testid="flow-sort"
                                >
                                    <ToggleGroupItem
                                        value="topic"
                                        data-testid="flow-sort-topic"
                                    >
                                        {t("flow.sort_topic")}
                                    </ToggleGroupItem>
                                    <ToggleGroupItem
                                        value="date"
                                        data-testid="flow-sort-date"
                                    >
                                        {t("flow.sort_date")}
                                    </ToggleGroupItem>
                                </ToggleGroup>
                            </div>
                            <ToggleGroup
                                variant="outline"
                                size="sm"
                                spacing={0}
                                aria-label={t("flow.layout_aria")}
                                value={[layout]}
                                onValueChange={onLayoutChange}
                                data-testid="flow-layout"
                            >
                                <DropdownMenu
                                    open={columnsMenuOpen}
                                    onOpenChange={onGridMenuOpenChange}
                                >
                                    <DropdownMenuTrigger
                                        render={
                                            <ToggleGroupItem
                                                value="grid"
                                                aria-label={t(
                                                    "flow.grid_layout_aria",
                                                )}
                                                data-testid="flow-grid-btn"
                                            />
                                        }
                                    >
                                        <LayoutGridIcon data-icon="inline-start" />
                                    </DropdownMenuTrigger>
                                    <DropdownMenuContent align="end">
                                        <DropdownMenuRadioGroup
                                            value={String(gridColumns)}
                                            onValueChange={onColumnsSelect}
                                        >
                                            <div className="grid grid-cols-4 gap-1">
                                                {[1, 2, 3, 4].map((columns) => (
                                                    <DropdownMenuRadioItem
                                                        key={columns}
                                                        value={String(columns)}
                                                        aria-label={tf(
                                                            "flow.columns_aria",
                                                            { count: columns },
                                                        )}
                                                        data-testid={`flow-columns-${columns}`}
                                                    >
                                                        {columns}
                                                    </DropdownMenuRadioItem>
                                                ))}
                                            </div>
                                        </DropdownMenuRadioGroup>
                                    </DropdownMenuContent>
                                </DropdownMenu>
                                <ToggleGroupItem
                                    value="list"
                                    aria-label={t("flow.list_layout")}
                                    data-testid="flow-list-btn"
                                >
                                    <ListIcon data-icon="inline-start" />
                                </ToggleGroupItem>
                            </ToggleGroup>
                        </div>
                    </TransmissionToolbar>
                    {addedNotice ? <AddedNotice /> : null}
                    <TransmissionSections
                        {...organizedSlots}
                        flowActive={active}
                        layoutClasses={
                            layout === "list"
                                ? FLOW_LIST_CLASSES
                                : gridClassesForColumns(gridColumns)
                        }
                        onReview={onReview}
                        onRetry={onRetry}
                        onContinue={onContinue}
                        pinStates={pinStates}
                        deleteStates={deleteStates}
                        onPinToggle={onPinToggle}
                        onPinRetry={onPinRetry}
                        onDeleteConfirm={onDeleteConfirm}
                        onDeleteRetry={onDeleteRetry}
                        draggingCardId={draggingCardId}
                        onPinnedDragStart={onPinnedDragStart}
                        onPinnedDragEnd={onPinnedDragEnd}
                        onPinnedDrop={onPinnedDrop}
                    />
                    {state.kind === "load-error" ? (
                        <Alert variant="destructive" className="mt-6">
                            <AlertTitle>{t("flow.load_more_error")}</AlertTitle>
                            <AlertDescription role="alert">
                                {state.message}
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    {state.kind === "loading-more" ? (
                        <div className="mt-6">
                            <LoadingSkeletons
                                count={3}
                                layoutClasses={
                                    layout === "list"
                                        ? FLOW_LIST_CLASSES
                                        : gridClassesForColumns(gridColumns)
                                }
                            />
                        </div>
                    ) : state.kind === "ready" &&
                      state.cursor.kind === "more" ? (
                        <div className="mt-6 flex justify-center">
                            <Button
                                variant="outline"
                                onClick={loadMore}
                                data-testid="flow-load-more"
                            >
                                {t("flow.load_more")}
                            </Button>
                        </div>
                    ) : null}
                    {state.kind === "load-error" ? (
                        <div className="mt-4 flex justify-center">
                            <Button
                                variant="outline"
                                onClick={() =>
                                    loadMoreFrom(state.slots, state.pageToken)
                                }
                                data-testid="flow-load-more-retry"
                            >
                                {t("flow.retry_load_more")}
                            </Button>
                        </div>
                    ) : null}
                </div>
            );
    }
}
