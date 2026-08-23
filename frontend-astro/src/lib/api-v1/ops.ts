import {
    AppendTipcardImagesRequestSchema,
    ContinueDailyReviewRequestSchema,
    DeleteByIdRequestSchema,
    EmptySchema,
    GetByIdRequestSchema,
    ListFlowCardsRequestSchema,
    PinTipcardRequestSchema,
    ReviewAndAdvanceRequestSchema,
    ReplaceTipcardImagesRequestSchema,
    TipsRequestV1Schema,
} from "../../generated/denpie_pb";
import type {
    ApiInfo,
    ApiResponse,
    DeleteByIdRequest,
    FlowCardInfo,
    FlowCardPage,
    GetByIdRequest,
    PinTipcardRequest,
    ListFlowCardsRequest,
    ReviewActionValue,
    ReviewAndAdvanceRequest,
    ReviewAndAdvanceResponse,
    TipsRequestV1,
    TipsResponse,
} from "../../generated/denpie_pb";
import { create } from "@bufbuild/protobuf";
import type { CallDeps } from "./transport";
import {
    buildEnvelope,
    callEnvelope,
    callMutationWithKeyEnvelope,
    newRequestId,
} from "./transport";
import {
    invalidateReadCache,
    withReadCache,
    readCacheKey,
    READ_CACHE_DEFAULT_TTL_MS as READ_CACHE_TTL_MS,
} from "./cache";
import { cursorFromPage, type FlowCursor } from "../flow-state";

export const getApiInfoOp = {
    case: "getApiInfo",
    value: create(EmptySchema, {}),
} as const;

export async function getApiInfo(
    deps: CallDeps = {},
): Promise<{ requestId: string; info: ApiInfo }> {
    return withReadCache(readCacheKey("getApiInfo"), READ_CACHE_TTL_MS, () =>
        fetchApiInfo(deps),
    );
}

/** Uncached producer behind {@link getApiInfo}. */
async function fetchApiInfo(
    deps: CallDeps,
): Promise<{ requestId: string; info: ApiInfo }> {
    const envelope = buildEnvelope(
        getApiInfoOp,
        newRequestId("info"),
        null,
        false,
    );
    const response: ApiResponse = await callEnvelope(envelope, deps);
    if (response.result.case !== "apiInfo") {
        throw new TypeError(
            `get_api_info returned unexpected result case ${String(response.result.case)}`,
        );
    }
    return { requestId: envelope.requestId, info: response.result.value };
}

export const FLOW_PAGE_DEFAULT_SIZE = 48;

export const listFlowCardsOp = (request: ListFlowCardsRequest) =>
    ({ case: "listFlowCards", value: request }) as const;

/** Result of a successful `list_flow_cards` read. */
export interface FlowCardsPage {
    requestId: string;
    cards: FlowCardPage["cards"];
    cursor: FlowCursor;
}

export interface ListFlowCardsOptions {
    /** Defaults to {@link FLOW_PAGE_DEFAULT_SIZE}. */
    pageSize?: number;
    /** Previous page's `nextPageToken`; omitted for page 1 (empty wire token). */
    pageToken?: string;
    /** Test double hook; defaults to the real transport. */
    deps?: CallDeps;
}

/**
 * Typed read of `list_flow_cards`; returns the `flow_card_page` result case.
 * Served from a short-TTL read cache; see {@link withReadCache}.
 */
export async function listFlowCards({
    pageSize = FLOW_PAGE_DEFAULT_SIZE,
    pageToken,
    deps = {},
}: ListFlowCardsOptions = {}): Promise<FlowCardsPage> {
    return withReadCache(
        readCacheKey(
            "listFlowCards",
            pageSize,
            pageToken === undefined ? "" : pageToken,
        ),
        READ_CACHE_TTL_MS,
        () => fetchFlowPage({ pageSize, pageToken, deps }),
    );
}

/** Uncached producer behind {@link listFlowCards}. */
async function fetchFlowPage({
    pageSize,
    pageToken,
    deps,
}: Required<Pick<ListFlowCardsOptions, "pageSize">> &
    Pick<ListFlowCardsOptions, "pageToken" | "deps">): Promise<FlowCardsPage> {
    const envelope = buildEnvelope(
        listFlowCardsOp(
            create(ListFlowCardsRequestSchema, {
                pageSize,
                // The wire field is a plain string; absence is the empty token.
                pageToken: pageToken ?? "",
            }),
        ),
        newRequestId("flow"),
        null,
        false,
    );
    const response: ApiResponse = await callEnvelope(envelope, deps);
    if (response.result.case !== "flowCardPage") {
        throw new TypeError(
            `list_flow_cards returned unexpected result case ${String(response.result.case)}`,
        );
    }
    const page = response.result.value;
    return {
        requestId: envelope.requestId,
        cards: page.cards,
        cursor: cursorFromPage({
            nextPageToken:
                page.nextPageToken === "" ? undefined : page.nextPageToken,
            hasMore: page.hasMore,
        }),
    };
}

