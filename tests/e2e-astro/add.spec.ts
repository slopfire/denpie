import { expect, test, type Route } from "@playwright/test";
import {
    create,
    fromBinary,
    toBinary,
} from "../../frontend-astro/node_modules/@bufbuild/protobuf/dist/esm/index.js";
import {
    ApiErrorSchema,
    ApiResponseSchema,
    ApiV1RequestSchema,
    ApiV1ResponseSchema,
    FlowCardInfoSchema,
    FlowCardPageSchema,
    TipCardResponseSchema,
    TipcardDetailSchema,
    TipsResponseSchema,
    TipcardTypeValue,
    type ApiV1Request,
    type ApiResponse,
} from "../../frontend-astro/src/generated/denpie_pb";

test.beforeEach(async ({ page }) => {
    await page.route("**/auth/me", async (route) => {
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({
                id: "add-fixture",
                username: "test",
                role: "user",
                display_name: "Add fixture",
                avatar_data: null,
                build_sha: "playwright",
            }),
        });
    });
});

function card(id: bigint, title: string, topic = "Astro") {
    return create(FlowCardInfoSchema, {
        id,
        title,
        topicName: topic,
        fullContent: `${title} body`,
        tipcardType: "casual_tip",
        status: "active",
        images: [],
    });
}

function repeatableCard(
    id: bigint,
    title: string,
    topic: string,
    pinned: boolean,
) {
    return create(FlowCardInfoSchema, {
        id,
        title,
        topicName: topic,
        fullContent: `${title} body`,
        tipcardType: "repeatable_tip",
        status: "active",
        pinned,
        repeatable: true,
        images: [],
    });
}

function requestOf(route: Route): ApiV1Request {
    const bytes = route.request().postDataBuffer();
    if (bytes === null) throw new TypeError("missing protobuf request body");
    return fromBinary(ApiV1RequestSchema, bytes);
}

async function success(
    route: Route,
    request: ApiV1Request,
    response: ApiResponse,
) {
    await route.fulfill({
        status: 200,
        contentType: "application/x-protobuf",
        body: Buffer.from(
            toBinary(
                ApiV1ResponseSchema,
                create(ApiV1ResponseSchema, {
                    requestId: request.requestId,
                    outcome: { case: "success", value: response },
                }),
            ),
        ),
    });
}

async function list(
    route: Route,
    request: ApiV1Request,
    cards: ReturnType<typeof card>[],
) {
    await success(
        route,
        request,
        create(ApiResponseSchema, {
            result: {
                case: "flowCardPage",
                value: create(FlowCardPageSchema, { cards, hasMore: false }),
            },
        }),
    );
}

test("casual add sends the exact tips_v1 request and keeps existing controls enabled", async ({
    page,
}) => {
    const existing = card(10n, "Existing");
    let tipsCalls = 0;
    let captured: ApiV1Request | undefined;
    let release!: () => void;
    const held = new Promise<void>((resolve) => {
        release = resolve;
    });

    await page.route("**/api/v1", async (route) => {
        const request = requestOf(route);
        if (request.call.op.case === "listFlowCards")
            return list(route, request, [existing]);
        if (request.call.op.case !== "tipsV1")
            throw new TypeError(
                `unexpected operation ${String(request.call.op.case)}`,
            );
        tipsCalls += 1;
        captured = request;
        await held;
        await success(
            route,
            request,
            create(ApiResponseSchema, {
                result: {
                    case: "tips",
                    value: create(TipsResponseSchema, { tips: [] }),
                },
            }),
        );
    });

    await page.goto("/flow");
    await page.getByTestId("tips-topics").fill(" Rust, Python ");
    await page.getByTestId("tips-submit").click();
    await expect(page.getByTestId("tips-submit")).toBeDisabled();
    await expect(page.getByTestId("pin-10")).toBeEnabled();
    await expect(page.getByTestId("review-acknowledge-10")).toBeEnabled();
    await expect.poll(() => tipsCalls).toBe(1);
    const payload =
        captured?.call.op.case === "tipsV1"
            ? captured.call.op.value
            : undefined;
    expect(payload).toBeDefined();
    expect(payload).toMatchObject({
        count: 0,
        topics: ["Rust", "Python"],
        tipcardType: TipcardTypeValue.CASUAL,
        excludeCardIds: [],
        manualContent: "",
        manualCompressedContent: "",
        manualImageData: [],
    });
    expect(captured?.idempotencyKey).toMatch(/^[0-9a-f]{32}$/);
    release();
    await expect(page.getByText("Cards added")).toBeVisible();
});

