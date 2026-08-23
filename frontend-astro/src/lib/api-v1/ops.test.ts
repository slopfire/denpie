import { describe, expect, test } from "bun:test";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
    ApiErrorSchema,
    ContinueDailyReviewResponseSchema,
    ApiV1RequestSchema,
    ApiV1ResponseSchema,
    ApiResponseSchema,
    DeleteByIdRequestSchema,
    EmptySchema,
    FlowCardInfoSchema,
    FlowCardPageSchema,
    ReviewActionValue,
    ReviewAndAdvanceResponseSchema,
    TipCardResponseSchema,
    TipcardDetailSchema,
    TipsRequestV1Schema,
    TipsResponseSchema,
    TipcardTypeValue,
} from "../../generated/denpie_pb";
import type {
    AppendTipcardImagesRequest,
    ContinueDailyReviewRequest,
    DeleteByIdRequest,
    GetByIdRequest,
    ListFlowCardsRequest,
    PinTipcardRequest,
    ReviewAndAdvanceRequest,
    ReviewAndAdvanceResponse,
    ReplaceTipcardImagesRequest,
    TipsRequestV1,
} from "../../generated/denpie_pb";
import { TransportError } from "./transport";
import type { CallDeps, FetchLike } from "./transport";
import {
    appendTipcardImages,
    continueDailyReview,
    FLOW_PAGE_DEFAULT_SIZE,
    getTipcard,
    listFlowCards,
    reviewAndAdvance,
    pinTipcard,
    deleteTipcard,
    createTips,
    replaceTipcardImages,
} from "./ops";

function card(id: bigint) {
    return create(FlowCardInfoSchema, { id, title: `card-${id}` });
}

function page(init: Parameters<typeof create>[1] = {}) {
    return create(FlowCardPageSchema, init);
}

/** A decoded `list_flow_cards` request captured at the transport boundary. */
interface CapturedRequest {
    case: string;
    value: ListFlowCardsRequest;
}

/** Fetch double that records the decoded request envelope and replies. */
function fakeDeps(
    reply: () => ReturnType<typeof page> | "wrong",
): CallDeps & { requests: CapturedRequest[] } {
    const requests: CapturedRequest[] = [];
    const fetch: FetchLike = async (_input, init) => {
        // Validate the BodyInit shape instead of casting it.
        if (!(init.body instanceof ArrayBuffer)) {
            throw new TypeError(
                `expected ArrayBuffer body, got ${typeof init.body}`,
            );
        }
        const body = new Uint8Array(init.body);
        const envelope = fromBinary(ApiV1RequestSchema, body);
        // Narrow the generated op discriminant; store the value at its
        // generated type with no casts.
        if (envelope.call.op.case !== "listFlowCards") {
            throw new TypeError(
                `expected listFlowCards op, got ${String(envelope.call.op.case)}`,
            );
        }
        const request: ListFlowCardsRequest = envelope.call.op.value;
        requests.push({ case: envelope.call.op.case, value: request });
        const raw = reply();
        const apiResponse =
            raw === "wrong"
                ? create(ApiResponseSchema, {})
                : create(ApiResponseSchema, {
                      result: { case: "flowCardPage", value: raw },
                  });
        const response = create(ApiV1ResponseSchema, {
            requestId: "srv-1",
            outcome: { case: "success", value: apiResponse },
        });
        return new Response(toBinary(ApiV1ResponseSchema, response), {
            status: 200,
            headers: { "Content-Type": "application/x-protobuf" },
        });
    };
    return { fetch, requests };
}

function expectListFlowCardsRequest(
    request: CapturedRequest,
): ListFlowCardsRequest {
    expect(request.case).toBe("listFlowCards");
    return request.value;
}

