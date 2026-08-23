import { useCallback, useEffect, useMemo, useState } from "react";
import {
    ArrowLeftIcon,
    ArchiveIcon,
    CalendarIcon,
    EyeIcon,
    PinIcon,
    PinOffIcon,
    RefreshCwIcon,
    SearchIcon,
    Trash2Icon,
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
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
    Card,
    CardContent,
    CardFooter,
    CardHeader,
} from "@/components/ui/card";
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog";
import { ImageLightbox } from "@/components/content/ImageLightbox";
import { MarkdownContent } from "@/components/content/MarkdownContent";
import { Input } from "@/components/ui/input";
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select";
import type { TipcardInfo } from "@/generated/denpie_pb";
import { deleteTipcard, pinTipcard } from "@/lib/api-v1/ops";
import { listTipcards } from "@/lib/api-v1/route-ops";
import { newIdempotencyKey } from "@/lib/api-v1/transport";
import { t, tf } from "@/lib/i18n";
import { useViewRefresh } from "@/islands/use-view-refresh";
import { CardImageManager } from "@/components/flow/CardImageManager";
import { archiveCardHeading, inventoryToFlowCard } from "@/lib/pages/archive";
import { LoadedImage } from "@/components/content/LoadedImage";
import {
    archiveSearch,
    archiveTopics,
    filterArchiveCards,
    parseArchiveSearch,
    type ArchiveFilters,
    type ArchiveSort,
    type ArchiveStatus,
} from "@/lib/pages/archive";

const statuses: ArchiveStatus[] = [
    "all",
    "active",
    "completed",
    "pending",
    "scheduled",
    "custom",
];

function isArchiveStatus(value: string | null): value is ArchiveStatus {
    return value !== null && statuses.some((status) => status === value);
}

function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
}

function imageUrls(card: TipcardInfo): string[] {
    return card.images.map(
        (image) =>
            image.downloadPath ||
            `/api/v1/tipcard-images/${image.id.toString()}`,
    );
}

function displayCardTitle(card: TipcardInfo): string {
    const heading = archiveCardHeading(card);
    return heading === "" ? t("archive.untitled_card") : heading;
}

function humanDate(value: string): string {
    if (value.trim() === "") return t("format.unknown_date");
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime())
        ? value
        : new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(
              parsed,
          );
}

function archiveStatusText(status: ArchiveStatus): string {
    switch (status) {
        case "all":
            return t("archive.status_all");
        case "active":
            return t("archive.status_active");
        case "completed":
            return t("archive.status_completed");
        case "pending":
            return t("archive.status_pending");
        case "scheduled":
            return t("archive.status_scheduled");
        case "custom":
            return t("archive.status_custom");
    }
}

function cardStatusText(card: TipcardInfo): string {
    if (card.status === "active" && card.repeatCount > 0) {
        return t("archive.status_scheduled");
    }
    switch (card.status) {
        case "active":
            return t("archive.status_active");
        case "completed":
            return t("archive.status_completed");
        case "pending":
            return t("archive.status_pending");
        case "custom":
            return t("archive.status_custom");
        case "reviewed":
            return t("archive.status_reviewed");
        case "learned":
        case "known":
            return t("archive.status_learned");
        case "dismissed":
            return t("archive.status_dismissed");
        case "archived":
            return t("archive.status_archived");
        case "":
            return t("archive.status_unknown");
        default:
            return tf("archive.status_other", { status: card.status });
    }
}

