import { useMemo, useState } from "react";
import { PinIcon, PinOffIcon, Trash2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { CardBodies } from "@/components/flow/Flow";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { LabCardFixtureJson } from "@/lib/lab-card-view";
import { repeatableStackLayers } from "@/lib/flow-view";
import {
    continueLabCard,
    deleteLabCard,
    labCardsFromFixtures,
    pinLabCard,
    reviewLabCard,
} from "@/lib/lab-card-state";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export interface LabGalleryProps {
    fixtures: readonly LabCardFixtureJson[];
}

export function LabGallery({ fixtures }: LabGalleryProps) {
    const initial = useMemo(() => labCardsFromFixtures(fixtures), [fixtures]);
    const [cards, setCards] = useState(initial);
    const [fullscreenId, setFullscreenId] = useState<bigint | null>(null);

    return (
        <TooltipProvider>
            <div className="mx-auto grid w-full max-w-6xl gap-6 px-4 py-8 md:grid-cols-2">
                {cards.map(({ fixtureId, notes, reviewMessage, card }) => {
                    const stackLayers = repeatableStackLayers(card);
                    const fullscreen = fullscreenId === card.id;
                    return (
                        <article
                            key={fixtureId}
                            data-testid={`lab-fixture-${fixtureId}`}
                            className="space-y-3"
                        >
                            <div className="flex items-baseline justify-between gap-4">
                                <h2 className="font-mono text-sm font-semibold">
                                    {fixtureId}
                                </h2>
                                <p className="text-xs text-muted-foreground">
                                    {notes}
                                </p>
                            </div>
                            <div
                                className={cn(
                                    "relative isolate h-full",
                                    stackLayers > 0 && "mr-3 mb-3",
                                    fullscreen &&
                                        "fixed inset-0 z-70 m-0 bg-background p-6",
                                )}
                                data-repeatable-stack={
                                    stackLayers > 0 ? stackLayers : undefined
                                }
                            >
                                {Array.from(
                                    { length: stackLayers },
                                    (_, index) => {
                                        const layer = index + 1;
                                        return (
                                            <div
                                                key={layer}
                                                aria-hidden="true"
                                                data-stack-layer={layer}
                                                className={`absolute inset-0 rounded-md border border-border bg-card shadow-sm ${
                                                    layer === 1
                                                        ? "translate-x-1 translate-y-1 opacity-85"
                                                        : layer === 2
                                                          ? "translate-x-2 translate-y-2 opacity-70"
                                                          : "translate-x-3 translate-y-3 opacity-55"
                                                }`}
                                                style={{ zIndex: -layer }}
                                            />
                                        );
                                    },
                                )}
                                <Card className="relative z-10 flex h-full min-h-60 flex-col gap-0 overflow-hidden rounded-md py-0 ring-border">
                                    <CardBodies
                                        card={card}
                                        fullscreen={fullscreen}
                                        onToggleFullscreen={() =>
                                            setFullscreenId((current) =>
                                                current === card.id
                                                    ? null
                                                    : card.id,
                                            )
                                        }
                                        detailActions={
                                            <>
                                                {reviewMessage === null ? (
                                                    <Button
                                                        size="sm"
                                                        onClick={() =>
                                                            setCards((current) =>
                                                                reviewLabCard(
                                                                    current,
                                                                    card.id,
                                                                    t(
                                                                        "flow.review_saved",
                                                                    ),
                                                                ),
                                                            )
                                                        }
                                                    >
                                                        {t(
                                                            "flow.review_action.good",
                                                        )}
                                                    </Button>
                                                ) : (
                                                    <Button
                                                        size="sm"
                                                        onClick={() =>
                                                            setCards((current) =>
                                                                continueLabCard(
                                                                    current,
                                                                    card.id,
                                                                ),
                                                            )
                                                        }
                                                    >
                                                        {t("lab.continue")}
                                                    </Button>
                                                )}
                                                <Button
                                                    type="button"
                                                    variant="outline"
                                                    size="icon-sm"
                                                    aria-label={
                                                        card.pinned
                                                            ? t("lab.unpin")
                                                            : t("lab.pin")
                                                    }
                                                    onClick={() =>
                                                        setCards((current) =>
                                                            pinLabCard(
                                                                current,
                                                                card.id,
                                                                !card.pinned,
                                                            ),
                                                        )
                                                    }
                                                >
                                                    {card.pinned ? (
                                                        <PinOffIcon />
                                                    ) : (
                                                        <PinIcon />
                                                    )}
                                                </Button>
                                                <Button
                                                    type="button"
                                                    variant="destructive"
                                                    size="icon-sm"
                                                    aria-label={t("lab.delete")}
                                                    onClick={() =>
                                                        setCards((current) =>
                                                            deleteLabCard(
                                                                current,
                                                                card.id,
                                                            ),
                                                        )
                                                    }
                                                >
                                                    <Trash2Icon />
                                                </Button>
                                            </>
                                        }
                                    />
                                    {reviewMessage === null ? null : (
                                        <CardContent
                                            data-testid={`lab-review-message-${fixtureId}`}
                                            className="px-4 pb-4 text-sm text-muted-foreground"
                                        >
                                            {reviewMessage}
                                        </CardContent>
                                    )}
                                </Card>
                            </div>
                        </article>
                    );
                })}
            </div>
        </TooltipProvider>
    );
}
