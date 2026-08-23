// Pure Flow pagination state: a discriminated cursor and total functions for
// the load-more success/failure transitions. No fetch, no React — the
// component owns when these run; this module owns what the next state is.

import type { FlowCardInfo, FlowCardPage } from "../generated/denpie_pb";
import { mergeCardsById } from "./flow-view";

/**
 * Pagination modeled as a discriminated union so illegal combinations are
 * unrepresentable: `more` always carries a non-empty token; `end` never does.
 */
export type FlowCursor = { kind: "end" } | { kind: "more"; pageToken: string };

/** Fields a fetched page contributes to cursor construction. */
export type PageCursorFields = {
  nextPageToken: string | undefined;
  hasMore: boolean;
};

/** Derive the cursor for a fetched page from its generated fields. */
export function cursorFromPage(page: PageCursorFields): FlowCursor {
  if (page.hasMore) {
    if (page.nextPageToken === undefined || page.nextPageToken === "") {
      throw new TypeError("list_flow_cards reported more pages without a cursor");
    }
    return { kind: "more", pageToken: page.nextPageToken };
  }
  if (page.nextPageToken !== undefined && page.nextPageToken !== "") {
    throw new TypeError("list_flow_cards returned a cursor for a final page");
  }
  return { kind: "end" };
}

/** Success transition: merge the new page and derive the next cursor. */
export function loadMoreSuccess(
  cards: readonly FlowCardInfo[],
  page: Pick<FlowCardPage, "cards"> & { cursor: FlowCursor },
): { cards: FlowCardInfo[]; cursor: FlowCursor } {
  return {
    cards: mergeCardsById(cards, page.cards),
    cursor: page.cursor,
  };
}
// On failure the component keeps the rendered cards and reuses the same
// `more` cursor verbatim, so a retry re-requests exactly the failed page.