function ArchiveCard({
    card,
    busy,
    onDelete,
    onDetail,
    onPin,
}: {
    card: TipcardInfo;
    busy: boolean;
    onDelete: (card: TipcardInfo) => void;
    onDetail: (card: TipcardInfo) => void;
    onPin: (card: TipcardInfo) => void;
}) {
    const images = imageUrls(card);
    const content = card.fullContent.trim() || card.compressedContent;
    return (
        <Card className="min-w-0">
            <CardHeader className="gap-3 border-b">
                <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-1.5">
                            <Badge variant="outline">
                                {card.topicName || t("archive.unassigned")}
                            </Badge>
                            <Badge variant="secondary">
                                {cardStatusText(card)}
                            </Badge>
                            {card.pinned ? (
                                <Badge>
                                    <PinIcon aria-hidden="true" />
                                    {t("archive.pinned")}
                                </Badge>
                            ) : null}
                        </div>
                        <h2 className="mt-2 line-clamp-2 text-base font-semibold">
                            {displayCardTitle(card)}
                        </h2>
                    </div>
                    <span className="shrink-0 font-mono text-[0.65rem] text-muted-foreground">
                        #{card.id.toString()}
                    </span>
                </div>
                <p className="flex items-center gap-1 text-xs text-muted-foreground">
                    <CalendarIcon className="size-3.5" aria-hidden="true" />
                    {humanDate(card.createdAt)}
                </p>
            </CardHeader>
            <CardContent className="flex flex-col gap-3">
                {images.length === 0 ? null : (
                    <div className="grid grid-cols-2 gap-2">
                        {images.slice(0, 4).map((src, index) => (
                            <LoadedImage
                                key={src}
                                src={src}
                                alt={tf("archive.image_alt", {
                                    title: displayCardTitle(card),
                                })}
                                className="size-full object-cover transition-transform hover:scale-105"
                                render={(image) => (
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        className="aspect-video h-auto w-full overflow-hidden rounded-md bg-muted p-0"
                                        onClick={() => onDetail(card)}
                                        aria-label={tf(
                                            "archive.open_image_details",
                                            {
                                                index: index + 1,
                                                title: displayCardTitle(card),
                                            },
                                        )}
                                    >
                                        {image}
                                    </Button>
                                )}
                            />
                        ))}
                    </div>
                )}
                <div
                    className="max-h-80 min-w-0 overflow-y-auto overscroll-contain pr-2"
                    data-testid={`archive-card-content-${card.id}`}
                >
                    <MarkdownContent content={content} />
                </div>
            </CardContent>
            <CardFooter className="flex-wrap justify-between gap-3 px-5 py-4">
                <span className="text-xs text-muted-foreground">
                    {card.repeatCount === 1
                        ? tf("format.review_count_one", {
                              count: card.repeatCount,
                          })
                        : tf("format.review_count_other", {
                              count: card.repeatCount,
                          })}
                </span>
                <div className="flex items-center gap-1">
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => onDetail(card)}
                    >
                        <EyeIcon data-icon="inline-start" />
                        {t("archive.details")}
                    </Button>
                    <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        disabled={busy}
                        onClick={() => onPin(card)}
                        aria-label={
                            card.pinned
                                ? tf("archive.unpin_card", {
                                      title: displayCardTitle(card),
                                  })
                                : tf("archive.pin_card", {
                                      title: displayCardTitle(card),
                                  })
                        }
                    >
                        {card.pinned ? <PinOffIcon /> : <PinIcon />}
                    </Button>
                    <Button
                        type="button"
                        variant="destructive"
                        size="icon-sm"
                        disabled={busy}
                        onClick={() => onDelete(card)}
                        aria-label={tf("common.delete_named", {
                            name: displayCardTitle(card),
                        })}
                    >
                        <Trash2Icon />
                    </Button>
                </div>
            </CardFooter>
        </Card>
    );
}