describe("listFlowCards", () => {
    test("request shape: op case, default page size, empty page token", async () => {
        const deps = fakeDeps(() => page());
        await listFlowCards({ deps });
        expect(deps.requests).toHaveLength(1);
        const request = expectListFlowCardsRequest(deps.requests[0]);
        expect(request.pageSize).toBe(FLOW_PAGE_DEFAULT_SIZE);
        expect(request.pageToken).toBe("");
    });

    test("pagination token is forwarded verbatim", async () => {
        const deps = fakeDeps(() => page());
        await listFlowCards({ pageSize: 10, pageToken: "cursor-abc", deps });
        const request = expectListFlowCardsRequest(deps.requests[0]);
        expect(request.pageSize).toBe(10);
        expect(request.pageToken).toBe("cursor-abc");
    });

    test("expected result: cards, next token kept, hasMore", async () => {
        const deps = fakeDeps(() =>
            page({
                cards: [card(1n), card(2n)],
                nextPageToken: "next-1",
                hasMore: true,
            }),
        );
        const result = await listFlowCards({ pageSize: 24, deps });
        expect(result.cards.map((c) => c.id)).toEqual([1n, 2n]);
        expect(result.cursor).toEqual({ kind: "more", pageToken: "next-1" });
    });

    test("empty next token becomes undefined", async () => {
        const deps = fakeDeps(() => page({ cards: [card(3n)] }));
        const result = await listFlowCards({
            pageSize: 24,
            pageToken: "prev",
            deps,
        });
        expect(result.cursor).toEqual({ kind: "end" });
    });

    test("rejects inconsistent pagination metadata", async () => {
        const missingToken = fakeDeps(() => page({ hasMore: true }));
        await expect(listFlowCards({ deps: missingToken })).rejects.toThrow(
            /more pages without a cursor/,
        );

        const strayToken = fakeDeps(() =>
            page({ hasMore: false, nextPageToken: "stray" }),
        );
        await expect(listFlowCards({ deps: strayToken })).rejects.toThrow(
            /cursor for a final page/,
        );
    });

    test("wrong result case throws a typed error", async () => {
        await expect(
            listFlowCards({ pageSize: 24, deps: fakeDeps(() => "wrong") }),
        ).rejects.toBeInstanceOf(TypeError);
    });
});

