import type { FlowCardInfo } from "@/generated/denpie_pb";
import type { LabCardFixtureJson } from "@/lib/lab-card-view";
import { fixtureToFlowCard } from "@/lib/lab-card-view";
import type { ReviewSlot } from "@/lib/flow-review-state";

type LiveReviewSlot = Extract<
    ReviewSlot,
    { kind: "idle" | "reviewing" | "error" }
>;

type FollowUpReviewSlot = Extract<
    ReviewSlot,
    { kind: "completed" | "awaitingRefill" | "continuing" | "continueError" }
>;

interface LabCardMetadata {
    readonly fixtureId: string;
    readonly notes: string;
}

export type LabCardState = LabCardMetadata &
    (
        | {
              readonly kind: "card";
              readonly slot: LiveReviewSlot;
          }
        | {
              readonly kind: "followUp";
              readonly slot: FollowUpReviewSlot;
              readonly reviewedCard: FlowCardInfo;
          }
    );

function reviewedIdentity(card: FlowCardInfo) {
    return {
        reviewedCardId: card.id,
        topicName: card.topicName,
        title: card.title,
        createdAt: card.createdAt,
        tipcardType: card.tipcardType,
        pinned: card.pinned,
    };
}

function fixtureState(
    fixture: LabCardFixtureJson,
    index: number,
): LabCardState {
    const card = fixtureToFlowCard(fixture, index);
    const metadata = { fixtureId: fixture.id, notes: fixture.notes };
    if (
        card.status !== "reviewed" ||
        fixture.review_message === "Review saved"
    ) {
        return { ...metadata, kind: "card", slot: { kind: "idle", card } };
    }
    const identity = reviewedIdentity(card);
    if (fixture.review_message !== null) {
        return {
            ...metadata,
            kind: "followUp",
            reviewedCard: card,
            slot: { kind: "completed", ...identity },
        };
    }
    return {
        ...metadata,
        kind: "followUp",
        reviewedCard: card,
        slot: {
            kind: "awaitingRefill",
            ...identity,
            refillToken: 1,
            refillAttempts: 0,
        },
    };
}

export function labCardsFromFixtures(
    fixtures: readonly LabCardFixtureJson[],
): LabCardState[] {
    return fixtures.map(fixtureState);
}

export function labCardId(state: LabCardState): bigint {
    return state.kind === "card"
        ? state.slot.card.id
        : state.slot.reviewedCardId;
}

export function pinLabCard(
    cards: readonly LabCardState[],
    id: bigint,
    pinned: boolean,
): LabCardState[] {
    return cards.map((item) =>
        item.kind === "card" && item.slot.card.id === id
            ? {
                  ...item,
                  slot: {
                      ...item.slot,
                      card: { ...item.slot.card, pinned },
                  },
              }
            : item,
    );
}

export function deleteLabCard(
    cards: readonly LabCardState[],
    id: bigint,
): LabCardState[] {
    return cards.filter((item) => labCardId(item) !== id);
}

export function reviewLabCard(
    cards: readonly LabCardState[],
    id: bigint,
): LabCardState[] {
    return cards.map((item) => {
        if (
            item.kind !== "card" ||
            item.slot.kind !== "idle" ||
            item.slot.card.id !== id
        ) {
            return item;
        }
        return {
            fixtureId: item.fixtureId,
            notes: item.notes,
            kind: "followUp",
            reviewedCard: item.slot.card,
            slot: {
                kind: "completed",
                ...reviewedIdentity(item.slot.card),
            },
        };
    });
}

export function continueLabCard(
    cards: readonly LabCardState[],
    id: bigint,
): LabCardState[] {
    return cards.map((item) =>
        item.kind === "followUp" &&
        item.slot.kind === "completed" &&
        item.slot.reviewedCardId === id
            ? {
                  fixtureId: item.fixtureId,
                  notes: item.notes,
                  kind: "card",
                  slot: {
                      kind: "idle",
                      card: {
                          ...item.reviewedCard,
                          status: "active",
                      },
                  },
              }
            : item,
    );
}
