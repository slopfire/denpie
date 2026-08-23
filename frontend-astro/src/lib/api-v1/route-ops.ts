// Typed /api/v1 operations used by the non-Flow route surfaces. This module
// intentionally stays separate from the Flow operation layer so route work can
// grow without changing Flow's request contracts.

import { create } from "@bufbuild/protobuf";
import {
    AddDocumentRequestSchema,
    AddPoolImageRequestSchema,
    AttachDocumentTopicRequestSchema,
    CreateApiKeyRequestSchema,
    DeleteByIdRequestSchema,
    EmptySchema,
    ExploreLinkRequestSchema,
    ForceDailyRefreshRequestSchema,
    GetByIdRequestSchema,
    UpdateSettingsRequestSchema,
    UpdateTopicRequestSchema,
    UploadDocumentRequestSchema,
} from "../../generated/denpie_pb";
import type {
    AddDocumentRequest,
    AddPoolImageRequest,
    ApiRequest,
    ApiKeyInfo,
    ApiResponse,
    AppSummary,
    AppTopicInfo,
    DocumentDetail,
    Documents,
    ForceDailyRefreshResponse,
    ExploredLinks,
    PoolImageCreated,
    PoolImageInfo,
    Settings,
    TipcardInfo,
    UpdateSettingsRequest,
    UpdateTopicRequest,
    UploadDocumentRequest,
    VisionModelTest,
} from "../../generated/denpie_pb";
import type { CallDeps } from "./transport";
import {
    buildEnvelope,
    callEnvelope,
    callMutationWithKeyEnvelope,
    newRequestId,
} from "./transport";

type CallResult = { requestId: string; response: ApiResponse };

async function readOperation(
    op: ApiRequest["op"],
    prefix: string,
    deps: CallDeps,
): Promise<CallResult> {
    const envelope = buildEnvelope(op, newRequestId(prefix), null, false);
    return {
        requestId: envelope.requestId,
        response: await callEnvelope(envelope, deps),
    };
}

async function mutateOperation(
    op: ApiRequest["op"],
    idempotencyKey: string,
    deps: CallDeps,
): Promise<CallResult> {
    return callMutationWithKeyEnvelope(op, idempotencyKey, deps);
}

function unexpectedResult(operation: string, response: ApiResponse): TypeError {
    return new TypeError(
        `${operation} returned unexpected result case ${String(response.result.case)}`,
    );
}

function requirePositiveId(id: bigint, operation: string): void {
    if (id <= 0n)
        throw new TypeError(`${operation} requires a positive id, got ${id}`);
}

export interface ReadDeps {
    deps?: CallDeps;
}

export async function getSettings({ deps = {} }: ReadDeps = {}): Promise<{
    requestId: string;
    settings: Settings;
}> {
    const { requestId, response } = await readOperation(
        { case: "getSettings", value: create(EmptySchema, {}) },
        "settings",
        deps,
    );
    if (response.result.case !== "settings") {
        throw unexpectedResult("get_settings", response);
    }
    return { requestId, settings: response.result.value };
}