describe("reviewAndAdvance", () => {
    interface CapturedMutation {
        case: string;
        value: ReviewAndAdvanceRequest;
        idempotencyKey: string;
    }

    /** Fetch double capturing the decoded mutation envelope and replying. */
    function fakeMutation(
        reply: () => ReturnType<typeof successResponse> | "wrong" | "error",
    ): CallDeps & { requests: CapturedMutation[] } {
        const requests: CapturedMutation[] = [];
        const fetch: FetchLike = async (_input, init) => {
            if (!(init.body instanceof ArrayBuffer)) {
                throw new TypeError(
                    `expected ArrayBuffer body, got ${typeof init.body}`,
                );
            }
            const envelope = fromBinary(
                ApiV1RequestSchema,
                new Uint8Array(init.body),
            );
            if (envelope.call.op.case !== "reviewAndAdvance") {
                throw new TypeError(
                    `expected reviewAndAdvance op, got ${String(envelope.call.op.case)}`,
                );
            }
            requests.push({
                case: envelope.call.op.case,
                value: envelope.call.op.value,
                idempotencyKey: envelope.idempotencyKey,
            });
            const raw = reply();
            const apiResponse =
                raw === "wrong"
                    ? create(ApiResponseSchema, {})
                    : raw === "error"
                      ? create(ApiResponseSchema, {
                            result: {
                                case: "error",
                                value: create(ApiErrorSchema, {
                                    message: "boom",
                                }),
                            },
                        })
                      : raw;
            const response = create(ApiV1ResponseSchema, {
                requestId: "srv-1",
                outcome: { case: "success", value: apiResponse },
            });
            return new Response(toBinary(ApiV1ResponseSchema, response), {
                status: 200,
                headers: { "Content-Type": "application/x-protobuf" },
            });
        };
        return { fetch, requests };
    }

    function successResponse(
        overrides: Partial<ReviewAndAdvanceResponse> = {},
    ) {
        return create(ApiResponseSchema, {
            result: {
                case: "reviewAndAdvance",
                value: create(ReviewAndAdvanceResponseSchema, {
                    reviewedCardId: 7n,
                    dailyComplete: false,
                    pendingCount: 3,
                    refillScheduled: true,
                    ...overrides,
                }),
            },
        });
    }

    test("request shape: bigint card id, grade, action, idempotency key", async () => {
        const deps = fakeMutation(() =>
            successResponse({ reviewedCardId: 9007199254740993n }),
        );
        await reviewAndAdvance({
            cardId: 9007199254740993n,
            grade: 4,
            action: ReviewActionValue.AGAIN,
            idempotencyKey: "key-1",
            deps,
        });
        expect(deps.requests).toHaveLength(1);
        const captured = deps.requests[0];
        expect(captured.case).toBe("reviewAndAdvance");
        expect(captured.idempotencyKey).toBe("key-1");
        expect(captured.value.cardId).toBe(9007199254740993n);
        expect(captured.value.grade).toBe(4);
        expect(captured.value.action).toBe(ReviewActionValue.AGAIN);
    });

    test("successful result maps requestId and all response fields", async () => {
        const next = card(8n);
        const deps = fakeMutation(() =>
            successResponse({
                reviewedCardId: 7n,
                nextCard: next,
                dailyComplete: true,
                pendingCount: 0,
                refillScheduled: false,
            }),
        );
        const outcome = await reviewAndAdvance({
            cardId: 7n,
            grade: 5,
            action: ReviewActionValue.LEARNED,
            idempotencyKey: "key-2",
            deps,
        });
        expect(outcome.requestId).toBeTruthy();
        expect(outcome.reviewedCardId).toBe(7n);
        expect(outcome.nextCard?.id).toBe(8n);
        expect(outcome.dailyComplete).toBe(true);
        expect(outcome.pendingCount).toBe(0);
        expect(outcome.refillScheduled).toBe(false);
    });

    test("wrong result case throws a typed error", async () => {
        const deps = fakeMutation(() => "wrong");
        await expect(
            reviewAndAdvance({
                cardId: 1n,
                grade: 3,
                action: ReviewActionValue.UNSPECIFIED,
                idempotencyKey: "key-3",
                deps,
            }),
        ).rejects.toBeInstanceOf(TypeError);
    });

    test("backend error surfaces as TransportError without retries", async () => {
        let calls = 0;
        const fetch: FetchLike = async (_input, init) => {
            calls += 1;
            if (!(init.body instanceof ArrayBuffer))
                throw new TypeError("body");
            const response = create(ApiV1ResponseSchema, {
                requestId: "srv-err",
                outcome: {
                    case: "error",
                    value: create(ApiErrorSchema, {
                        code: 13,
                        message: "internal failure",
                        retryable: false,
                    }),
                },
            });
            return new Response(toBinary(ApiV1ResponseSchema, response), {
                status: 200,
                headers: { "Content-Type": "application/x-protobuf" },
            });
        };
        await expect(
            reviewAndAdvance({
                cardId: 1n,
                grade: 3,
                action: ReviewActionValue.UNSPECIFIED,
                idempotencyKey: "key-4",
                deps: { fetch },
            }),
        ).rejects.toThrow(/internal failure/);
        expect(calls).toBe(1);
    });

    test("mismatched reviewedCardId throws", async () => {
        const deps = fakeMutation(() =>
            successResponse({ reviewedCardId: 99n }),
        );
        await expect(
            reviewAndAdvance({
                cardId: 7n,
                grade: 3,
                action: ReviewActionValue.UNSPECIFIED,
                idempotencyKey: "key-5",
                deps,
            }),
        ).rejects.toThrow(/reviewed card 99/);
    });
});

function createApiResponse(init?: Parameters<typeof create>[1]) {
    return create(ApiResponseSchema, init);
}

type SuccessReply = () => ReturnType<typeof createApiResponse>;

/** Fetch double that records the decoded `get_tipcard` request and replies. */
function fakeDetail(
    reply: SuccessReply,
): CallDeps & { requests: { value: GetByIdRequest }[] } {
    const requests: { value: GetByIdRequest }[] = [];
    const fetch: FetchLike = async (_input, init) => {
        if (!(init.body instanceof ArrayBuffer)) {
            throw new TypeError(
                `expected ArrayBuffer body, got ${typeof init.body}`,
            );
        }
        const envelope = fromBinary(
            ApiV1RequestSchema,
            new Uint8Array(init.body),
        );
        if (envelope.call.op.case !== "getTipcard") {
            throw new TypeError(
                `expected getTipcard op, got ${String(envelope.call.op.case)}`,
            );
        }
        const request: GetByIdRequest = envelope.call.op.value;
        requests.push({ value: request });
        const response = create(ApiV1ResponseSchema, {
            requestId: "srv-1",
            outcome: { case: "success", value: reply() },
        });
        return new Response(toBinary(ApiV1ResponseSchema, response), {
            status: 200,
            headers: { "Content-Type": "application/x-protobuf" },
        });
    };
    return { fetch, requests };
}

