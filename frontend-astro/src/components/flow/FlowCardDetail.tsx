import {
    useCallback,
    useEffect,
    useRef,
    useState,
    lazy,
    Suspense,
} from "react";
import {
    CircleHelpIcon,
    ExternalLinkIcon,
    FileTextIcon,
    LinkIcon,
    Maximize2Icon,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ImageLightbox } from "@/components/content/ImageLightbox";

/** Lazy markdown shared with the Flow grid; the chunk warms during idle. */
const LazyMarkdownContent = lazy(() =>
    import("@/components/content/MarkdownContent").then((m) => ({
        default: m.MarkdownContent,
    })),
);
import { CardImageManager } from "@/components/flow/CardImageManager";
import {
    Card,
    CardContent,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card";
import {
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemMedia,
    ItemTitle,
} from "@/components/ui/item";
import {
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import type { FlowCardInfo } from "@/generated/denpie_pb";
import { getTipcard } from "@/lib/api-v1/ops";
import {
    closeCardDetail,
    detailFailed,
    detailSources,
    detailSucceeded,
    humanDetailDate,
    INITIAL_FLOW_DETAIL_STATE,
    loadingDetailRequest,
    openCardDetail,
    retryCardDetail,
    type DetailRequest,
    type DetailSourceIconKind,
    type FlowDetailState,
} from "@/lib/flow-detail-state";
import { cardImageUrls } from "@/lib/flow-view";
import { t, tf } from "@/lib/i18n";

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

function SourceIcon({ kind }: { kind: DetailSourceIconKind }) {
    switch (kind) {
        case "link":
            return <LinkIcon aria-hidden />;
        case "document":
            return <FileTextIcon aria-hidden />;
        case "unknown":
            return <CircleHelpIcon aria-hidden />;
    }
}

function DetailLoading() {
    return (
        <div
            className="flex flex-col gap-4 px-4 pb-6"
            data-testid="card-detail-loading"
            role="status"
        >
            <span className="sr-only">{t("card.detail.loading")}</span>
            <Skeleton className="h-48 w-full rounded-xl" />
            <Skeleton className="h-5 w-3/4" />
            <Skeleton className="h-5 w-full" />
            <Skeleton className="h-24 w-full rounded-xl" />
        </div>
    );
}

function DetailSources({ card }: { card: FlowCardInfo }) {
    const sources = detailSources(card);
    if (sources.length === 0) return null;
    return (
        <Card size="sm" data-testid="card-detail-sources">
            <CardHeader>
                <CardTitle>{t("card.sources.title")}</CardTitle>
                <CardDescription>
                    {t("card.sources.description")}
                </CardDescription>
            </CardHeader>
            <CardContent>
                <ItemGroup>
                    {sources.map((source, index) => {
                        const description =
                            source.href !== null
                                ? t("card.sources.external_link")
                                : source.icon === "document"
                                  ? t("card.sources.stored_document")
                                  : source.icon === "link"
                                    ? t("card.sources.link_unavailable")
                                    : t("card.sources.source");
                        const content = (
                            <>
                                <ItemMedia variant="icon">
                                    <SourceIcon kind={source.icon} />
                                </ItemMedia>
                                <ItemContent>
                                    <ItemTitle>{source.label}</ItemTitle>
                                    <ItemDescription>
                                        {description}
                                    </ItemDescription>
                                </ItemContent>
                                {source.href === null ? null : (
                                    <ItemActions>
                                        <ExternalLinkIcon aria-hidden />
                                    </ItemActions>
                                )}
                            </>
                        );
                        return source.href === null ? (
                            <Item
                                key={`${source.label}:${source.icon}:${index}`}
                                variant="outline"
                            >
                                {content}
                            </Item>
                        ) : (
                            <Item
                                key={`${source.href}:${source.label}:${index}`}
                                variant="outline"
                                render={
                                    <a
                                        href={source.href}
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
            </CardContent>
        </Card>
    );
}

function DetailReady({
    card,
    onCardChanged,
    actions,
}: {
    card: FlowCardInfo;
    onCardChanged: (card: FlowCardInfo) => void;
    actions?: React.ReactNode;
}) {
    const [lightboxIndex, setLightboxIndex] = useState<number | null>(null);
    const created = humanDetailDate(card.createdAt);
    const content =
        card.fullContent.trim() === ""
            ? card.compressedContent
            : card.fullContent;
    return (
        <div
            className="flex flex-col gap-4 px-4 pb-6"
            data-testid="card-detail-content"
        >
            <Card>
                <CardHeader>
                    <div className="flex flex-wrap gap-2">
                        <Badge variant="outline">{card.topicName}</Badge>
                        {card.pinned ? <Badge>{t("card.pinned")}</Badge> : null}
                        {created === null ? null : (
                            <Badge variant="secondary">
                                {tf("card.created", { date: created })}
                            </Badge>
                        )}
                    </div>
                    <CardTitle>{card.title}</CardTitle>
                    <CardDescription>
                        {t("card.detail.description")}
                    </CardDescription>
                </CardHeader>
                <CardContent className="flex flex-col gap-4">
                    {cardImageUrls(card).map((url, index) => (
                        <Button
                            key={url}
                            type="button"
                            variant="ghost"
                            aria-label={tf("images.open_for_card", {
                                index: index + 1,
                                title: card.title,
                            })}
                            onClick={() => setLightboxIndex(index)}
                            className="h-auto w-full p-0"
                        >
                            <img
                                src={url}
                                alt={tf("images.illustration_for_card", {
                                    title: card.title,
                                })}
                                loading="lazy"
                                className="max-h-96 w-full rounded-md border border-border object-contain"
                            />
                        </Button>
                    ))}
                    <Suspense
                        fallback={
                            <div className="space-y-2 animate-pulse" aria-hidden="true">
                                <div className="h-4 rounded bg-muted" />
                                <div className="h-4 w-5/6 rounded bg-muted" />
                                <div className="h-4 w-2/3 rounded bg-muted" />
                            </div>
                        }
                    >
                        <LazyMarkdownContent content={content} />
                    </Suspense>
                </CardContent>
                <CardFooter className="flex flex-wrap gap-2 text-xs text-muted-foreground">
                    <span>{cardTypeLabel(card.tipcardType)}</span>
                    <span aria-hidden="true">·</span>
                    <span>{cardStatusLabel(card.status)}</span>
                    <span aria-hidden="true">·</span>
                    <span>
                        {tf("card.review_count", { count: card.repeatCount })}
                    </span>
                </CardFooter>
            </Card>
            <DetailSources card={card} />
            <CardImageManager card={card} onChanged={onCardChanged} />
            {actions === undefined ? null : (
                <Card size="sm" className="sticky bottom-4">
                    <CardContent className="flex items-center gap-2">
                        {actions}
                    </CardContent>
                </Card>
            )}
            <ImageLightbox
                open={lightboxIndex !== null}
                images={cardImageUrls(card).map((url) => ({
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
        </div>
    );
}

export function FlowCardDetailTrigger({ cardId }: { cardId: bigint }) {
    return (
        <DialogTrigger
            render={<Button variant="outline" size="icon" />}
            aria-label={tf("card.detail.open_aria", {
                id: cardId.toString(),
            })}
            data-testid={`detail-open-${cardId}`}
        >
            <Maximize2Icon data-icon="inline-start" />
        </DialogTrigger>
    );
}

export function FlowCardDetail({
    card,
    open,
    onOpenChange,
    onCardChanged,
    actions,
}: {
    card: FlowCardInfo;
    open: boolean;
    onOpenChange: (open: boolean) => void;
    onCardChanged?: (card: FlowCardInfo) => void;
    actions?: React.ReactNode;
}) {
    const [state, setState] = useState<FlowDetailState>(
        INITIAL_FLOW_DETAIL_STATE,
    );
    const stateRef = useRef<FlowDetailState>(INITIAL_FLOW_DETAIL_STATE);
    const mountedRef = useRef(true);

    const apply = useCallback((next: FlowDetailState) => {
        stateRef.current = next;
        setState(next);
    }, []);

    useEffect(
        () => () => {
            mountedRef.current = false;
        },
        [],
    );

    const launch = useCallback(
        (request: DetailRequest) => {
            void (async () => {
                try {
                    const result = await getTipcard({ cardId: request.cardId });
                    if (!mountedRef.current) return;
                    apply(
                        detailSucceeded(stateRef.current, request, result.card),
                    );
                } catch (error) {
                    if (!mountedRef.current) return;
                    apply(
                        detailFailed(
                            stateRef.current,
                            request,
                            error instanceof Error
                                ? error.message
                                : String(error),
                        ),
                    );
                }
            })();
        },
        [apply],
    );

    const beginLoad = useCallback(
        (next: FlowDetailState) => {
            apply(next);
            const request = loadingDetailRequest(next);
            if (request !== undefined) launch(request);
        },
        [apply, launch],
    );

    useEffect(() => {
        if (!open) return;
        const current = stateRef.current;
        if (
            (current.kind === "ready" ||
                current.kind === "error" ||
                current.kind === "loading") &&
            current.request.cardId === card.id
        ) {
            return;
        }
        beginLoad(openCardDetail(current, card.id));
    }, [open, card.id, beginLoad]);

    const dismiss = useCallback(() => {
        const current = stateRef.current;
        if (current.kind === "loading") {
            apply(closeCardDetail(current));
        }
        onOpenChange(false);
    }, [apply, onOpenChange]);

    const onRetry = useCallback(() => {
        beginLoad(retryCardDetail(stateRef.current));
    }, [beginLoad]);

    const applyChangedCard = useCallback(
        (changed: FlowCardInfo) => {
            const current = stateRef.current;
            if (current.kind === "ready") {
                apply({ ...current, card: changed });
            }
            onCardChanged?.(changed);
        },
        [apply, onCardChanged],
    );

    return (
        <DialogContent
            className="inset-0 top-0 left-0 block h-[100dvh] max-w-none translate-x-0 translate-y-0 overflow-y-auto rounded-none p-0 sm:max-w-none lg:left-56 lg:w-[calc(100%-14rem)]"
            overlayClassName="lg:left-56"
            container={
                typeof document === "undefined"
                    ? undefined
                    : (document.getElementById("flow-view") ?? undefined)
            }
            data-testid="card-detail-fullscreen"
            data-flow-fullscreen=""
            onClick={(event) => {
                if (event.target === event.currentTarget) {
                    dismiss();
                }
            }}
        >
            <div className="mx-auto flex min-h-full w-full max-w-[850px] flex-col gap-4 px-4 py-5 sm:px-6">
                <DialogHeader className="pr-10">
                    <DialogTitle>
                        {state.kind === "ready"
                            ? state.card.title
                            : card.title}
                    </DialogTitle>
                    <DialogDescription>
                        {tf("card.detail.full_description", {
                            topic: card.topicName,
                        })}
                    </DialogDescription>
                </DialogHeader>
                {state.kind === "loading" ? <DetailLoading /> : null}
                {state.kind === "error" ? (
                    <Alert
                        variant="destructive"
                        data-testid="card-detail-error"
                    >
                        <AlertTitle>
                            {t("card.detail.load_error")}
                        </AlertTitle>
                        <AlertDescription className="flex flex-col items-start gap-3">
                            <span role="alert">{state.message}</span>
                            <Button
                                variant="outline"
                                size="sm"
                                onClick={onRetry}
                                data-testid="card-detail-retry"
                            >
                                {t("common.retry")}
                            </Button>
                        </AlertDescription>
                    </Alert>
                ) : null}
                {state.kind === "ready" ? (
                    <DetailReady
                        card={state.card}
                        onCardChanged={applyChangedCard}
                        actions={actions}
                    />
                ) : null}
            </div>
        </DialogContent>
    );
}