test("detail-resolution failure retries resolution without resending tips_v1", async ({
    page,
}) => {
    const created = card(44n, "Created");
    let tipsCalls = 0;
    let detailCalls = 0;
    let reconcileCalls = 0;
    await page.route("**/api/v1", async (route) => {
        const request = requestOf(route);
        switch (request.call.op.case) {
            case "listFlowCards":
                reconcileCalls += 1;
                if (reconcileCalls === 1) return list(route, request, []);
                await route.fulfill({
                    status: 503,
                    contentType: "text/plain",
                    body: "reconciliation unavailable",
                });
                return;
            case "tipsV1":
                tipsCalls += 1;
                return success(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "tips",
                            value: create(TipsResponseSchema, {
                                tips: [
                                    create(TipCardResponseSchema, {
                                        id: 44n,
                                        topic: "Astro",
                                        tipcardType: "casual_tip",
                                    }),
                                ],
                            }),
                        },
                    }),
                );
            case "getTipcard":
                detailCalls += 1;
                if (detailCalls === 1) {
                    await route.fulfill({
                        status: 200,
                        contentType: "application/x-protobuf",
                        body: Buffer.from(
                            toBinary(
                                ApiV1ResponseSchema,
                                create(ApiV1ResponseSchema, {
                                    requestId: request.requestId,
                                    outcome: {
                                        case: "error",
                                        value: create(ApiErrorSchema, {
                                            code: 13,
                                            message: "detail unavailable",
                                            retryable: true,
                                        }),
                                    },
                                }),
                            ),
                        ),
                    });
                    return;
                }
                return success(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "tipcardDetail",
                            value: create(TipcardDetailSchema, {
                                card: created,
                            }),
                        },
                    }),
                );
            default:
                throw new TypeError(
                    `unexpected operation ${String(request.call.op.case)}`,
                );
        }
    });

    await page.goto("/flow");
    await page.getByTestId("tips-topics").fill("Astro");
    await page.getByTestId("tips-submit").click();
    await expect(page.getByTestId("add-retry-resolve")).toBeVisible();
    expect(tipsCalls).toBe(1);
    await page.getByTestId("add-retry-resolve").click();
    await expect(page.getByTestId("flow-slot-44")).toContainText("Created");
    expect(tipsCalls).toBe(1);
    expect(detailCalls).toBe(2);
});

test("manual add sends content and a small supported image", async ({
    page,
}) => {
    let captured: ApiV1Request | undefined;
    await page.route("**/api/v1", async (route) => {
        const request = requestOf(route);
        if (request.call.op.case === "listFlowCards")
            return list(route, request, []);
        if (request.call.op.case !== "tipsV1")
            throw new TypeError(
                `unexpected operation ${String(request.call.op.case)}`,
            );
        captured = request;
        return success(
            route,
            request,
            create(ApiResponseSchema, {
                result: {
                    case: "tips",
                    value: create(TipsResponseSchema, { tips: [] }),
                },
            }),
        );
    });
    await page.goto("/flow");
    await page.getByTestId("tips-topics").fill("Sketching");
    await page.getByTestId("tips-kind").getByText("Manual").click();
    await page
        .getByTestId("manual-content")
        .fill("Draw the concept from memory.");
    await page.getByTestId("manual-images-input").setInputFiles({
        name: "tiny.png",
        mimeType: "image/png",
        buffer: Buffer.from(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
            "base64",
        ),
    });
    await expect(page.getByText("1 images")).toBeVisible();
    await page.getByTestId("tips-submit").click();
    await expect(page.getByText("Cards added")).toBeVisible();
    const payload =
        captured?.call.op.case === "tipsV1"
            ? captured.call.op.value
            : undefined;
    expect(payload).toMatchObject({
        count: 0,
        topics: ["Sketching"],
        tipcardType: TipcardTypeValue.MANUAL,
        manualContent: "Draw the concept from memory.",
        manualCompressedContent: "",
    });
    expect(payload?.manualImageData).toHaveLength(1);
    expect(payload?.manualImageData[0]).toMatch(/^data:image\/png;base64,/);
});

test("repeatable add replaces the stable pinned topic slot before quiet reconciliation", async ({
    page,
}) => {
    const previous = repeatableCard(20n, "Previous Rust", "Rust", true);
    const replacementDetail = repeatableCard(
        21n,
        "Replacement Rust",
        "Rust",
        false,
    );
    const replacementList = repeatableCard(
        21n,
        "Replacement Rust",
        "Rust",
        true,
    );
    let listCalls = 0;
    let releaseReconciliation!: () => void;
    const reconciliationHeld = new Promise<void>((resolve) => {
        releaseReconciliation = resolve;
    });

    await page.addInitScript(() => {
        window.localStorage.setItem("denpie-pinned-card-order", "[20]");
    });
    await page.route("**/api/v1", async (route) => {
        const request = requestOf(route);
        switch (request.call.op.case) {
            case "listFlowCards":
                listCalls += 1;
                if (listCalls === 1) return list(route, request, [previous]);
                await reconciliationHeld;
                return list(route, request, [replacementList]);
            case "tipsV1":
                return success(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "tips",
                            value: create(TipsResponseSchema, {
                                tips: [
                                    create(TipCardResponseSchema, {
                                        id: 21n,
                                        topic: "Rust",
                                        tipcardType: "repeatable_tip",
                                    }),
                                ],
                            }),
                        },
                    }),
                );
            case "getTipcard":
                return success(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "tipcardDetail",
                            value: create(TipcardDetailSchema, {
                                card: replacementDetail,
                            }),
                        },
                    }),
                );
            default:
                throw new TypeError(
                    `unexpected operation ${String(request.call.op.case)}`,
                );
        }
    });

    await page.goto("/flow");
    await page.getByTestId("tips-topics").fill("Rust");
    await page.getByTestId("tips-kind").getByText("Repeat").click();
    await page.getByTestId("tips-submit").click();

    await expect(page.getByTestId("flow-slot-20")).toHaveCount(0);
    await expect(page.getByTestId("flow-pinned-grid")).toContainText(
        "Replacement Rust",
    );
    await expect
        .poll(() =>
            page.evaluate(() =>
                window.localStorage.getItem("denpie-pinned-card-order"),
            ),
        )
        .toBe("[21]");
    expect(listCalls).toBe(2);

    releaseReconciliation();
    await expect(page.getByTestId("add-success")).toBeVisible();
    await expect(page.getByTestId("flow-pinned-grid")).toContainText(
        "Replacement Rust",
    );
});