export interface UpdateSettingsOptions {
    patch: UpdateSettingsRequest;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function updateSettings({
    patch,
    idempotencyKey,
    deps = {},
}: UpdateSettingsOptions): Promise<{ requestId: string }> {
    const { requestId, response } = await mutateOperation(
        {
            case: "updateSettings",
            value: create(UpdateSettingsRequestSchema, patch),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "ok") {
        throw unexpectedResult("update_settings", response);
    }
    return { requestId };
}

export async function listApiKeys({ deps = {} }: ReadDeps = {}): Promise<{
    requestId: string;
    keys: ApiKeyInfo[];
}> {
    const { requestId, response } = await readOperation(
        { case: "listApiKeys", value: create(EmptySchema, {}) },
        "keys",
        deps,
    );
    if (response.result.case !== "apiKeys") {
        throw unexpectedResult("list_api_keys", response);
    }
    return { requestId, keys: response.result.value.keys };
}

export interface CreateApiKeyOptions {
    clientName?: string;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function createApiKey({
    clientName = "",
    idempotencyKey,
    deps = {},
}: CreateApiKeyOptions): Promise<{ requestId: string; apiKey: string }> {
    const { requestId, response } = await mutateOperation(
        {
            case: "createApiKey",
            value: create(CreateApiKeyRequestSchema, { clientName }),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "apiKeyCreated") {
        throw unexpectedResult("create_api_key", response);
    }
    return { requestId, apiKey: response.result.value.apiKey };
}

export interface DeleteApiKeyOptions {
    id: bigint;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function deleteApiKey({
    id,
    idempotencyKey,
    deps = {},
}: DeleteApiKeyOptions): Promise<{ requestId: string }> {
    requirePositiveId(id, "delete_api_key");
    const { requestId, response } = await mutateOperation(
        {
            case: "deleteApiKey",
            value: create(DeleteByIdRequestSchema, { id }),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "ok") {
        throw unexpectedResult("delete_api_key", response);
    }
    return { requestId };
}

export async function listTipcards({ deps = {} }: ReadDeps = {}): Promise<{
    requestId: string;
    cards: TipcardInfo[];
}> {
    const { requestId, response } = await readOperation(
        { case: "listTipcards", value: create(EmptySchema, {}) },
        "tipcards",
        deps,
    );
    if (response.result.case !== "tipcards") {
        throw unexpectedResult("list_tipcards", response);
    }
    return { requestId, cards: response.result.value.cards };
}

export async function getSummary({ deps = {} }: ReadDeps = {}): Promise<{
    requestId: string;
    summary: AppSummary;
}> {
    const { requestId, response } = await readOperation(
        { case: "getSummary", value: create(EmptySchema, {}) },
        "summary",
        deps,
    );
    if (response.result.case !== "summary") {
        throw unexpectedResult("get_summary", response);
    }
    return { requestId, summary: response.result.value };
}

export async function listAppTopics({ deps = {} }: ReadDeps = {}): Promise<{
    requestId: string;
    topics: AppTopicInfo[];
}> {
    const { requestId, response } = await readOperation(
        { case: "listAppTopics", value: create(EmptySchema, {}) },
        "topics",
        deps,
    );
    if (response.result.case !== "appTopics") {
        throw unexpectedResult("list_app_topics", response);
    }
    return { requestId, topics: response.result.value.topics };
}

export interface UpdateTopicOptions {
    patch: UpdateTopicRequest;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function updateTopic({
    patch,
    idempotencyKey,
    deps = {},
}: UpdateTopicOptions): Promise<{ requestId: string }> {
    requirePositiveId(patch.id, "update_topic");
    const { requestId, response } = await mutateOperation(
        {
            case: "updateTopic",
            value: create(UpdateTopicRequestSchema, patch),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "ok") {
        throw unexpectedResult("update_topic", response);
    }
    return { requestId };
}

export interface DeleteTopicOptions {
    id: bigint;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function deleteTopic({
    id,
    idempotencyKey,
    deps = {},
}: DeleteTopicOptions): Promise<{ requestId: string }> {
    requirePositiveId(id, "delete_topic");
    const { requestId, response } = await mutateOperation(
        {
            case: "deleteTopic",
            value: create(DeleteByIdRequestSchema, { id }),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "ok") {
        throw unexpectedResult("delete_topic", response);
    }
    return { requestId };
}

export interface ForceDailyRefreshOptions {
    topics: string;
    tipcardType?: string;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function forceDailyRefresh({
    topics,
    tipcardType = "",
    idempotencyKey,
    deps = {},
}: ForceDailyRefreshOptions): Promise<{
    requestId: string;
    refresh: ForceDailyRefreshResponse;
}> {
    const { requestId, response } = await mutateOperation(
        {
            case: "forceDailyRefresh",
            value: create(ForceDailyRefreshRequestSchema, {
                topics,
                tipcardType,
            }),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "forceDailyRefresh") {
        throw unexpectedResult("force_daily_refresh", response);
    }
    return { requestId, refresh: response.result.value };
}

export async function listDocuments({ deps = {} }: ReadDeps = {}): Promise<{
    requestId: string;
    documents: Documents["docs"];
}> {
    const { requestId, response } = await readOperation(
        { case: "listDocuments", value: create(EmptySchema, {}) },
        "documents",
        deps,
    );
    if (response.result.case !== "documents") {
        throw unexpectedResult("list_documents", response);
    }
    return { requestId, documents: response.result.value.docs };
}

export interface GetDocumentOptions {
    id: bigint;
    deps?: CallDeps;
}

export async function getDocument({
    id,
    deps = {},
}: GetDocumentOptions): Promise<{
    requestId: string;
    document: DocumentDetail;
}> {
    requirePositiveId(id, "get_document");
    const { requestId, response } = await readOperation(
        { case: "getDocument", value: create(GetByIdRequestSchema, { id }) },
        "document",
        deps,
    );
    if (response.result.case !== "documentDetail") {
        throw unexpectedResult("get_document", response);
    }
    return { requestId, document: response.result.value };
}

function documentMutationResult(
    operation: string,
    response: ApiResponse,
): DocumentDetail {
    switch (response.result.case) {
        case "documentCreated":
        case "documentDetail":
            return response.result.value;
        default:
            throw unexpectedResult(operation, response);
    }
}

export interface CreateDocumentOptions {
    request: AddDocumentRequest;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function createDocument({
    request,
    idempotencyKey,
    deps = {},
}: CreateDocumentOptions): Promise<{
    requestId: string;
    document: DocumentDetail;
}> {
    const { requestId, response } = await mutateOperation(
        {
            case: "createDocument",
            value: create(AddDocumentRequestSchema, request),
        },
        idempotencyKey,
        deps,
    );
    return {
        requestId,
        document: documentMutationResult("create_document", response),
    };
}

export interface UploadDocumentOptions {
    request: UploadDocumentRequest;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function uploadDocument({
    request,
    idempotencyKey,
    deps = {},
}: UploadDocumentOptions): Promise<{
    requestId: string;
    document: DocumentDetail;
}> {
    const { requestId, response } = await mutateOperation(
        {
            case: "uploadDocument",
            value: create(UploadDocumentRequestSchema, request),
        },
        idempotencyKey,
        deps,
    );
    return {
        requestId,
        document: documentMutationResult("upload_document", response),
    };
}

export interface DeleteDocumentOptions {
    id: bigint;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function deleteDocument({
    id,
    idempotencyKey,
    deps = {},
}: DeleteDocumentOptions): Promise<{ requestId: string }> {
    requirePositiveId(id, "delete_document");
    const { requestId, response } = await mutateOperation(
        {
            case: "deleteDocument",
            value: create(DeleteByIdRequestSchema, { id }),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "ok") {
        throw unexpectedResult("delete_document", response);
    }
    return { requestId };
}

export interface DocumentTopicOptions {
    documentId: bigint;
    topicId: bigint;
    idempotencyKey: string;
    deps?: CallDeps;
}

async function updateDocumentTopic(
    operation: "attachDocumentTopic" | "detachDocumentTopic",
    request: DocumentTopicOptions,
): Promise<{ requestId: string }> {
    requirePositiveId(request.documentId, operation);
    requirePositiveId(request.topicId, operation);
    const { requestId, response } = await mutateOperation(
        {
            case: operation,
            value: create(AttachDocumentTopicRequestSchema, {
                documentId: request.documentId,
                topicId: request.topicId,
            }),
        },
        request.idempotencyKey,
        request.deps ?? {},
    );
    if (response.result.case !== "ok") {
        throw unexpectedResult(
            operation === "attachDocumentTopic"
                ? "attach_document_topic"
                : "detach_document_topic",
            response,
        );
    }
    return { requestId };
}

export function attachDocumentTopic(
    options: DocumentTopicOptions,
): Promise<{ requestId: string }> {
    return updateDocumentTopic("attachDocumentTopic", options);
}

export function detachDocumentTopic(
    options: DocumentTopicOptions,
): Promise<{ requestId: string }> {
    return updateDocumentTopic("detachDocumentTopic", options);
}

export interface ExploreLinkOptions {
    url: string;
    deps?: CallDeps;
}

export async function exploreLink({
    url,
    deps = {},
}: ExploreLinkOptions): Promise<{
    requestId: string;
    links: ExploredLinks["links"];
}> {
    const { requestId, response } = await readOperation(
        {
            case: "exploreLink",
            value: create(ExploreLinkRequestSchema, { url }),
        },
        "explore",
        deps,
    );
    if (response.result.case !== "exploredLinks") {
        throw unexpectedResult("explore_link", response);
    }
    return { requestId, links: response.result.value.links };
}

export async function testVisionModel({ deps = {} }: ReadDeps = {}): Promise<{
    requestId: string;
    result: VisionModelTest;
}> {
    const { requestId, response } = await readOperation(
        { case: "testVisionModel", value: create(EmptySchema, {}) },
        "vision",
        deps,
    );
    if (response.result.case !== "visionModelTest") {
        throw unexpectedResult("test_vision_model", response);
    }
    return { requestId, result: response.result.value };
}

export async function listPoolImages({ deps = {} }: ReadDeps = {}): Promise<{
    requestId: string;
    images: PoolImageInfo[];
}> {
    const { requestId, response } = await readOperation(
        { case: "listPoolImages", value: create(EmptySchema, {}) },
        "pool",
        deps,
    );
    if (response.result.case !== "poolImages") {
        throw unexpectedResult("list_pool_images", response);
    }
    return { requestId, images: response.result.value.images };
}

export interface CreatePoolImageOptions {
    request: AddPoolImageRequest;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function createPoolImage({
    request,
    idempotencyKey,
    deps = {},
}: CreatePoolImageOptions): Promise<{
    requestId: string;
    image: PoolImageCreated;
}> {
    const { requestId, response } = await mutateOperation(
        {
            case: "createPoolImage",
            value: create(AddPoolImageRequestSchema, request),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "poolImageCreated") {
        throw unexpectedResult("create_pool_image", response);
    }
    return { requestId, image: response.result.value };
}

export interface DeletePoolImageOptions {
    id: bigint;
    idempotencyKey: string;
    deps?: CallDeps;
}

export async function deletePoolImage({
    id,
    idempotencyKey,
    deps = {},
}: DeletePoolImageOptions): Promise<{ requestId: string }> {
    requirePositiveId(id, "delete_pool_image");
    const { requestId, response } = await mutateOperation(
        {
            case: "deletePoolImage",
            value: create(DeleteByIdRequestSchema, { id }),
        },
        idempotencyKey,
        deps,
    );
    if (response.result.case !== "ok") {
        throw unexpectedResult("delete_pool_image", response);
    }
    return { requestId };
}