/** Fetch double that records the decoded continue mutation and its key. */
function fakeContinue(reply: SuccessReply): CallDeps & {
    requests: { value: ContinueDailyReviewRequest; idempotencyKey: string }[];
} {
    const requests: {
        value: ContinueDailyReviewRequest;
        idempotencyKey: string;
    }[] = [];
    const fetch: FetchLike = async (_input, init) => {
        if (!(init.body instanceof ArrayBuffer)) {
            throw new TypeError(
                `expected ArrayBuffer body, got ${typeof init.body}`,
            );
        }
        const envelope = fromBinary(
            ApiV1RequestSchema,
            new Uint8Array(init.body),
        );
        if (envelope.call.op.case !== "continueDailyReview") {
            throw new TypeError(
                `expected continueDailyReview op, got ${String(envelope.call.op.case)}`,
            );
        }
        const request: ContinueDailyReviewRequest = envelope.call.op.value;
        requests.push({
            value: request,
            idempotencyKey: envelope.idempotencyKey,
        });
        const response = create(ApiV1ResponseSchema, {
            requestId: "srv-1",
            outcome: { case: "success", value: reply() },
        });
        return new Response(toBinary(ApiV1ResponseSchema, response), {
            status: 200,
            headers: { "Content-Type": "application/x-protobuf" },
        });
    };
    return { fetch, requests };
}

describe("getTipcard", () => {
    function tipcardReply(
        detailCard:
            | Parameters<typeof create<typeof FlowCardInfoSchema>>[1]
            | undefined
            | "wrong",
    ) {
        return () =>
            detailCard === "wrong"
                ? createApiResponse({})
                : createApiResponse({
                      result: {
                          case: "tipcardDetail",
                          value: create(TipcardDetailSchema, {
                              card: detailCard,
                          }),
                      },
                  });
    }

    test("request shape: GetByIdRequest with exact bigint card id", async () => {
        const deps = fakeDetail(tipcardReply({ id: 9007199254740993n }));
        await getTipcard({ cardId: 9007199254740993n, deps });
        expect(deps.requests).toHaveLength(1);
        expect(deps.requests[0].value.id).toBe(9007199254740993n);
    });

    test("success maps requestId and the exact detail card", async () => {
        const deps = fakeDetail(
            tipcardReply({ id: 7n, title: "detail", pinned: true }),
        );
        const read = await getTipcard({ cardId: 7n, deps });
        expect(read.requestId).toBeTruthy();
        expect(read.card.id).toBe(7n);
        expect(read.card.title).toBe("detail");
        expect(read.card.pinned).toBe(true);
    });

    test("missing detail card throws", async () => {
        const deps = fakeDetail(tipcardReply(undefined));
        await expect(getTipcard({ cardId: 7n, deps })).rejects.toThrow(
            /no card/,
        );
    });

    test("mismatched detail card id throws", async () => {
        const deps = fakeDetail(tipcardReply({ id: 99n }));
        await expect(getTipcard({ cardId: 7n, deps })).rejects.toThrow(
            /returned card 99/,
        );
    });

    test("wrong result case throws a typed error", async () => {
        const deps = fakeDetail(tipcardReply("wrong"));
        await expect(getTipcard({ cardId: 7n, deps })).rejects.toBeInstanceOf(
            TypeError,
        );
    });
});

