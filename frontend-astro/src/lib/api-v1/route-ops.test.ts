import { describe, expect, test } from "bun:test";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
    ApiResponseSchema,
    ApiV1RequestSchema,
    ApiV1ResponseSchema,
    ApiKeyCreatedSchema,
    AppSummarySchema,
    DocumentDetailSchema,
    EmptySchema,
    EnhancePromptTemplateResultSchema,
    SettingsSchema,
    TipcardInfoSchema,
    TipcardsSchema,
    UpdateTopicRequestSchema,
} from "../../generated/denpie_pb";
import type { ApiResponse } from "../../generated/denpie_pb";
import type { CallDeps, FetchLike } from "./transport";
import {
    createApiKey,
    createDocument,
    deleteApiKey,
    enhancePromptTemplate,
    getSettings,
    getSummary,
    listTipcards,
    updateTopic,
} from "./route-ops";

function success(result?: ApiResponse["result"]): Response {
    const envelope = create(ApiV1ResponseSchema, {
        requestId: "server-request",
        outcome: {
            case: "success",
            value: create(
                ApiResponseSchema,
                result === undefined ? {} : { result },
            ),
        },
    });
    return new Response(toBinary(ApiV1ResponseSchema, envelope), {
        status: 200,
        headers: { "Content-Type": "application/x-protobuf" },
    });
}

function fakeDeps(reply: () => Response): CallDeps & {
    requests: ReturnType<typeof fromBinary<typeof ApiV1RequestSchema>>[];
} {
    const requests: ReturnType<typeof fromBinary<typeof ApiV1RequestSchema>>[] =
        [];
    const fetch: FetchLike = async (_input, init) => {
        if (!(init.body instanceof ArrayBuffer)) {
            throw new TypeError("expected ArrayBuffer protobuf body");
        }
        requests.push(
            fromBinary(ApiV1RequestSchema, new Uint8Array(init.body)),
        );
        return reply();
    };
    return { fetch, requests };
}

describe("route operation wrappers", () => {
    test("enhancePromptTemplate sends the topic id and maps the suggestion", async () => {
        const deps = fakeDeps(() =>
            success({
                case: "enhancePromptTemplate",
                value: create(EnhancePromptTemplateResultSchema, {
                    promptTemplate: "Write useful daily tip cards about {topic}.",
                    groundingStrategy: "agentic",
                    rationale: "too many skip titles",
                }),
            }),
        );

        const result = await enhancePromptTemplate({
            topicId: 11n,
            deps,
        });

        expect(deps.requests[0]!.call.op.case).toBe("enhancePromptTemplate");
        if (deps.requests[0]!.call.op.case === "enhancePromptTemplate") {
            expect(deps.requests[0]!.call.op.value.topicId).toBe(11n);
        }
        expect(result.suggestion.groundingStrategy).toBe("agentic");
        expect(result.suggestion.rationale).toBe("too many skip titles");
        await expect(
            enhancePromptTemplate({
                deps: fakeDeps(() => success()),
            }),
        ).rejects.toThrow(
            /enhance_prompt_template returned unexpected result case/,
        );
    });

    test("getSettings decodes the exact result and preserves uint64 bigint", async () => {
        const maxActiveCards = 9_007_199_254_740_993n;
        const deps = fakeDeps(() =>
            success({
                case: "settings",
                value: create(SettingsSchema, { maxActiveCards }),
            }),
        );

        const result = await getSettings({ deps });

        expect(deps.requests[0]!.call.op.case).toBe("getSettings");
        expect(result.settings.maxActiveCards).toBe(maxActiveCards);
    });

    test("createApiKey sends caller-owned idempotency and maps the one-time secret", async () => {
        const deps = fakeDeps(() =>
            success({
                case: "apiKeyCreated",
                value: create(ApiKeyCreatedSchema, { apiKey: "secret-once" }),
            }),
        );

        const result = await createApiKey({
            clientName: "desktop-widget",
            idempotencyKey: "key-create-1",
            deps,
        });

        const request = deps.requests[0];
        expect(request!.call.op.case).toBe("createApiKey");
        if (request!.call.op.case === "createApiKey") {
            expect(request!.call.op.value.clientName).toBe("desktop-widget");
        }
        expect(request?.idempotencyKey).toBe("key-create-1");
        expect(result.apiKey).toBe("secret-once");
    });

    test("listTipcards keeps bigint card IDs and rejects a wrong result case", async () => {
        const cardId = 9_007_199_254_740_993n;
        const card = create(TipcardInfoSchema, {
            id: cardId,
            title: "Archive card",
        });
        const deps = fakeDeps(() =>
            success({
                case: "tipcards",
                value: create(TipcardsSchema, { cards: [card] }),
            }),
        );

        const result = await listTipcards({ deps });

        expect(result.cards[0]?.id).toBe(cardId);
        await expect(
            listTipcards({ deps: fakeDeps(() => success()) }),
        ).rejects.toThrow(/list_tipcards returned unexpected result case/);
    });

    test("getSummary maps the generated summary message", async () => {
        const deps = fakeDeps(() =>
            success({
                case: "summary",
                value: create(AppSummarySchema, {
                    topics: 4n,
                    activeCards: 12n,
                }),
            }),
        );

        const result = await getSummary({ deps });

        expect(result.summary.topics).toBe(4n);
        expect(result.summary.activeCards).toBe(12n);
    });

    test("updateTopic encodes generated bigint patch and requires the mutation key", async () => {
        const topicId = 9_007_199_254_740_993n;
        const patch = create(UpdateTopicRequestSchema, {
            id: topicId,
            dailyCardCount: 5,
        });
        const deps = fakeDeps(() =>
            success({ case: "ok", value: create(EmptySchema, {}) }),
        );

        const result = await updateTopic({
            patch,
            idempotencyKey: "topic-update-1",
            deps,
        });

        const request = deps.requests[0];
        expect(request!.call.op.case).toBe("updateTopic");
        if (request!.call.op.case === "updateTopic") {
            expect(request!.call.op.value.id).toBe(topicId);
            expect(request!.call.op.value.dailyCardCount).toBe(5);
        }
        expect(result.requestId).toMatch(/^mut-/);
        await expect(
            deleteApiKey({ id: 0n, idempotencyKey: "bad", deps }),
        ).rejects.toThrow(/positive id/);
    });

    test("createDocument accepts the documented documentCreated result", async () => {
        const documentId = 77n;
        const document = create(DocumentDetailSchema, {
            id: documentId,
            title: "A source",
            topicIds: [3n],
        });
        const deps = fakeDeps(() =>
            success({ case: "documentCreated", value: document }),
        );

        const result = await createDocument({
            request: {
                $typeName: "denpie.AddDocumentRequest",
                topicIdOpt: "",
                sourceType: "document",
                title: "A source",
                url: "",
                content: "body",
                topicIds: [3n],
            },
            idempotencyKey: "document-create-1",
            deps,
        });

        expect(result.document.id).toBe(documentId);
        expect(deps.requests[0]!.call.op.case).toBe("createDocument");
    });
});
