import { useEffect, useMemo, useState } from "react";
import { RotateCcwIcon } from "lucide-react";
import { ReviewSlotCard } from "@/components/flow/Flow";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Button } from "@/components/ui/button";
import type { LabCardFixtureJson } from "@/lib/lab-card-view";
import type { DeleteCardState } from "@/lib/flow-delete-state";
import type { PinCardState } from "@/lib/flow-pin-state";
import {
    continueLabCard,
    deleteLabCard,
    labCardsFromFixtures,
    pinLabCard,
    reviewLabCard,
} from "@/lib/lab-card-state";
import {
    DEFAULT_LAB_GALLERY_SETTINGS,
    labGallerySearch,
    matchesLabFixture,
    parseLabGallerySettings,
    type LabGalleryColumns,
} from "@/lib/lab-gallery-controls";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export interface LabGalleryProps {
    fixtures: readonly LabCardFixtureJson[];
}

const IDLE_PIN_STATE = { kind: "idle" } satisfies PinCardState;
const IDLE_DELETE_STATE = { kind: "idle" } satisfies DeleteCardState;
const ignoreCardId = (_cardId: bigint) => {};

export function LabGallery({ fixtures }: LabGalleryProps) {
    const initial = useMemo(() => labCardsFromFixtures(fixtures), [fixtures]);
    const [cards, setCards] = useState(initial);
    const [settings, setSettings] = useState(DEFAULT_LAB_GALLERY_SETTINGS);
    const [queryLoaded, setQueryLoaded] = useState(false);

    useEffect(() => {
        setSettings(parseLabGallerySettings(window.location.search));
        setQueryLoaded(true);
    }, []);

    useEffect(() => {
        if (!queryLoaded) return;
        document.documentElement.classList.toggle(
            "dark",
            settings.theme === "dark",
        );
        const next = `${window.location.pathname}${labGallerySearch(settings)}`;
        window.history.replaceState(null, "", next);
    }, [queryLoaded, settings]);

    const visibleCards = cards.filter((item) => {
        const card = item.kind === "card" ? item.slot.card : item.reviewedCard;
        return matchesLabFixture(settings.filter, [
            item.fixtureId,
            item.notes,
            card.topicName,
            card.title,
            card.tipcardType,
            card.status,
        ]);
    });

    const gridColumns =
        settings.columns === 1
            ? "grid-cols-1"
            : settings.columns === 2
              ? "md:grid-cols-2"
              : settings.columns === 3
                ? "md:grid-cols-2 xl:grid-cols-3"
                : "md:grid-cols-2 xl:grid-cols-4";
    const viewportWidth =
        settings.viewport === "mobile"
            ? "max-w-[360px]"
            : settings.viewport === "tablet"
              ? "max-w-3xl"
              : "max-w-[1600px]";

    return (
        <TooltipProvider>
            <div className="sticky top-0 z-60 border-b border-border bg-background/95 px-4 py-3 backdrop-blur">
                <div className="mx-auto flex max-w-[1600px] flex-wrap items-end gap-3">
                    <label className="flex min-w-52 flex-1 flex-col gap-1 text-xs text-muted-foreground">
                        {t("lab.controls.filter")}
                        <input
                            type="search"
                            value={settings.filter}
                            data-testid="lab-filter"
                            onChange={(event) =>
                                setSettings((current) => ({
                                    ...current,
                                    filter: event.currentTarget.value,
                                }))
                            }
                            className="h-9 rounded-md border border-input bg-background px-3 text-sm text-foreground"
                        />
                    </label>
                    <LabSelect
                        label={t("lab.controls.layout")}
                        value={settings.layout}
                        testId="lab-layout"
                        options={[
                            ["grid", t("lab.controls.grid")],
                            ["list", t("lab.controls.list")],
                        ]}
                        onChange={(layout) =>
                            setSettings((current) => ({ ...current, layout }))
                        }
                    />
                    <LabSelect
                        label={t("lab.controls.columns")}
                        value={String(settings.columns)}
                        testId="lab-columns"
                        disabled={settings.layout === "list"}
                        options={[
                            ["1", "1"],
                            ["2", "2"],
                            ["3", "3"],
                            ["4", "4"],
                        ]}
                        onChange={(value) => {
                            const columns: LabGalleryColumns =
                                value === "1"
                                    ? 1
                                    : value === "3"
                                      ? 3
                                      : value === "4"
                                        ? 4
                                        : 2;
                            setSettings((current) => ({ ...current, columns }));
                        }}
                    />
                    <LabSelect
                        label={t("lab.controls.viewport")}
                        value={settings.viewport}
                        testId="lab-viewport"
                        options={[
                            ["fluid", t("lab.controls.fluid")],
                            ["tablet", t("lab.controls.tablet")],
                            ["mobile", t("lab.controls.mobile")],
                        ]}
                        onChange={(viewport) =>
                            setSettings((current) => ({ ...current, viewport }))
                        }
                    />
                    <LabSelect
                        label={t("lab.controls.theme")}
                        value={settings.theme}
                        testId="lab-theme"
                        options={[
                            ["dark", t("lab.controls.dark")],
                            ["light", t("lab.controls.light")],
                        ]}
                        onChange={(theme) =>
                            setSettings((current) => ({ ...current, theme }))
                        }
                    />
                    <Button
                        type="button"
                        variant="outline"
                        data-testid="lab-reset"
                        onClick={() => {
                            setCards(initial);
                            setSettings(DEFAULT_LAB_GALLERY_SETTINGS);
                        }}
                    >
                        <RotateCcwIcon /> {t("lab.controls.reset")}
                    </Button>
                </div>
            </div>
            <div
                className={cn(
                    "mx-auto grid w-full gap-6 px-4 py-8 transition-[max-width]",
                    viewportWidth,
                    settings.layout === "list" ? "grid-cols-1" : gridColumns,
                )}
                data-testid="lab-gallery-grid"
                data-layout={settings.layout}
                data-columns={settings.columns}
                data-viewport={settings.viewport}
            >
                {visibleCards.map((item) => (
                    <article
                        key={item.fixtureId}
                        data-testid={`lab-fixture-${item.fixtureId}`}
                        data-lab-state={item.slot.kind}
                        className="space-y-3"
                    >
                        <div className="flex items-baseline justify-between gap-4">
                            <h2 className="font-mono text-sm font-semibold">
                                {item.fixtureId}
                            </h2>
                            <p className="text-xs text-muted-foreground">
                                {item.notes}
                            </p>
                        </div>
                        <ReviewSlotCard
                            slot={item.slot}
                            onReview={(cardId) =>
                                setCards((current) =>
                                    reviewLabCard(current, cardId),
                                )
                            }
                            onRetry={ignoreCardId}
                            onContinue={(cardId) =>
                                setCards((current) =>
                                    continueLabCard(current, cardId),
                                )
                            }
                            pinCard={IDLE_PIN_STATE}
                            deleteCard={IDLE_DELETE_STATE}
                            onPinToggle={(cardId, pinned) =>
                                setCards((current) =>
                                    pinLabCard(current, cardId, pinned),
                                )
                            }
                            onPinRetry={ignoreCardId}
                            onDeleteConfirm={(cardId) =>
                                setCards((current) =>
                                    deleteLabCard(current, cardId),
                                )
                            }
                            onDeleteRetry={ignoreCardId}
                            onPinnedDragStart={ignoreCardId}
                            onPinnedDragEnd={ignoreCardId}
                            loadDetailCard={async (cardId) => {
                                const detailCard =
                                    item.kind === "card"
                                        ? item.slot.card
                                        : item.reviewedCard;
                                if (detailCard.id !== cardId) {
                                    throw new Error(
                                        `Fixture ${item.fixtureId} has no card ${cardId}`,
                                    );
                                }
                                return detailCard;
                            }}
                        />
                    </article>
                ))}
            </div>
        </TooltipProvider>
    );
}

function LabSelect<T extends string>({
    label,
    value,
    options,
    testId,
    disabled = false,
    onChange,
}: {
    label: string;
    value: T;
    options: readonly (readonly [T, string])[];
    testId: string;
    disabled?: boolean;
    onChange: (value: T) => void;
}) {
    return (
        <label className="flex flex-col gap-1 text-xs text-muted-foreground">
            {label}
            <select
                value={value}
                disabled={disabled}
                data-testid={testId}
                onChange={(event) => {
                    const selected = options.find(
                        ([option]) => option === event.currentTarget.value,
                    );
                    if (selected !== undefined) onChange(selected[0]);
                }}
                className="h-9 rounded-md border border-input bg-background px-3 text-sm text-foreground disabled:opacity-50"
            >
                {options.map(([option, text]) => (
                    <option key={option} value={option}>
                        {text}
                    </option>
                ))}
            </select>
        </label>
    );
}