describe("continueDailyReview", () => {
    function continueReply(init?: {
        availableCards?: bigint;
        activeCardId?: bigint;
        pendingCount?: number;
    }) {
        return () =>
            createApiResponse({
                result: {
                    case: "continueDailyReview",
                    value: create(ContinueDailyReviewResponseSchema, init),
                },
            });
    }

    test("request shape: exactly one topic, repeatable_tip, caller-owned key", async () => {
        const deps = fakeContinue(
            continueReply({
                availableCards: 2n,
                activeCardId: 7n,
                pendingCount: 1,
            }),
        );
        await continueDailyReview({
            topicName: "rust",
            idempotencyKey: "key-continue-1",
            deps,
        });
        expect(deps.requests).toHaveLength(1);
        expect(deps.requests[0].idempotencyKey).toBe("key-continue-1");
        const request = deps.requests[0].value;
        expect(request.topics).toEqual(["rust"]);
        expect(request.tipcardType).toBe("repeatable_tip");
    });

    test("blank topic is rejected before any fetch", async () => {
        for (const topicName of ["", "   "]) {
            let calls = 0;
            const fetch: FetchLike = async () => {
                calls += 1;
                throw new Error("must not fetch");
            };
            await expect(
                continueDailyReview({
                    topicName,
                    idempotencyKey: "k",
                    deps: { fetch },
                }),
            ).rejects.toThrow(/non-blank topic/);
            expect(calls).toBe(0);
        }
    });

    test("bigints are preserved in the success mapping", async () => {
        const deps = fakeContinue(
            continueReply({
                availableCards: 9007199254740993n,
                activeCardId: 77n,
                pendingCount: 4,
            }),
        );
        const outcome = await continueDailyReview({
            topicName: "rust",
            idempotencyKey: "key-c2",
            deps,
        });
        expect(outcome.requestId).toBeTruthy();
        expect(outcome.availableCards).toBe(9007199254740993n);
        expect(outcome.activeCardId).toBe(77n);
        expect(outcome.pendingCount).toBe(4);
    });

    test("missing activeCardId throws", async () => {
        const deps = fakeContinue(continueReply({ availableCards: 1n }));
        await expect(
            continueDailyReview({ topicName: "t", idempotencyKey: "k", deps }),
        ).rejects.toThrow(/positive activeCardId/);
    });

    test("non-positive activeCardId throws", async () => {
        const deps = fakeContinue(
            continueReply({ availableCards: 1n, activeCardId: 0n }),
        );
        await expect(
            continueDailyReview({ topicName: "t", idempotencyKey: "k", deps }),
        ).rejects.toThrow(/positive activeCardId/);
    });

    test("wrong result case throws a typed error", async () => {
        const deps = fakeContinue(() => createApiResponse({}));
        await expect(
            continueDailyReview({ topicName: "t", idempotencyKey: "k", deps }),
        ).rejects.toBeInstanceOf(TypeError);
    });

    test("backend error surfaces without retries and never reads the detail", async () => {
        let calls = 0;
        const fetch: FetchLike = async (_input, init) => {
            calls += 1;
            if (!(init.body instanceof ArrayBuffer))
                throw new TypeError("body");
            const envelope = fromBinary(
                ApiV1RequestSchema,
                new Uint8Array(init.body),
            );
            expect(envelope.call.op.case).toBe("continueDailyReview");
            const response = create(ApiV1ResponseSchema, {
                requestId: "srv-e",
                outcome: {
                    case: "error",
                    value: create(ApiErrorSchema, {
                        code: 13,
                        message: "no cards left",
                        retryable: false,
                    }),
                },
            });
            return new Response(toBinary(ApiV1ResponseSchema, response), {
                status: 200,
                headers: { "Content-Type": "application/x-protobuf" },
            });
        };
        await expect(
            continueDailyReview({
                topicName: "t",
                idempotencyKey: "k",
                deps: { fetch },
            }),
        ).rejects.toThrow(/no cards left/);
        expect(calls).toBe(1);
    });
});

/** Fetch double that records the decoded pin mutation and its key. */
function fakePin(reply: SuccessReply): CallDeps & {
    requests: { value: PinTipcardRequest; idempotencyKey: string }[];
} {
    const requests: {
        value: PinTipcardRequest;
        idempotencyKey: string;
    }[] = [];
    const fetch: FetchLike = async (_input, init) => {
        if (!(init.body instanceof ArrayBuffer)) {
            throw new TypeError(
                `expected ArrayBuffer body, got ${typeof init.body}`,
            );
        }
        const envelope = fromBinary(
            ApiV1RequestSchema,
            new Uint8Array(init.body),
        );
        if (envelope.call.op.case !== "pinTipcard") {
            throw new TypeError(
                `expected pinTipcard op, got ${String(envelope.call.op.case)}`,
            );
        }
        const request: PinTipcardRequest = envelope.call.op.value;
        requests.push({
            value: request,
            idempotencyKey: envelope.idempotencyKey,
        });
        const response = create(ApiV1ResponseSchema, {
            requestId: "srv-1",
            outcome: { case: "success", value: reply() },
        });
        return new Response(toBinary(ApiV1ResponseSchema, response), {
            status: 200,
            headers: { "Content-Type": "application/x-protobuf" },
        });
    };
    return { fetch, requests };
}

