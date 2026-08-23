import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
    FlowCardDetail,
    FlowCardDetailTrigger,
} from "@/components/flow/FlowCardDetail";
import { Dialog } from "@/components/ui/dialog";
import { ImageLightbox } from "@/components/content/ImageLightbox";
import { MarkdownContent } from "@/components/content/MarkdownContent";
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
import {
    continueDailyReview,
    getTipcard,
    listFlowCards,
    reviewAndAdvance,
    pinTipcard,
    deleteTipcard,
    createTips,
} from "@/lib/api-v1/ops";
import { TransportError, newIdempotencyKey } from "@/lib/api-v1/transport";
import { FlowAddForm } from "@/components/flow/FlowAddForm";
import { buildTipsRequest, clearAddPrefill } from "@/lib/flow-add-form";
import type { AddTipsPayload } from "@/lib/flow-add-form";
import {
    addFailed,
    addMutationSucceeded,
    addRetryDecision,
    canStartAdd,
    resolutionRetryDecision,
    resolveFailed,
    resolveSettled,
    startAdd,
    startMutationRetry,
    startResolutionRetry,
    type AddAttempt,
    type AddLifecycle,
    type AddResolutionRun,
} from "@/lib/flow-add-state";
import {
    integrateCreatedCards,
    mergeReconciledCards,
} from "@/lib/flow-add-integration";
import {
    appendIdleSlots,
    classifyReviewError,
    reviewActionsFor,
    slotsFromCards,
    type ReviewChoice,
} from "@/lib/flow-review-actions";
import type { FlowCursor } from "@/lib/flow-state";
import {
    continueFailure,
    continueRetryDecision,
    continueSuccess,
    refillPollFound,
    refillPollMiss,
    retryDecision,
    flowSlotKey,
    reviewFailure,
    reviewSuccess,
    startContinue,
    startReview,
    type ContinueAttempt,
    type ReviewAttempt,
    type ReviewSlot,
} from "@/lib/flow-review-state";
import {
    applyPinFailure,
    applyPinSuccess,
    EMPTY_PIN_STATE,
    pinCardState,
    pinRetryDecision,
    startPin,
    type PinAttempt,
    type PinState,
} from "@/lib/flow-pin-state";
import {
    applyDeleteFailure,
    applyDeleteSuccess,
    deleteCardState,
    deleteRetryDecision,
    EMPTY_DELETE_STATE,
    startDelete,
    type DeleteCardState,
    type DeleteAttempt,
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
import {
    splitTopicPicks,
    TRANSMISSION_MAX_PICKS,
} from "@/lib/flow-transmission";

const PAGE_SIZE = 48;
/** Delay before each bounded refill poll after an awaiting-refill slot. */
const REFILL_POLL_DELAY_MS = 2000;
/** Miss budget per awaiting-refill slot before it becomes completed. */
const REFILL_MAX_ATTEMPTS = 4;
const REVIEW_SWIPE_DELAY_MS = 180;

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

type FlowSlotsState =
    | { kind: "ready"; slots: ReviewSlot[]; cursor: FlowCursor }
    | { kind: "loading-more"; slots: ReviewSlot[]; pageToken: string }
    | {
          kind: "load-error";
          slots: ReviewSlot[];
          pageToken: string;
          message: string;
      };

/**
 * Discriminated Flow state with only legal combinations: `loading-more` and
 * `load-error` always carry the non-empty cursor token they are fetching or
 * retrying; pagination is a `FlowCursor` union (`end` | `more` + required
 * token), so "hasMore without a token" is unrepresentable. Cards ride inside
 * ready/loading-more/load-error as `ReviewSlot`s, so a "load more" never
 * replaces already rendered cards — and a reviewed card's replacement,
 * completed, or awaiting-refill placeholder stays at its exact list position.
 */
export type FlowState =
    | { kind: "initial-loading" }
    | FlowSlotsState
    | { kind: "empty" }
    | { kind: "error"; message: string };

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
                className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 rounded-none border-b px-4 py-3"
                data-testid={`card-title-bar-${card.id}`}
            >
                <div className="flex items-center justify-self-start gap-2">
                    {leading}
                    <TopicIcon
                        aria-hidden
                        icon={lookupTopicIconId(card.topicIcon)}
                        className="size-7 shrink-0"
                        color={card.topicColor || undefined}
                        data-testid={`topic-icon-${card.id}`}
                    />
                </div>
                <div className="flex min-w-0 items-center justify-center gap-1.5 px-1 text-center text-lg font-bold tracking-[0.02em] capitalize">
                    {card.pinned ? (
                        <PinIcon
                            aria-hidden
                            data-testid={`card-pinned-${card.id}`}
                            className="size-4 shrink-0 text-primary"
                        />
                    ) : null}
                    <span className="line-clamp-2 break-words">
                        {card.topicName}
                    </span>
                </div>
                <div className="flex items-center justify-self-end gap-2">
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
            <CardContent className="flex flex-1 flex-col gap-4 px-4 py-4">
                <span className="sr-only">{card.title}</span>
                {view.imageUrls.length === 0 ? null : (
                    <div
                        className={cn(
                            "grid gap-2",
                            view.imageUrls.length > 1 && "grid-cols-2",
                        )}
                    >
                        {view.imageUrls.map((url, index) => (
                            <Button
                                key={url}
                                type="button"
                                variant="ghost"
                                aria-label={tf("images.open_for_card", {
                                    index: index + 1,
                                    title: card.title,
                                })}
                                onClick={() => setLightboxIndex(index)}
                                className="aspect-video h-auto w-full max-h-[220px] overflow-hidden rounded-md bg-muted p-0"
                            >
                                <img
                                    src={url}
                                    alt={tf("images.illustration_for_card", {
                                        title: card.title,
                                    })}
                                    loading="lazy"
                                    className="size-full rounded-md border border-border object-cover"
                                />
                            </Button>
                        ))}
                    </div>
                )}
                {contentKind === "normal" ? (
                <div
                    ref={bodyRef}
                    data-testid={`card-body-${card.id}`}
                    className={cn(
                        "min-w-0 flex-1 text-base leading-7",
                        !expanded && hasCompact && "max-h-72",
                        !expanded &&
                            hasCompact &&
                            (compactScrollable
                                ? "overflow-y-auto"
                                : "overflow-hidden"),
                    )}
                >
                    <MarkdownContent content={content} />
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
                <AlertDescription role="status">
                    {tf("flow.continuing_description", {
                        topic: slot.topicName,
                    })}
                </AlertDescription>
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
            <span
                className="min-w-0 flex-1 text-sm font-medium text-muted-foreground"
                role="status"
            >
                {tf("flow.continuing_description", { topic: slot.topicName })}
            </span>
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

function ReviewSlotCard({
    slot,
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
    flowActive = true,
}: {
    slot: ReviewSlot;
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
    const cardPin = pinCardState(pinStates, id);
    const pinBusy = cardPin.kind !== "idle";
    const cardDelete = deleteCardState(deleteStates, id);
    const deleteBusy = cardDelete.kind === "deleting";
    const controlsBusy = pinBusy || deleteBusy;
    const dragEnabled = live && enableDrag && slot.card.pinned;
    const stackLayers = live ? repeatableStackLayers(slot.card) : 0;
    const reviewSwipe = live ? repeatableReviewSwipe(slot) : undefined;
    const liveControls = live ? (
        <LiveCardControls
            slot={slot}
            pinState={cardPin}
            deleteState={cardDelete}
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
                        "relative isolate h-full",
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
                            "relative z-10 flex h-full min-h-60 flex-col gap-0 overflow-hidden rounded-md py-0 ring-border",
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
                        <CardContent
                            className="mt-auto flex items-center gap-2 border-t px-4 py-4"
                            data-testid={`card-actions-${id}`}
                        >
                            {liveControls}
                        </CardContent>
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
}

function SlotList({
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
                        className={
                            draggingCardId === id ? "opacity-50" : undefined
                        }
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
                            pinStates={pinStates}
                            deleteStates={deleteStates}
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
}

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
                        <span id="flow-count">{picks.length}</span>/
                        {TRANSMISSION_MAX_PICKS} {t("flow.picks_count_suffix")}
                    </div>
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
    const [state, setState] = useState<FlowState>({ kind: "initial-loading" });
    // Ref mirror of the latest state so event handlers read current data
    // without launching work inside a setState updater (updaters must stay
    // pure — React may invoke them more than once).
    const stateRef = useRef<FlowState>({ kind: "initial-loading" });
    // Generation counter: async results from an earlier load/retry/unmount are
    // stale and must not touch state.
    const generationRef = useRef(0);
    // Explicit in-flight ownership: double clicks cannot start a second request.
    const inFlightRef = useRef(false);
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

    const [pinStates, setPinStates] = useState<PinState>(EMPTY_PIN_STATE);
    const pinStatesRef = useRef<PinState>(EMPTY_PIN_STATE);
    const applyPins = useCallback((next: PinState) => {
        pinStatesRef.current = next;
        setPinStates(next);
    }, []);
    const [deleteStates, setDeleteStates] =
        useState<DeleteState>(EMPTY_DELETE_STATE);
    const deleteStatesRef = useRef<DeleteState>(EMPTY_DELETE_STATE);
    const applyDeletes = useCallback((next: DeleteState) => {
        deleteStatesRef.current = next;
        setDeleteStates(next);
    }, []);

    const loadInitial = useCallback(async () => {
        if (inFlightRef.current) return;
        inFlightRef.current = true;
        const generation = ++generationRef.current;
        apply({ kind: "initial-loading" });
        try {
            const page = await listFlowCards({ pageSize: PAGE_SIZE });
            if (!mountedRef.current || generationRef.current !== generation)
                return;
            if (page.cards.length === 0) {
                apply({ kind: "empty" });
            } else {
                apply({
                    kind: "ready",
                    slots: slotsFromCards(page.cards),
                    cursor: page.cursor,
                });
            }
        } catch (error) {
            if (!mountedRef.current || generationRef.current !== generation)
                return;
            apply({
                kind: "error",
                message: error instanceof Error ? error.message : String(error),
            });
        } finally {
            if (generationRef.current === generation)
                inFlightRef.current = false;
        }
    }, [apply]);

    const loadMoreFrom = useCallback(
        (slots: ReviewSlot[], pageToken: string) => {
            if (inFlightRef.current) return;
            inFlightRef.current = true;
            const generation = ++generationRef.current;
            apply({ kind: "loading-more", slots, pageToken });
            void (async () => {
                try {
                    const page = await listFlowCards({
                        pageSize: PAGE_SIZE,
                        pageToken,
                    });
                    if (
                        !mountedRef.current ||
                        generationRef.current !== generation
                    )
                        return;
                    const current = stateRef.current;
                    if (
                        current.kind !== "loading-more" ||
                        current.pageToken !== pageToken
                    )
                        return;
                    apply({
                        kind: "ready",
                        slots: appendIdleSlots(current.slots, page.cards),
                        cursor: page.cursor,
                    });
                } catch (error) {
                    // Recoverable: keep the rendered slots and the same cursor so a
                    // retry re-requests exactly the failed page. The notice persists
                    // until a successful retry or an explicit whole-flow reload.
                    if (
                        !mountedRef.current ||
                        generationRef.current !== generation
                    )
                        return;
                    const current = stateRef.current;
                    if (
                        current.kind !== "loading-more" ||
                        current.pageToken !== pageToken
                    )
                        return;
                    apply({
                        kind: "load-error",
                        slots: current.slots,
                        pageToken,
                        message:
                            error instanceof Error
                                ? error.message
                                : String(error),
                    });
                } finally {
                    if (generationRef.current === generation)
                        inFlightRef.current = false;
                }
            })();
        },
        [apply],
    );

    const loadMore = useCallback(() => {
        // Reads current data from the ref; no request launches inside any updater.
        const current = stateRef.current;
        if (current.kind !== "ready" || current.cursor.kind !== "more") return;
        loadMoreFrom(current.slots, current.cursor.pageToken);
    }, [loadMoreFrom]);

    /**
     * Launch exactly one card's mutation outside any state updater/ref
     * callback. `attempt` and the post-`startReview` slot list are provided by
     * the caller; the per-slot generation captured here makes stale results —
     * including after unmount or a newer retry — unable to commit.
     */
    const launchReview = useCallback(
        (started: ReviewSlot[], cardId: bigint, attempt: ReviewAttempt) => {
            const target = started.find(
                (candidate) =>
                    candidate.kind === "reviewing" &&
                    candidate.card.id === cardId,
            );
            if (target?.kind !== "reviewing") return;
            const generation = target.generation;
            void (async () => {
                try {
                    if (target.card.tipcardType === "repeatable_tip") {
                        await new Promise((resolve) =>
                            setTimeout(resolve, REVIEW_SWIPE_DELAY_MS),
                        );
                        if (!mountedRef.current) return;
                    }
                    const outcome = await reviewAndAdvance({
                        cardId,
                        grade: attempt.grade,
                        action: attempt.action,
                        idempotencyKey: attempt.idempotencyKey,
                    });
                    if (!mountedRef.current) return;
                    const current = stateRef.current;
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const nextSlots = reviewSuccess(
                        current.slots,
                        outcome.reviewedCardId,
                        generation,
                        outcome,
                    );
                    if (nextSlots === current.slots) return; // stale generation
                    apply({ ...current, slots: nextSlots });
                } catch (error) {
                    if (!mountedRef.current) return;
                    const current = stateRef.current;
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const nextSlots = reviewFailure(
                        current.slots,
                        cardId,
                        generation,
                        classifyReviewError(error),
                    );
                    if (nextSlots === current.slots) return; // stale generation
                    apply({ ...current, slots: nextSlots });
                }
            })();
        },
        [apply],
    );

    const onReview = useCallback(
        (cardId: bigint, choice: ReviewChoice) => {
            if (pinCardState(pinStatesRef.current, cardId).kind !== "idle")
                return;
            const current = stateRef.current;
            if (
                current.kind !== "ready" &&
                current.kind !== "loading-more" &&
                current.kind !== "load-error"
            )
                return;
            const attempt: ReviewAttempt = {
                grade: choice.grade,
                action: choice.action,
                idempotencyKey: newIdempotencyKey(),
            };
            const started = startReview(current.slots, cardId, attempt);
            if (started === current.slots) return;
            apply({ ...current, slots: started });
            launchReview(started, cardId, attempt);
        },
        [apply, launchReview],
    );

    const onRetry = useCallback(
        (cardId: bigint) => {
            if (pinCardState(pinStatesRef.current, cardId).kind !== "idle")
                return;
            const current = stateRef.current;
            if (
                current.kind !== "ready" &&
                current.kind !== "loading-more" &&
                current.kind !== "load-error"
            )
                return;
            const errored = current.slots.find(
                (slot) => slot.kind === "error" && slot.card.id === cardId,
            );
            if (errored?.kind !== "error") return;
            const decision = retryDecision(errored);
            const attempt: ReviewAttempt =
                decision.kind === "reuseAttempt"
                    ? decision.attempt
                    : {
                          grade: decision.grade,
                          action: decision.action,
                          idempotencyKey: newIdempotencyKey(),
                      };
            const started = startReview(current.slots, cardId, attempt);
            if (started === current.slots) return;
            apply({ ...current, slots: started });
            launchReview(started, cardId, attempt);
        },
        [apply, launchReview],
    );

    /**
     * Launch exactly one Continue mutation outside any state updater. The
     * per-slot generation captured from the `continuing` slot makes stale
     * results — including after unmount or a newer retry — unable to commit.
     */
    const launchContinue = useCallback(
        (
            started: ReviewSlot[],
            reviewedCardId: bigint,
            attempt: ContinueAttempt,
        ) => {
            const target = started.find(
                (candidate) =>
                    candidate.kind === "continuing" &&
                    candidate.reviewedCardId === reviewedCardId,
            );
            if (target?.kind !== "continuing") return;
            const generation = target.generation;
            void (async () => {
                let mutationReturned = false;
                try {
                    const outcome = await continueDailyReview({
                        topicName: attempt.topicName,
                        idempotencyKey: attempt.idempotencyKey,
                    });
                    mutationReturned = true;
                    const detail = await getTipcard({
                        cardId: outcome.activeCardId,
                    });
                    if (!mountedRef.current) return;
                    const current = stateRef.current;
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const nextSlots = continueSuccess(
                        current.slots,
                        reviewedCardId,
                        generation,
                        detail.card,
                        outcome.pendingCount,
                    );
                    if (nextSlots === current.slots) return; // stale generation
                    apply({ ...current, slots: nextSlots });
                } catch (error) {
                    if (!mountedRef.current) return;
                    const current = stateRef.current;
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const classified = classifyReviewError(error);
                    // Once the mutation itself returned successfully, a detail-read
                    // failure cannot prove the mutation did not commit: force the
                    // indeterminate verdict so Retry reuses the exact same key and
                    // obtains the idempotent result before reading detail again.
                    const failure = mutationReturned
                        ? { ...classified, mutationOutcomeIndeterminate: true }
                        : classified;
                    const nextSlots = continueFailure(
                        current.slots,
                        reviewedCardId,
                        generation,
                        failure,
                    );
                    if (nextSlots === current.slots) return; // stale generation
                    apply({ ...current, slots: nextSlots });
                }
            })();
        },
        [apply],
    );

    /**
     * Start (or retry) Continue for one completed/errored slot. The second
     * click cannot launch another mutation: after the first, the slot is no
     * longer `completed`/`continueError`, so `startContinue` is a no-op.
     */
    const onContinue = useCallback(
        (reviewedCardId: bigint) => {
            const current = stateRef.current;
            if (
                current.kind !== "ready" &&
                current.kind !== "loading-more" &&
                current.kind !== "load-error"
            )
                return;
            const slot = current.slots.find(
                (candidate) =>
                    (candidate.kind === "completed" ||
                        candidate.kind === "continueError") &&
                    candidate.reviewedCardId === reviewedCardId,
            );
            if (slot?.kind !== "completed" && slot?.kind !== "continueError")
                return;
            const attempt: ContinueAttempt =
                slot.kind === "continueError"
                    ? (() => {
                          const decision = continueRetryDecision(slot);
                          return decision.kind === "reuseAttempt"
                              ? decision.attempt
                              : {
                                    topicName: decision.topicName,
                                    idempotencyKey: newIdempotencyKey(),
                                };
                      })()
                    : {
                          topicName: slot.topicName,
                          idempotencyKey: newIdempotencyKey(),
                      };
            const started = startContinue(
                current.slots,
                reviewedCardId,
                attempt,
            );
            if (started === current.slots) return;
            apply({ ...current, slots: started });
            launchContinue(started, reviewedCardId, attempt);
        },
        [apply, launchContinue],
    );

    /**
     * Launch exactly one pin mutation outside any state updater. The captured
     * attempt makes stale results — a newer retry, unmount, or an unknown
     * attempt — unable to commit; the pure model owns the exact commit rules.
     */
    const launchPin = useCallback(
        (attempt: PinAttempt) => {
            void (async () => {
                try {
                    await pinTipcard({
                        cardId: attempt.cardId,
                        pinned: attempt.targetPinned,
                        idempotencyKey: attempt.idempotencyKey,
                    });
                    if (!mountedRef.current) return;
                    const current = stateRef.current;
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const committed = applyPinSuccess(
                        current.slots,
                        pinStatesRef.current,
                        attempt,
                    );
                    if (committed.state === pinStatesRef.current) return; // stale
                    apply({ ...current, slots: committed.slots });
                    applyPins(committed.state);
                } catch (error) {
                    if (!mountedRef.current) return;
                    const current = stateRef.current;
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const committed = applyPinFailure(
                        current.slots,
                        pinStatesRef.current,
                        attempt,
                        classifyReviewError(error),
                    );
                    if (committed.state === pinStatesRef.current) return; // stale
                    apply({ ...current, slots: committed.slots });
                    applyPins(committed.state);
                }
            })();
        },
        [apply, applyPins],
    );

    const beginPin = useCallback(
        (attempt: PinAttempt) => {
            const current = stateRef.current;
            if (
                current.kind !== "ready" &&
                current.kind !== "loading-more" &&
                current.kind !== "load-error"
            )
                return;
            const started = startPin(
                current.slots,
                pinStatesRef.current,
                attempt,
            );
            if (started === undefined) return;
            applyPins(started);
            launchPin(attempt);
        },
        [applyPins, launchPin],
    );

    const onPinToggle = useCallback(
        (cardId: bigint, targetPinned: boolean) => {
            const previous = pinCardState(pinStatesRef.current, cardId);
            const attempt: PinAttempt = {
                cardId,
                targetPinned,
                idempotencyKey: newIdempotencyKey(),
                generation:
                    previous.kind === "idle"
                        ? 1
                        : previous.attempt.generation + 1,
            };
            beginPin(attempt);
        },
        [beginPin],
    );

    const onPinRetry = useCallback(
        (cardId: bigint) => {
            const errored = pinCardState(pinStatesRef.current, cardId);
            if (errored.kind !== "error") return;
            const decision = pinRetryDecision(errored);
            const attempt: PinAttempt =
                decision.kind === "reuseAttempt"
                    ? decision.attempt
                    : {
                          cardId: decision.cardId,
                          targetPinned: decision.targetPinned,
                          idempotencyKey: newIdempotencyKey(),
                          generation: decision.generation,
                      };
            beginPin(attempt);
        },
        [beginPin],
    );

    /**
     * Launch one delete mutation for the captured attempt. A response commits
     * only while the same per-card generation is still current. Success removes
     * that exact slot in place and updates the shared pinned-order key; it
     * never refetches the Flow.
     */
    const launchDelete = useCallback(
        (attempt: DeleteAttempt) => {
            void (async () => {
                try {
                    await deleteTipcard({
                        cardId: attempt.cardId,
                        idempotencyKey: attempt.idempotencyKey,
                    });
                    if (!mountedRef.current) return;
                    const current = stateRef.current;
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const committed = applyDeleteSuccess(
                        current.slots,
                        deleteStatesRef.current,
                        attempt,
                        pinnedOrderRef.current,
                    );
                    if (committed.state === deleteStatesRef.current) return;
                    applyDeletes(committed.state);
                    applyPinnedOrder(committed.pinnedOrder);
                    if (committed.slots.length === 0) {
                        apply({ kind: "empty" });
                    } else {
                        apply({ ...current, slots: committed.slots });
                    }
                } catch (error) {
                    if (!mountedRef.current) return;
                    const current = stateRef.current;
                    if (
                        current.kind !== "ready" &&
                        current.kind !== "loading-more" &&
                        current.kind !== "load-error"
                    )
                        return;
                    const committed = applyDeleteFailure(
                        current.slots,
                        deleteStatesRef.current,
                        attempt,
                        pinnedOrderRef.current,
                        classifyReviewError(error),
                    );
                    if (committed.state === deleteStatesRef.current) return;
                    applyDeletes(committed.state);
                }
            })();
        },
        [apply, applyDeletes, applyPinnedOrder],
    );

    const beginDelete = useCallback(
        (attempt: DeleteAttempt) => {
            const current = stateRef.current;
            if (
                current.kind !== "ready" &&
                current.kind !== "loading-more" &&
                current.kind !== "load-error"
            )
                return;
            const started = startDelete(
                current.slots,
                deleteStatesRef.current,
                attempt,
            );
            if (started === undefined) return;
            applyDeletes(started);
            launchDelete(attempt);
        },
        [applyDeletes, launchDelete],
    );

    const onDeleteConfirm = useCallback(
        (cardId: bigint) => {
            const previous = deleteCardState(deleteStatesRef.current, cardId);
            const attempt: DeleteAttempt = {
                cardId,
                idempotencyKey: newIdempotencyKey(),
                generation:
                    previous.kind === "idle"
                        ? 1
                        : previous.attempt.generation + 1,
            };
            beginDelete(attempt);
        },
        [beginDelete],
    );

    const onDeleteRetry = useCallback(
        (cardId: bigint) => {
            const errored = deleteCardState(deleteStatesRef.current, cardId);
            if (errored.kind !== "error") return;
            const decision = deleteRetryDecision(errored);
            const attempt: DeleteAttempt =
                decision.kind === "reuseAttempt"
                    ? decision.attempt
                    : {
                          cardId: decision.cardId,
                          idempotencyKey: newIdempotencyKey(),
                          generation: decision.generation,
                      };
            beginDelete(attempt);
        },
        [beginDelete],
    );

    // ---------------------------------------------------------------------------
    // Add-card form lifecycle.
    // ---------------------------------------------------------------------------
    const [addState, setAddState] = useState<AddLifecycle>({ kind: "idle" });
    const addStateRef = useRef<AddLifecycle>({ kind: "idle" });
    const addGenerationRef = useRef(0);
    // Double-submit guard: a click while a launch is pending cannot start twice.
    const addInFlightRef = useRef(false);
    const [addedNotice, setAddedNotice] = useState(false);
    const applyAdd = useCallback((next: AddLifecycle) => {
        addStateRef.current = next;
        setAddState(next);
    }, []);

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

    /**
     * Atomically integrate one resolved batch into the latest slots and saved
     * pinned order. Returns whether an authoritative quiet list read is still
     * required; stale resolution runs cannot alter Flow state.
     */
    const commitIntegratedCards = useCallback(
        (run: AddResolutionRun, cards: FlowCardInfo[]): boolean => {
            const add = addStateRef.current;
            if (add.kind !== "resolving" || add.run !== run) return false;
            const current = stateRef.current;
            const slots =
                current.kind === "empty"
                    ? []
                    : current.kind === "ready" ||
                        current.kind === "loading-more" ||
                        current.kind === "load-error"
                      ? current.slots
                      : undefined;
            if (slots === undefined) return false;
            const integrated = integrateCreatedCards({
                slots,
                cards,
                pinnedOrder: pinnedOrderRef.current,
                busyCardIds: busyCardIds(),
            });
            applyPinnedOrder(integrated.pinnedOrder);
            if (current.kind === "empty") {
                if (integrated.slots.length > 0) {
                    apply({
                        kind: "ready",
                        slots: integrated.slots,
                        cursor: { kind: "end" },
                    });
                }
            } else if (
                current.kind === "ready" ||
                current.kind === "loading-more" ||
                current.kind === "load-error"
            ) {
                apply({ ...current, slots: integrated.slots });
            }
            return integrated.needsReconciliation;
        },
        [apply, applyPinnedOrder, busyCardIds],
    );

    /**
     * Resolution phase: after `tips_v1` succeeded, resolve every returned
     * positive ID with `get_tipcard`, integrate the details, then — for
     * repeatable creation, an empty created-ID list, or any detail failure —
     * run one authoritative quiet list reconciliation. Never resubmits the
     * mutation; stale/unmounted results cannot commit.
     */
    const launchResolution = useCallback(
        (run: AddResolutionRun) => {
            void (async () => {
                try {
                    const outcomes = await Promise.allSettled(
                        run.createdIds.map(
                            async (id) =>
                                (await getTipcard({ cardId: id })).card,
                        ),
                    );
                    if (
                        !mountedRef.current ||
                        addStateRef.current.kind !== "resolving" ||
                        addStateRef.current.run !== run
                    )
                        return;
                    const details = outcomes.flatMap((outcome) =>
                        outcome.status === "fulfilled" ? [outcome.value] : [],
                    );
                    const detailFailed = outcomes.some(
                        (outcome) => outcome.status === "rejected",
                    );
                    const integrationNeedsReconcile = commitIntegratedCards(
                        run,
                        details,
                    );
                    const needsReconcile =
                        run.attempt.payload.kind === "repeatable" ||
                        run.createdIds.length === 0 ||
                        detailFailed ||
                        integrationNeedsReconcile;
                    if (!needsReconcile) {
                        const next = resolveSettled(addStateRef.current, run);
                        if (next !== addStateRef.current) {
                            applyAdd(next);
                            setAddedNotice(true);
                        }
                        return;
                    }
                    try {
                        const page = await listFlowCards({
                            pageSize: PAGE_SIZE,
                        });
                        if (
                            !mountedRef.current ||
                            addStateRef.current.kind !== "resolving" ||
                            addStateRef.current.run !== run
                        )
                            return;
                        const current = stateRef.current;
                        if (
                            current.kind === "ready" ||
                            current.kind === "loading-more" ||
                            current.kind === "load-error"
                        ) {
                            apply({
                                ...current,
                                slots: mergeReconciledCards(
                                    current.slots,
                                    page.cards,
                                ),
                            });
                        } else if (
                            current.kind === "empty" &&
                            page.cards.length > 0
                        ) {
                            apply({
                                kind: "ready",
                                slots: mergeReconciledCards([], page.cards),
                                cursor: page.cursor,
                            });
                        }
                        const next = resolveSettled(addStateRef.current, run);
                        if (next !== addStateRef.current) {
                            applyAdd(next);
                            setAddedNotice(true);
                        }
                    } catch (error) {
                        if (!mountedRef.current) return;
                        applyAdd(
                            resolveFailed(
                                addStateRef.current,
                                run,
                                error instanceof Error
                                    ? error.message
                                    : String(error),
                            ),
                        );
                    }
                } catch (error) {
                    if (!mountedRef.current) return;
                    applyAdd(
                        resolveFailed(
                            addStateRef.current,
                            run,
                            error instanceof Error
                                ? error.message
                                : String(error),
                        ),
                    );
                } finally {
                    // Resolution owns the guard: it starts only after the mutation
                    // settled and covers both the submit and resolve-retry paths.
                    addInFlightRef.current = false;
                }
            })();
        },
        [apply, applyAdd, commitIntegratedCards],
    );

    /** Launch exactly one `tips_v1` mutation for the captured attempt. */
    const launchAddMutation = useCallback(
        (attempt: AddAttempt) => {
            void (async () => {
                try {
                    const outcome = await createTips({
                        request: buildTipsRequest(attempt.payload),
                        idempotencyKey: attempt.payload.idempotencyKey,
                    });
                    if (!mountedRef.current) return;
                    const createdIds = outcome.tips.map((tip) => tip.id);
                    const run: AddResolutionRun = {
                        attempt,
                        createdIds,
                        resolutionGeneration: 1,
                    };
                    const next = addMutationSucceeded(
                        addStateRef.current,
                        attempt,
                        run,
                    );
                    if (next === addStateRef.current) return;
                    clearAddPrefill();
                    applyAdd(next);
                    launchResolution(run);
                } catch (error) {
                    if (!mountedRef.current) return;
                    const indeterminate =
                        error instanceof TransportError
                            ? error.mutationOutcomeIndeterminate
                            : true;
                    applyAdd(
                        addFailed(addStateRef.current, attempt, {
                            mutationOutcomeIndeterminate: indeterminate,
                            message:
                                error instanceof Error
                                    ? error.message
                                    : String(error),
                        }),
                    );
                    addInFlightRef.current = false;
                }
            })();
        },
        [applyAdd, launchResolution],
    );

    const onAddSubmit = useCallback(
        (payload: AddTipsPayload) => {
            if (addInFlightRef.current || !canStartAdd(addStateRef.current))
                return;
            addInFlightRef.current = true;
            addGenerationRef.current += 1;
            setAddedNotice(false);
            const attempt: AddAttempt = {
                payload,
                submissionGeneration: addGenerationRef.current,
            };
            applyAdd(startAdd(addStateRef.current, attempt));
            launchAddMutation(attempt);
        },
        [applyAdd, launchAddMutation],
    );

    /**
     * Mutation retry. An outcome-indeterminate failure reuses the exact
     * captured payload and key; a determinate failure preserves the semantic
     * payload with a fresh key/generation.
     */
    const onAddRetryMutation = useCallback(() => {
        if (addInFlightRef.current) return;
        const errored = addStateRef.current;
        if (errored.kind !== "mutationError") return;
        const decision = addRetryDecision(errored);
        const attempt: AddAttempt =
            decision.kind === "reuseAttempt"
                ? decision.attempt
                : {
                      payload: {
                          ...decision.payload,
                          idempotencyKey: newIdempotencyKey(),
                      },
                      submissionGeneration: decision.submissionGeneration,
                  };
        const next = startMutationRetry(errored, attempt);
        if (next === errored) return;
        addInFlightRef.current = true;
        addGenerationRef.current = Math.max(
            addGenerationRef.current,
            attempt.submissionGeneration,
        );
        applyAdd(next);
        launchAddMutation(attempt);
    }, [applyAdd, launchAddMutation]);

    /**
     * Post-mutation retry: retries only detail resolution/reconciliation —
     * never the already-successful mutation.
     */
    const onAddRetryResolve = useCallback(() => {
        const errored = addStateRef.current;
        if (errored.kind !== "resolutionError" || addInFlightRef.current)
            return;
        const decision = resolutionRetryDecision(
            errored,
            errored.run.resolutionGeneration + 1,
        );
        if (decision === undefined) return;
        const next = startResolutionRetry(errored, decision);
        if (next === errored) return;
        addInFlightRef.current = true;
        applyAdd(next);
        launchResolution(decision.run);
    }, [applyAdd, launchResolution]);

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
            generationRef.current += 1;
        };
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
                <div data-testid="flow-ready">
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