export function ArchivePage({ active = true }: { active?: boolean }) {
    const [cards, setCards] = useState<TipcardInfo[]>([]);
    const [loading, setLoading] = useState(true);
    const [busyId, setBusyId] = useState<bigint | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [query, setQuery] = useState("");
    const [status, setStatus] = useState<ArchiveStatus>(() =>
        typeof window === "undefined"
            ? "all"
            : parseArchiveSearch(window.location.search).status,
    );
    const [topic, setTopic] = useState(() =>
        typeof window === "undefined"
            ? ""
            : parseArchiveSearch(window.location.search).topic,
    );
    const [sort, setSort] = useState<ArchiveSort>("topic");
    const [deleteTarget, setDeleteTarget] = useState<TipcardInfo | null>(null);
    const [detailCard, setDetailCard] = useState<TipcardInfo | null>(null);
    const [lightboxOpen, setLightboxOpen] = useState(false);
    const [lightboxIndex, setLightboxIndex] = useState(0);

    const refresh = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            const result = await listTipcards();
            setCards(result.cards);
        } catch (cause) {
            setError(errorMessage(cause));
        } finally {
            setLoading(false);
        }
    }, []);

    useViewRefresh(active, refresh);

    useEffect(() => {
        if (typeof window === "undefined") return;
        const nextSearch = archiveSearch({ status, topic });
        if (window.location.search === nextSearch) return;
        window.history.replaceState(
            null,
            "",
            `${window.location.pathname}${nextSearch}${window.location.hash}`,
        );
    }, [status, topic]);

    const filters: ArchiveFilters = { query, status, topic, sort };
    const filtered = useMemo(
        () => filterArchiveCards(cards, filters),
        [cards, query, sort, status, topic],
    );
    const topics = useMemo(() => archiveTopics(cards), [cards]);
    const detailImages = detailCard === null ? [] : imageUrls(detailCard);

    const mutate = async (card: TipcardInfo, action: "pin" | "delete") => {
        setBusyId(card.id);
        setError(null);
        try {
            if (action === "pin") {
                await pinTipcard({
                    cardId: card.id,
                    pinned: !card.pinned,
                    idempotencyKey: newIdempotencyKey(),
                });
            } else {
                await deleteTipcard({
                    cardId: card.id,
                    idempotencyKey: newIdempotencyKey(),
                });
                setDeleteTarget(null);
                if (detailCard?.id === card.id) setDetailCard(null);
            }
            await refresh();
        } catch (cause) {
            setError(errorMessage(cause));
        } finally {
            setBusyId(null);
        }
    };

    return (
        <section
            className="mx-auto flex w-full max-w-7xl flex-col gap-5"
            data-testid="archive-page"
        >
            <header className="flex flex-wrap items-end justify-between gap-3">
                <div>
                    <p className="text-sm font-medium text-muted-foreground">
                        {t("archive.inventory")}
                    </p>
                    <h1 className="text-2xl font-semibold tracking-tight">
                        {t("archive.title")}
                    </h1>
                    <p className="mt-2 text-sm text-muted-foreground">
                        {tf("format.archive_card_count", {
                            shown: filtered.length,
                            total: cards.length,
                        })}
                    </p>
                </div>
                <Button
                    type="button"
                    variant="outline"
                    onClick={() => void refresh()}
                    disabled={loading || busyId !== null}
                >
                    <RefreshCwIcon data-icon="inline-start" />
                    {t("common.refresh")}
                </Button>
            </header>

            {error === null ? null : (
                <Alert variant="destructive">
                    <AlertTitle>{t("archive.load_failed")}</AlertTitle>
                    <AlertDescription>{error}</AlertDescription>
                </Alert>
            )}

            {topic === "" ? null : (
                <Button
                    type="button"
                    variant="secondary"
                    className="self-start"
                    data-testid="archive-topic-back"
                    onClick={() => {
                        window.history.pushState({}, "", "/grounding");
                        window.dispatchEvent(
                            new PopStateEvent("popstate", {
                                state: window.history.state,
                            }),
                        );
                    }}
                >
                    <ArrowLeftIcon data-icon="inline-start" />
                    {status === "pending"
                        ? tf("archive.pending_cards_for", { topic })
                        : status === "scheduled"
                          ? tf("archive.scheduled_cards_for", { topic })
                          : tf("archive.cards_for", { topic })}
                </Button>
            )}

            <div className="flex flex-col gap-3 rounded-xl border bg-card p-3 sm:flex-row sm:flex-wrap sm:items-center">
                <div className="relative min-w-0 flex-1 sm:min-w-64">
                    <SearchIcon
                        className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <Input
                        value={query}
                        onChange={(event) => setQuery(event.target.value)}
                        placeholder={t("archive.search_placeholder")}
                        aria-label={t("archive.search_label")}
                        className="pl-8"
                    />
                </div>
                <Select
                    value={status}
                    onValueChange={(value) => {
                        if (isArchiveStatus(value)) setStatus(value);
                    }}
                >
                    <SelectTrigger
                        className="w-full sm:w-40"
                        aria-label={t("archive.status_filter_label")}
                    >
                        <SelectValue>{archiveStatusText(status)}</SelectValue>
                    </SelectTrigger>
                    <SelectContent>
                        {statuses.map((item) => (
                            <SelectItem key={item} value={item}>
                                {archiveStatusText(item)}
                            </SelectItem>
                        ))}
                    </SelectContent>
                </Select>
                <Select
                    value={topic}
                    onValueChange={(value) => value !== null && setTopic(value)}
                >
                    <SelectTrigger
                        className="w-full sm:w-48"
                        aria-label={t("archive.topic_filter_label")}
                    >
                        <SelectValue>
                            {topic || t("archive.all_topics")}
                        </SelectValue>
                    </SelectTrigger>
                    <SelectContent>
                        <SelectItem value="">
                            {t("archive.all_topics")}
                        </SelectItem>
                        {topics.map((item) => (
                            <SelectItem key={item} value={item}>
                                {item}
                            </SelectItem>
                        ))}
                    </SelectContent>
                </Select>
                <div
                    className="flex gap-1"
                    role="group"
                    aria-label={t("archive.sort_label")}
                >
                    <Button
                        type="button"
                        size="sm"
                        variant={sort === "topic" ? "secondary" : "ghost"}
                        aria-pressed={sort === "topic"}
                        onClick={() => setSort("topic")}
                    >
                        {t("archive.sort_topic")}
                    </Button>
                    <Button
                        type="button"
                        size="sm"
                        variant={sort === "date" ? "secondary" : "ghost"}
                        aria-pressed={sort === "date"}
                        onClick={() => setSort("date")}
                    >
                        {t("archive.sort_date")}
                    </Button>
                </div>
                <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                        setQuery("");
                        setStatus("all");
                        setTopic("");
                        setSort("topic");
                    }}
                >
                    {t("archive.clear")}
                </Button>
            </div>

            {loading ? (
                <p
                    className="py-16 text-center text-sm text-muted-foreground"
                    role="status"
                >
                    {t("archive.loading")}
                </p>
            ) : filtered.length === 0 ? (
                <div className="rounded-xl border border-dashed p-16 text-center">
                    <ArchiveIcon
                        className="mx-auto mb-3 size-9 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <p className="font-medium">{t("archive.no_match")}</p>
                    <p className="mt-1 text-sm text-muted-foreground">
                        {t("archive.no_match_description")}
                    </p>
                </div>
            ) : (
                <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
                    {filtered.map((card) => (
                        <ArchiveCard
                            key={card.id.toString()}
                            card={card}
                            busy={busyId === card.id}
                            onPin={(item) => void mutate(item, "pin")}
                            onDelete={setDeleteTarget}
                            onDetail={(item) => setDetailCard(item)}
                        />
                    ))}
                </div>
            )}

            <AlertDialog
                open={deleteTarget !== null}
                onOpenChange={(open) => {
                    if (!open && busyId === null) setDeleteTarget(null);
                }}
            >
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {t("confirm.delete_card")}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {tf("confirm.delete_archived_card_description", {
                                title:
                                    deleteTarget === null
                                        ? t("archive.this_card")
                                        : displayCardTitle(deleteTarget),
                            })}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <AlertDialogCancel disabled={busyId !== null}>
                            {t("common.cancel")}
                        </AlertDialogCancel>
                        <AlertDialogAction
                            variant="destructive"
                            disabled={busyId !== null}
                            onClick={() => {
                                if (deleteTarget !== null)
                                    void mutate(deleteTarget, "delete");
                            }}
                        >
                            {busyId === deleteTarget?.id
                                ? t("common.deleting")
                                : t("common.delete_card")}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>

            <Dialog
                open={detailCard !== null}
                onOpenChange={(open) => {
                    if (!open) setDetailCard(null);
                }}
            >
                <DialogContent
                    className="max-h-[92dvh] w-[calc(100%_-_1rem)] max-w-5xl overflow-y-auto p-5 sm:w-[calc(100%_-_2rem)] sm:max-w-5xl sm:p-7"
                    data-testid="archive-detail-dialog"
                >
                    {detailCard === null ? null : (
                        <>
                            <DialogHeader>
                                <div className="flex flex-wrap gap-1.5">
                                    <Badge variant="outline">
                                        {detailCard.topicName ||
                                            t("archive.unassigned")}
                                    </Badge>
                                    <Badge variant="secondary">
                                        {cardStatusText(detailCard)}
                                    </Badge>
                                </div>
                                <DialogTitle>
                                    {displayCardTitle(detailCard)}
                                </DialogTitle>
                                <DialogDescription>
                                    {tf("format.created_card", {
                                        date: humanDate(detailCard.createdAt),
                                        id: detailCard.id.toString(),
                                    })}
                                </DialogDescription>
                            </DialogHeader>
                            {detailImages.length === 0 ? null : (
                                <div className="grid grid-cols-2 gap-2">
                                    {detailImages.map((src, index) => (
                                        <LoadedImage
                                            key={src}
                                            src={src}
                                            alt={tf("archive.image_alt", {
                                                title: displayCardTitle(
                                                    detailCard,
                                                ),
                                            })}
                                            className="size-full object-contain"
                                            render={(image) => (
                                                <Button
                                                    type="button"
                                                    variant="ghost"
                                                    onClick={() => {
                                                        setLightboxIndex(index);
                                                        setLightboxOpen(true);
                                                    }}
                                                    className="aspect-video h-auto w-full overflow-hidden rounded-md bg-muted p-0"
                                                    aria-label={tf(
                                                        "archive.open_image",
                                                        { index: index + 1 },
                                                    )}
                                                >
                                                    {image}
                                                </Button>
                                            )}
                                        />
                                    ))}
                                </div>
                            )}
                            <MarkdownContent
                                className="min-w-0 text-base sm:text-[1.05rem] sm:leading-8"
                                content={
                                    detailCard.fullContent ||
                                    detailCard.compressedContent
                                }
                            />
                            <CardImageManager
                                card={inventoryToFlowCard(detailCard)}
                                onChanged={(next) => {
                                    setDetailCard({
                                        ...detailCard,
                                        images: next.images,
                                    });
                                    void refresh();
                                }}
                            />
                            <DialogFooter showCloseButton />
                        </>
                    )}
                </DialogContent>
            </Dialog>
            <ImageLightbox
                open={lightboxOpen}
                images={detailImages.map((src) => ({
                    src,
                    alt: tf("archive.image_alt", {
                        title:
                            detailCard === null
                                ? t("archive.untitled_card")
                                : displayCardTitle(detailCard),
                    }),
                }))}
                initialIndex={lightboxIndex}
                onOpenChange={setLightboxOpen}
            />
        </section>
    );
}