describe("pinTipcard", () => {
    function pinReply() {
        return () =>
            createApiResponse({
                result: { case: "ok", value: create(EmptySchema, {}) },
            });
    }

    test("request shape: PinTipcardRequest with exact bigint id and caller key", async () => {
        const deps = fakePin(pinReply());
        await pinTipcard({
            cardId: 9007199254740993n,
            pinned: true,
            idempotencyKey: "key-pin-1",
            deps,
        });
        expect(deps.requests).toHaveLength(1);
        expect(deps.requests[0].idempotencyKey).toBe("key-pin-1");
        const request = deps.requests[0].value;
        expect(request.id).toBe(9007199254740993n);
        expect(request.pinned).toBe(true);
    });

    test("exact ok result case succeeds and maps requestId", async () => {
        const deps = fakePin(pinReply());
        const outcome = await pinTipcard({
            cardId: 7n,
            pinned: false,
            idempotencyKey: "key-pin-2",
            deps,
        });
        expect(outcome.requestId).toMatch(/^mut-/);
    });

    test("wrong result case throws a typed error", async () => {
        const deps = fakePin(() => createApiResponse({}));
        await expect(
            pinTipcard({ cardId: 7n, pinned: true, idempotencyKey: "k", deps }),
        ).rejects.toBeInstanceOf(TypeError);
    });

    test("non-positive ids are rejected before any fetch", async () => {
        for (const cardId of [0n, -5n]) {
            let calls = 0;
            const fetch: FetchLike = async () => {
                calls += 1;
                throw new Error("must not fetch");
            };
            await expect(
                pinTipcard({
                    cardId,
                    pinned: true,
                    idempotencyKey: "k",
                    deps: { fetch },
                }),
            ).rejects.toThrow(/positive card id/);
            expect(calls).toBe(0);
        }
    });

    test("backend error surfaces as TransportError without retries", async () => {
        let calls = 0;
        const fetch: FetchLike = async (_input, init) => {
            calls += 1;
            if (!(init.body instanceof ArrayBuffer))
                throw new TypeError("body");
            const envelope = fromBinary(
                ApiV1RequestSchema,
                new Uint8Array(init.body),
            );
            expect(envelope.call.op.case).toBe("pinTipcard");
            const response = create(ApiV1ResponseSchema, {
                requestId: "srv-e",
                outcome: {
                    case: "error",
                    value: create(ApiErrorSchema, {
                        code: 13,
                        message: "pin rejected",
                        retryable: false,
                    }),
                },
            });
            return new Response(toBinary(ApiV1ResponseSchema, response), {
                status: 200,
                headers: { "Content-Type": "application/x-protobuf" },
            });
        };
        await expect(
            pinTipcard({
                cardId: 7n,
                pinned: true,
                idempotencyKey: "k",
                deps: { fetch },
            }),
        ).rejects.toBeInstanceOf(TransportError);
        expect(calls).toBe(1);
    });
});

/** Fetch double that records the decoded delete mutation and its key. */
function fakeDelete(reply: SuccessReply): CallDeps & {
    requests: { value: DeleteByIdRequest; idempotencyKey: string }[];
} {
    const requests: { value: DeleteByIdRequest; idempotencyKey: string }[] = [];
    const fetch: FetchLike = async (_input, init) => {
        if (!(init.body instanceof ArrayBuffer)) {
            throw new TypeError(
                `expected ArrayBuffer body, got ${typeof init.body}`,
            );
        }
        const envelope = fromBinary(
            ApiV1RequestSchema,
            new Uint8Array(init.body),
        );
        if (envelope.call.op.case !== "deleteTipcard") {
            throw new TypeError(
                `expected deleteTipcard op, got ${String(envelope.call.op.case)}`,
            );
        }
        const request: DeleteByIdRequest = envelope.call.op.value;
        requests.push({
            value: request,
            idempotencyKey: envelope.idempotencyKey,
        });
        const response = create(ApiV1ResponseSchema, {
            requestId: "srv-del",
            outcome: { case: "success", value: reply() },
        });
        return new Response(toBinary(ApiV1ResponseSchema, response), {
            status: 200,
            headers: { "Content-Type": "application/x-protobuf" },
        });
    };
    return { fetch, requests };
}