export const reviewAndAdvanceOp = (request: ReviewAndAdvanceRequest) =>
    ({ case: "reviewAndAdvance", value: request }) as const;

/** Result of a successful `review_and_advance` mutation. */
export interface ReviewAndAdvanceOutcome {
    requestId: string;
    /** Generated bigint ID of the reviewed card. */
    reviewedCardId: bigint;
    /** The next repeatable card, when the server advanced the topic slot. */
    nextCard?: ReviewAndAdvanceResponse["nextCard"];
    dailyComplete: boolean;
    pendingCount: number;
    refillScheduled: boolean;
}

export interface ReviewAndAdvanceOptions {
    cardId: bigint;
    grade: number;
    action: ReviewActionValue;
    /**
     * Caller-owned idempotency key. The caller must reuse the exact same key on
     * a retry after an outcome-indeterminate failure (so an indeterminate
     * mutation can never execute twice); after a determinate failure it may
     * allocate a fresh key.
     */
    idempotencyKey: string;
    /** Test double hook; defaults to the real transport. */
    deps?: CallDeps;
}

/**
 * Typed `review_and_advance` mutation; returns the `review_and_advance`
 * result case with generated bigint card IDs preserved.
 */
export async function reviewAndAdvance({
    cardId,
    grade,
    action,
    idempotencyKey,
    deps = {},
}: ReviewAndAdvanceOptions): Promise<ReviewAndAdvanceOutcome> {
    const { requestId, response } = await callMutationWithKeyEnvelope(
        reviewAndAdvanceOp(
            create(ReviewAndAdvanceRequestSchema, { cardId, grade, action }),
        ),
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "reviewAndAdvance") {
        throw new TypeError(
            `review_and_advance returned unexpected result case ${String(response.result.case)}`,
        );
    }
    const result = response.result.value;
    if (result.reviewedCardId !== cardId) {
        throw new TypeError(
            `review_and_advance reviewed card ${result.reviewedCardId} but card ${cardId} was requested`,
        );
    }
    invalidateReadCache();
    return {
        requestId,
        reviewedCardId: result.reviewedCardId,
        nextCard: result.nextCard === undefined ? undefined : result.nextCard,
        dailyComplete: result.dailyComplete,
        pendingCount: result.pendingCount,
        refillScheduled: result.refillScheduled,
    };
}

export const getTipcardOp = (request: GetByIdRequest) =>
    ({ case: "getTipcard", value: request }) as const;

/** Result of a successful `get_tipcard` read. */
export interface TipcardRead {
    requestId: string;
    /** The exact detail card, with generated bigint IDs preserved. */
    card: FlowCardInfo;
}

export interface GetTipcardOptions {
    cardId: bigint;
    /** Test double hook; defaults to the real transport. */
    deps?: CallDeps;
}

/**
 * Typed read of `get_tipcard`; returns the `tipcard_detail` result case.
 * The response card must exist and match the requested ID exactly.
 */
export async function getTipcard({
    cardId,
    deps = {},
}: GetTipcardOptions): Promise<TipcardRead> {
    return withReadCache(
        readCacheKey("getTipcard", cardId.toString()),
        READ_CACHE_TTL_MS,
        () => fetchTipcard(cardId, deps),
    );
}

/** Uncached producer behind {@link getTipcard}. */
async function fetchTipcard(
    cardId: bigint,
    deps: CallDeps,
): Promise<TipcardRead> {
    const envelope = buildEnvelope(
        getTipcardOp(create(GetByIdRequestSchema, { id: cardId })),
        newRequestId("detail"),
        null,
        false,
    );
    const response: ApiResponse = await callEnvelope(envelope, deps);
    if (response.result.case !== "tipcardDetail") {
        throw new TypeError(
            `get_tipcard returned unexpected result case ${String(response.result.case)}`,
        );
    }
    const detail = response.result.value;
    if (detail.card === undefined) {
        throw new TypeError(`get_tipcard returned no card for id ${cardId}`);
    }
    if (detail.card.id !== cardId) {
        throw new TypeError(
            `get_tipcard returned card ${detail.card.id} but card ${cardId} was requested`,
        );
    }
    return { requestId: envelope.requestId, card: detail.card };
}

