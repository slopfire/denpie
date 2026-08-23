// Pure Flow review action mapping and slot-list transitions for the review
// UI. No fetch, no React — the component owns when requests launch; this
// module owns which grade/action each card type offers and how fetched pages
// become/extend `ReviewSlot[]` lists without disturbing live slot states.

import {
  ReviewActionValue,
  type FlowCardInfo,
} from "../generated/denpie_pb";
import type {
  ReviewFailureFields,
  ReviewSlot,
} from "./flow-review-state";
import { TransportError } from "./api-v1/transport";

/** One concrete review mutation the UI can offer for a card. */
export interface ReviewChoice {
  /** Stable test-id fragment (`again`, `learned`, `acknowledge`, …). */
  id: string;
  label: string;
  grade: number;
  action: ReviewActionValue;
}

/** Primary buttons plus, for repeatable tips, the grouped skip reasons. */
export interface ReviewActions {
  primary: ReviewChoice[];
  /**
   * Skip reasons rendered inside one native shadcn `DropdownMenuGroup`;
   * absent for card types without named skip actions.
   */
  skipGroup?: ReviewChoice[];
}

/** Exact action mapping per active tipcard type (protocol vocabulary only). */
export function reviewActionsFor(card: FlowCardInfo): ReviewActions {
  switch (card.tipcardType) {
    case "casual_tip":
    case "manual_tip":
      return {
        primary: [
          {
            id: "dismiss",
            label: "Dismiss",
            grade: 3,
            action: ReviewActionValue.SKIP_NOT_INTERESTED,
          },
          {
            id: "acknowledge",
            label: "Acknowledge",
            grade: 3,
            action: ReviewActionValue.ACKNOWLEDGE,
          },
        ],
      };
    case "repeatable_tip":
      return {
        primary: [
          {
            id: "again",
            label: "Again",
            grade: 1,
            action: ReviewActionValue.AGAIN,
          },
          {
            id: "learned",
            label: "Learned",
            grade: 5,
            action: ReviewActionValue.LEARNED,
          },
        ],
        skipGroup: [
          {
            id: "known",
            label: "Known",
            grade: 5,
            action: ReviewActionValue.SKIP_KNOWN,
          },
          {
            id: "not-interested",
            label: "Not interested",
            grade: 3,
            action: ReviewActionValue.SKIP_NOT_INTERESTED,
          },
          {
            id: "too-difficult",
            label: "Too difficult",
            grade: 1,
            action: ReviewActionValue.SKIP_TOO_DIFFICULT,
          },
        ],
      };
    default:
      // Every other active type grades with no named action string.
      return {
        primary: [
          { id: "again", label: "Again", grade: 1, action: ReviewActionValue.UNSPECIFIED },
          { id: "good", label: "Good", grade: 3, action: ReviewActionValue.UNSPECIFIED },
          { id: "easy", label: "Easy", grade: 5, action: ReviewActionValue.UNSPECIFIED },
        ],
      };
  }
}

/** A fetched initial page becomes idle slots in server order. */
export function slotsFromCards(cards: readonly FlowCardInfo[]): ReviewSlot[] {
  return cards.map((card) => ({ kind: "idle", card }) satisfies ReviewSlot);
}

/**
 * Load-more transition over slots: every existing slot keeps its exact state
 * (idle, reviewing, error, or placeholder), and each newly fetched card that
 * is not already represented by any slot appends as an idle slot in page
 * order. Duplicate IDs inside the incoming page are also ignored.
 */
export function appendIdleSlots(
  slots: readonly ReviewSlot[],
  incoming: readonly FlowCardInfo[],
): ReviewSlot[] {
  const knownIds = new Set(
    slots.map((slot) =>
      slot.kind === "idle" ||
      slot.kind === "reviewing" ||
      slot.kind === "error"
        ? slot.card.id
        : slot.reviewedCardId,
    ),
  );
  const appended: ReviewSlot[] = [];
  for (const card of incoming) {
    if (knownIds.has(card.id)) continue;
    knownIds.add(card.id);
    appended.push({ kind: "idle", card });
  }
  return [...slots, ...appended];
}

/**
 * Failure classification at the review boundary: a `TransportError` carries
 * the transport's own indeterminate-mutation verdict (the request may have
 * committed, so a safe retry must reuse the exact key); every other error is
 * conservatively indeterminate. Once a mutation has been sent, a protocol or
 * client-side decoding invariant failure cannot prove that it did not commit.
 */
export function classifyReviewError(error: unknown): ReviewFailureFields {
  if (error instanceof TransportError) {
    return {
      mutationOutcomeIndeterminate: error.mutationOutcomeIndeterminate,
      message: error.message,
    };
  }
  return {
    mutationOutcomeIndeterminate: true,
    message: error instanceof Error ? error.message : String(error),
  };
}