describe("deleteTipcard", () => {
    function okReply() {
        return () =>
            createApiResponse({
                result: { case: "ok", value: create(EmptySchema, {}) },
            });
    }

    test("request shape: DeleteByIdRequest with the exact bigint id and caller key", async () => {
        const deps = fakeDelete(okReply());
        await deleteTipcard({
            cardId: 9007199254740993n,
            idempotencyKey: "key-del-1",
            deps,
        });
        expect(deps.requests).toHaveLength(1);
        expect(deps.requests[0].idempotencyKey).toBe("key-del-1");
        expect(deps.requests[0].value.id).toBe(9007199254740993n);
    });

    test("exact ok result case succeeds and maps requestId", async () => {
        const deps = fakeDelete(okReply());
        const outcome = await deleteTipcard({
            cardId: 7n,
            idempotencyKey: "key-del-2",
            deps,
        });
        expect(outcome.requestId).toMatch(/^mut-/);
    });

    test("wrong result case throws a typed error", async () => {
        const deps = fakeDelete(() => createApiResponse({}));
        await expect(
            deleteTipcard({ cardId: 7n, idempotencyKey: "k", deps }),
        ).rejects.toBeInstanceOf(TypeError);
    });

    test("non-positive ids are rejected before any fetch", async () => {
        for (const cardId of [0n, -5n]) {
            let calls = 0;
            const fetch: FetchLike = async () => {
                calls += 1;
                throw new Error("must not fetch");
            };
            await expect(
                deleteTipcard({ cardId, idempotencyKey: "k", deps: { fetch } }),
            ).rejects.toThrow(/positive card id/);
            expect(calls).toBe(0);
        }
    });

    test("backend error surfaces as TransportError without retries", async () => {
        let calls = 0;
        const fetch: FetchLike = async (_input, init) => {
            calls += 1;
            if (!(init.body instanceof ArrayBuffer))
                throw new TypeError("body");
            const envelope = fromBinary(
                ApiV1RequestSchema,
                new Uint8Array(init.body),
            );
            expect(envelope.call.op.case).toBe("deleteTipcard");
            const response = create(ApiV1ResponseSchema, {
                requestId: "srv-e",
                outcome: {
                    case: "error",
                    value: create(ApiErrorSchema, {
                        code: 13,
                        message: "delete rejected",
                        retryable: false,
                    }),
                },
            });
            return new Response(toBinary(ApiV1ResponseSchema, response), {
                status: 200,
                headers: { "Content-Type": "application/x-protobuf" },
            });
        };
        await expect(
            deleteTipcard({ cardId: 7n, idempotencyKey: "k", deps: { fetch } }),
        ).rejects.toBeInstanceOf(TransportError);
        expect(calls).toBe(1);
    });
});

describe("createTips", () => {
    interface CapturedTipsMutation {
        request: TipsRequestV1;
        idempotencyKey: string;
    }

    function fakeTips(
        reply: () => ReturnType<typeof createApiResponse>,
    ): CallDeps & { requests: CapturedTipsMutation[] } {
        const requests: CapturedTipsMutation[] = [];
        const fetch: FetchLike = async (_input, init) => {
            if (!(init.body instanceof ArrayBuffer)) {
                throw new TypeError(
                    `expected ArrayBuffer body, got ${typeof init.body}`,
                );
            }
            const envelope = fromBinary(
                ApiV1RequestSchema,
                new Uint8Array(init.body),
            );
            if (envelope.call.op.case !== "tipsV1") {
                throw new TypeError(
                    `expected tipsV1 op, got ${String(envelope.call.op.case)}`,
                );
            }
            requests.push({
                request: envelope.call.op.value,
                idempotencyKey: envelope.idempotencyKey,
            });
            const response = create(ApiV1ResponseSchema, {
                requestId: "srv-tips",
                outcome: { case: "success", value: reply() },
            });
            return new Response(toBinary(ApiV1ResponseSchema, response), {
                status: 200,
                headers: { "Content-Type": "application/x-protobuf" },
            });
        };
        return { fetch, requests };
    }

    function request(): TipsRequestV1 {
        return create(TipsRequestV1Schema, {
            count: 5,
            topics: ["Rust", "Rust", "Systems"],
            tipcardType: TipcardTypeValue.REPEATABLE,
            excludeCardIds: [],
            manualContent: "",
            manualCompressedContent: "",
            manualImageData: [],
        });
    }

    test("sends the generated request with the caller-owned key", async () => {
        const cardId = 9007199254740993n;
        const deps = fakeTips(() =>
            createApiResponse({
                result: {
                    case: "tips",
                    value: create(TipsResponseSchema, {
                        tips: [
                            create(TipCardResponseSchema, {
                                id: cardId,
                                topic: "Rust",
                                tipcardType: "repeatable_tip",
                                pinned: false,
                            }),
                        ],
                    }),
                },
            }),
        );
        const outcome = await createTips({
            request: request(),
            idempotencyKey: "tips-key-1",
            deps,
        });

        expect(deps.requests).toHaveLength(1);
        expect(deps.requests[0]?.idempotencyKey).toBe("tips-key-1");
        expect(deps.requests[0]?.request.topics).toEqual([
            "Rust",
            "Rust",
            "Systems",
        ]);
        expect(deps.requests[0]?.request.count).toBe(5);
        expect(outcome.requestId).toMatch(/^mut-/);
        expect(outcome.tips[0]?.id).toBe(cardId);
    });

    test("requires the exact generated tips result case", async () => {
        const deps = fakeTips(() => createApiResponse({}));
        await expect(
            createTips({
                request: request(),
                idempotencyKey: "tips-key-2",
                deps,
            }),
        ).rejects.toBeInstanceOf(TypeError);
    });

    test("rejects a blank caller key before any fetch", async () => {
        let calls = 0;
        const fetch: FetchLike = async () => {
            calls += 1;
            throw new Error("must not fetch");
        };
        await expect(
            createTips({
                request: request(),
                idempotencyKey: "  ",
                deps: { fetch },
            }),
        ).rejects.toThrow(/non-empty idempotency key/);
        expect(calls).toBe(0);
    });
});