/** Result of a successful `continue_daily_review` mutation. */
export interface ContinueDailyReviewOutcome {
    requestId: string;
    /** Eligible unseen cards remaining behind the active topic card. */
    availableCards: bigint;
    /**
     * Required positive bigint ID of the prepared active card; read it back
     * with {@link getTipcard} for the exact `FlowCardInfo`.
     */
    activeCardId: bigint;
    pendingCount: number;
}

export interface ContinueDailyReviewOptions {
    topicName: string;
    /**
     * Caller-owned idempotency key; reuse the exact same key after an
     * outcome-indeterminate failure.
     */
    idempotencyKey: string;
    /** Test double hook; defaults to the real transport. */
    deps?: CallDeps;
}

/**
 * Typed `continue_daily_review` mutation: exactly one topic, repeatable tip
 * cards only. Returns the required positive `activeCardId`; this operation
 * never reads the card itself — follow up with {@link getTipcard}.
 */
export async function continueDailyReview({
    topicName,
    idempotencyKey,
    deps = {},
}: ContinueDailyReviewOptions): Promise<ContinueDailyReviewOutcome> {
    if (topicName.trim() === "") {
        throw new TypeError("continue_daily_review requires a non-blank topic");
    }
    const { requestId, response } = await callMutationWithKeyEnvelope(
        {
            case: "continueDailyReview",
            value: create(ContinueDailyReviewRequestSchema, {
                topics: [topicName],
                tipcardType: "repeatable_tip",
            }),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "continueDailyReview") {
        throw new TypeError(
            `continue_daily_review returned unexpected result case ${String(response.result.case)}`,
        );
    }
    const result = response.result.value;
    if (result.activeCardId === undefined || result.activeCardId <= 0n) {
        throw new TypeError(
            "continue_daily_review must return a positive activeCardId",
        );
    }
    invalidateReadCache();
    return {
        requestId,
        availableCards: result.availableCards,
        activeCardId: result.activeCardId,
        pendingCount: result.pendingCount,
    };
}

export const pinTipcardOp = (request: PinTipcardRequest) =>
    ({ case: "pinTipcard", value: request }) as const;

/** Successful `pin_tipcard` mutations return only the request id. */
export interface PinTipcardOutcome {
    requestId: string;
}

export interface PinTipcardOptions {
    cardId: bigint;
    pinned: boolean;
    idempotencyKey: string;
    deps?: CallDeps;
}

/**
 * Typed `pin_tipcard` mutation. The exact bigint id is preserved on the
 * wire; non-positive ids are rejected before any fetch. Success requires
 * the exact `ok` result case.
 */
export async function pinTipcard({
    cardId,
    pinned,
    idempotencyKey,
    deps = {},
}: PinTipcardOptions): Promise<PinTipcardOutcome> {
    if (cardId <= 0n) {
        throw new TypeError(
            `pin_tipcard requires a positive card id, got ${cardId}`,
        );
    }
    const { requestId, response } = await callMutationWithKeyEnvelope(
        pinTipcardOp(create(PinTipcardRequestSchema, { id: cardId, pinned })),
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "ok") {
        throw new TypeError(
            `pin_tipcard returned unexpected result case ${String(response.result.case)}`,
        );
    }
    invalidateReadCache();
    return { requestId };
}

export const deleteTipcardOp = (request: DeleteByIdRequest) =>
    ({ case: "deleteTipcard", value: request }) as const;

/** Successful `delete_tipcard` mutations return only the request id. */
export interface DeleteTipcardOutcome {
    requestId: string;
}

export interface DeleteTipcardOptions {
    cardId: bigint;
    idempotencyKey: string;
    deps?: CallDeps;
}

/**
 * Typed `delete_tipcard` mutation. The exact bigint id is preserved on the
 * wire; non-positive ids are rejected before any fetch. Success requires
 * the exact `ok` result case. The caller owns the idempotency key so an
 * outcome-indeterminate retry can reuse it verbatim.
 */
export async function deleteTipcard({
    cardId,
    idempotencyKey,
    deps = {},
}: DeleteTipcardOptions): Promise<DeleteTipcardOutcome> {
    if (cardId <= 0n) {
        throw new TypeError(
            `delete_tipcard requires a positive card id, got ${cardId}`,
        );
    }
    const { requestId, response } = await callMutationWithKeyEnvelope(
        deleteTipcardOp(create(DeleteByIdRequestSchema, { id: cardId })),
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "ok") {
        throw new TypeError(
            `delete_tipcard returned unexpected result case ${String(response.result.case)}`,
        );
    }
    invalidateReadCache();
    return { requestId };
}

export const createTipsOp = (request: TipsRequestV1) =>
    ({ case: "tipsV1", value: request }) as const;

/** Result of a successful `tips_v1` mutation. */
export interface TipsOutcome {
    requestId: string;
    tips: TipsResponse["tips"];
}

export interface CreateTipsOptions {
    /** The generated wire request, built by pure `buildTipsRequest`. */
    request: TipsRequestV1;
    /**
     * Caller-owned idempotency key; reuse the exact same key after an
     * outcome-indeterminate failure.
     */
    idempotencyKey: string;
    /** Test double hook; defaults to the real transport. */
    deps?: CallDeps;
}

/**
 * Typed `tips_v1` mutation. Success requires the exact `tips` result case
 * and preserves the created bigint IDs verbatim.
 */
export async function createTips({
    request,
    idempotencyKey,
    deps = {},
}: CreateTipsOptions): Promise<TipsOutcome> {
    if (idempotencyKey.trim() === "") {
        throw new TypeError("tips_v1 requires a non-empty idempotency key");
    }
    // Recreate through the canonical generated schema at the boundary. This
    // keeps transport code from inventing a second request shape and rejects
    // values that are not generated TipsRequestV1 messages before any fetch.
    if (request.$typeName !== "denpie.TipsRequestV1") {
        throw new TypeError(
            "tips_v1 requires a generated TipsRequestV1 payload",
        );
    }
    const wireRequest = create(TipsRequestV1Schema, request);
    const { requestId, response } = await callMutationWithKeyEnvelope(
        createTipsOp(wireRequest),
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "tips") {
        throw new TypeError(
            `tips_v1 returned unexpected result case ${String(response.result.case)}`,
        );
    }
    invalidateReadCache();
    return {
        requestId,
        tips: response.result.value.tips,
    };
}

export interface AppendTipcardImagesOptions {
    cardId: bigint;
    imageData?: readonly string[];
    poolImageIds?: readonly bigint[];
    urls?: readonly string[];
    idempotencyKey: string;
    deps?: CallDeps;
}

/** Append one or more validated image inputs to an owned tipcard. */
export async function appendTipcardImages({
    cardId,
    imageData = [],
    poolImageIds = [],
    urls = [],
    idempotencyKey,
    deps = {},
}: AppendTipcardImagesOptions): Promise<{ requestId: string }> {
    if (cardId <= 0n) {
        throw new TypeError(
            `append_tipcard_images requires a positive card id, got ${cardId}`,
        );
    }
    if (
        imageData.length === 0 &&
        poolImageIds.length === 0 &&
        urls.every((url) => url.trim() === "")
    ) {
        throw new TypeError(
            "append_tipcard_images requires at least one image input",
        );
    }
    const { requestId, response } = await callMutationWithKeyEnvelope(
        {
            case: "appendTipcardImages",
            value: create(AppendTipcardImagesRequestSchema, {
                cardId,
                imageData: [...imageData],
                poolImageIds: [...poolImageIds],
                urls: urls.map((url) => url.trim()).filter(Boolean),
            }),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "ok") {
        throw new TypeError(
            `append_tipcard_images returned unexpected result case ${String(response.result.case)}`,
        );
    }
    invalidateReadCache();
    return { requestId };
}

export interface ReplaceTipcardImagesOptions {
    cardId: bigint;
    imageData: readonly string[];
    idempotencyKey: string;
    deps?: CallDeps;
}

/** Replace an owned tipcard's uploads. An empty list clears every image. */
export async function replaceTipcardImages({
    cardId,
    imageData,
    idempotencyKey,
    deps = {},
}: ReplaceTipcardImagesOptions): Promise<{ requestId: string }> {
    if (cardId <= 0n) {
        throw new TypeError(
            `replace_tipcard_images requires a positive card id, got ${cardId}`,
        );
    }
    const { requestId, response } = await callMutationWithKeyEnvelope(
        {
            case: "replaceTipcardImages",
            value: create(ReplaceTipcardImagesRequestSchema, {
                cardId,
                imageData: [...imageData],
            }),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "ok") {
        throw new TypeError(
            `replace_tipcard_images returned unexpected result case ${String(response.result.case)}`,
        );
    }
    invalidateReadCache();
    return { requestId };
}
