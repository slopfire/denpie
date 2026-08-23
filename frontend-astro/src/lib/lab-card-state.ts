import type { FlowCardInfo } from "@/generated/denpie_pb";
import type { LabCardFixtureJson } from "@/lib/lab-card-view";
import { fixtureToFlowCard } from "@/lib/lab-card-view";

export interface LabCardState {
    readonly fixtureId: string;
    readonly notes: string;
    readonly reviewMessage: string | null;
    readonly card: FlowCardInfo;
}

export function labCardsFromFixtures(
    fixtures: readonly LabCardFixtureJson[],
): LabCardState[] {
    return fixtures.map((fixture, index) => ({
        fixtureId: fixture.id,
        notes: fixture.notes,
        reviewMessage: fixture.review_message ?? null,
        card: fixtureToFlowCard(fixture, index),
    }));
}

export function pinLabCard(
    cards: readonly LabCardState[],
    id: bigint,
    pinned: boolean,
): LabCardState[] {
    return cards.map((item) =>
        item.card.id === id
            ? { ...item, card: { ...item.card, pinned } }
            : item,
    );
}

export function deleteLabCard(
    cards: readonly LabCardState[],
    id: bigint,
): LabCardState[] {
    return cards.filter((item) => item.card.id !== id);
}

export function reviewLabCard(
    cards: readonly LabCardState[],
    id: bigint,
    message: string,
): LabCardState[] {
    return cards.map((item) =>
        item.card.id === id
            ? {
                  ...item,
                  reviewMessage: message,
                  card: { ...item.card, status: "reviewed" },
              }
            : item,
    );
}

export function continueLabCard(
    cards: readonly LabCardState[],
    id: bigint,
): LabCardState[] {
    return cards.map((item) =>
        item.card.id === id
            ? {
                  ...item,
                  reviewMessage: null,
                  card: { ...item.card, status: "active" },
              }
            : item,
    );
}