describe("tipcard image mutations", () => {
    test("append and replace preserve every typed source and caller key", async () => {
        const captured: Array<
            | {
                  case: "appendTipcardImages";
                  value: AppendTipcardImagesRequest;
                  key: string;
              }
            | {
                  case: "replaceTipcardImages";
                  value: ReplaceTipcardImagesRequest;
                  key: string;
              }
        > = [];
        const fetch: FetchLike = async (_input, init) => {
            if (!(init.body instanceof ArrayBuffer)) {
                throw new TypeError("expected ArrayBuffer body");
            }
            const envelope = fromBinary(
                ApiV1RequestSchema,
                new Uint8Array(init.body),
            );
            if (envelope.call.op.case === "appendTipcardImages") {
                captured.push({
                    case: envelope.call.op.case,
                    value: envelope.call.op.value,
                    key: envelope.idempotencyKey,
                });
            } else if (envelope.call.op.case === "replaceTipcardImages") {
                captured.push({
                    case: envelope.call.op.case,
                    value: envelope.call.op.value,
                    key: envelope.idempotencyKey,
                });
            } else {
                throw new TypeError(
                    `unexpected operation ${String(envelope.call.op.case)}`,
                );
            }
            const response = create(ApiV1ResponseSchema, {
                requestId: "srv-images",
                outcome: {
                    case: "success",
                    value: create(ApiResponseSchema, {
                        result: {
                            case: "ok",
                            value: create(EmptySchema, {}),
                        },
                    }),
                },
            });
            return new Response(toBinary(ApiV1ResponseSchema, response), {
                status: 200,
                headers: { "Content-Type": "application/x-protobuf" },
            });
        };

        await appendTipcardImages({
            cardId: 9007199254740993n,
            imageData: ["data:image/png;base64,AA"],
            poolImageIds: [8n, 13n],
            urls: ["https://example.com/card.png"],
            idempotencyKey: "append-images-1",
            deps: { fetch },
        });
        await replaceTipcardImages({
            cardId: 9007199254740993n,
            imageData: [],
            idempotencyKey: "clear-images-1",
            deps: { fetch },
        });

        expect(captured).toHaveLength(2);
        const append = captured[0];
        expect(append?.case).toBe("appendTipcardImages");
        if (append?.case !== "appendTipcardImages") throw new Error("append");
        expect(append.key).toBe("append-images-1");
        expect(append.value.cardId).toBe(9007199254740993n);
        expect(append.value.imageData).toEqual(["data:image/png;base64,AA"]);
        expect(append.value.poolImageIds).toEqual([8n, 13n]);
        expect(append.value.urls).toEqual(["https://example.com/card.png"]);
        const replace = captured[1];
        expect(replace?.case).toBe("replaceTipcardImages");
        if (replace?.case !== "replaceTipcardImages")
            throw new Error("replace");
        expect(replace.key).toBe("clear-images-1");
        expect(replace.value.imageData).toEqual([]);
    });
});
