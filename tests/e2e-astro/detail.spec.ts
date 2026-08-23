import { expect, test, type Page, type Route } from "@playwright/test";
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
    CardSourceSchema,
    FlowCardInfoSchema,
    FlowCardPageSchema,
    TipcardDetailSchema,
    TipcardImageInfoSchema,
    type ApiResponse,
    type ApiV1Request,
} from "../../frontend-astro/src/generated/denpie_pb";

const HUGE_ID = 9_007_199_254_740_993n;

test.beforeEach(async ({ page }) => {
    await page.route("**/auth/me", async (route) => {
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({
                id: "detail-fixture",
                username: "test",
                role: "user",
                display_name: "Detail fixture",
                avatar_data: null,
                build_sha: "playwright",
            }),
        });
    });
});

function card(id: bigint, title: string) {
    return create(FlowCardInfoSchema, {
        id,
        title,
        topicName: "Astro migration",
        fullContent: `${title} preview`,
        tipcardType: "casual_tip",
        status: "active",
        images: [],
    });
}

function detail(
    id: bigint,
    title: string,
    options: { images?: boolean; sources?: boolean } = {},
) {
    return create(FlowCardInfoSchema, {
        id,
        title,
        topicName: "Astro migration",
        fullContent: `${title} complete content with the details loaded lazily.`,
        compressedContent: `${title} compressed content`,
        tipcardType: "casual_tip",
        status: "active",
        createdAt: "2026-08-23T00:00:00Z",
        images: options.images
            ? [
                  create(TipcardImageInfoSchema, {
                      id: 88n,
                      position: 0n,
                      mimeType: "image/webp",
                      byteSize: 1234n,
                      downloadPath: "/api/v1/tipcard-images/88",
                  }),
              ]
            : [],
        sources: options.sources
            ? [
                  create(CardSourceSchema, {
                      documentId: 501n,
                      sourceType: "document",
                      title: "Migration notes",
                  }),
                  create(CardSourceSchema, {
                      documentId: 0n,
                      sourceType: "link",
                      title: "Astro documentation",
                      url: "https://docs.astro.build/en/guides/",
                  }),
              ]
            : [],
    });
}

function decode(route: Route): ApiV1Request {
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

async function detailSuccess(
    route: Route,
    request: ApiV1Request,
    loaded: ReturnType<typeof detail>,
) {
    await success(
        route,
        request,
        create(ApiResponseSchema, {
            result: {
                case: "tipcardDetail",
                value: create(TipcardDetailSchema, { card: loaded }),
            },
        }),
    );
}

async function detailError(
    route: Route,
    request: ApiV1Request,
    message: string,
) {
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
                            message,
                            retryable: true,
                        }),
                    },
                }),
            ),
        ),
    });
}

async function openDetail(page: Page, id: bigint) {
    await page.getByTestId(`detail-open-${id}`).click();
    await expect(page.getByTestId("card-detail-fullscreen")).toBeVisible();
}

async function closeDetail(page: Page) {
    await page.getByRole("button", { name: /close/i }).click();
    await expect(page.getByTestId("card-detail-fullscreen")).toHaveCount(0);
}

test("detail is lazy, preserves huge bigint IDs, renders content/images/sources, and caches reopen", async ({
    page,
}) => {
    const preview = card(HUGE_ID, "Lazy card");
    const loaded = detail(HUGE_ID, "Lazy card", {
        images: true,
        sources: true,
    });
    let listCalls = 0;
    const detailIds: bigint[] = [];

    await page.route("**/api/v1/tipcard-images/88", async (route) => {
        await route.fulfill({
            status: 200,
            contentType: "image/svg+xml",
            body: '<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="360"><rect width="1200" height="360" fill="#18181b"/><path d="M80 280 270 90l150 150 130-130 260 260H80Z" fill="#71717a"/><circle cx="930" cy="105" r="52" fill="#a1a1aa"/></svg>',
        });
    });
    await page.route("**/api/v1", async (route) => {
        const request = decode(route);
        if (request.call.op.case === "listFlowCards") {
            listCalls += 1;
            await list(route, request, [preview]);
            return;
        }
        if (request.call.op.case !== "getTipcard") {
            throw new TypeError(
                `unexpected operation ${String(request.call.op.case)}`,
            );
        }
        detailIds.push(request.call.op.value.id);
        await detailSuccess(route, request, loaded);
    });

    await page.goto("/flow");
    await expect(page.getByTestId(`flow-slot-${HUGE_ID}`)).toBeVisible();
    expect(detailIds).toEqual([]);

    await openDetail(page, HUGE_ID);
    await expect(page.getByTestId("card-detail-fullscreen")).toHaveCSS(
        "background-color",
        "rgb(9, 9, 11)",
    );
    await expect(page.getByTestId("card-detail-content")).toContainText(
        "complete content with the details loaded lazily",
    );
    await expect(
        page.getByTestId("card-detail-content").locator("img"),
    ).toHaveAttribute("src", "/api/v1/tipcard-images/88");
    await expect
        .poll(() =>
            page
                .getByTestId("card-detail-content")
                .locator("img")
                .evaluate((image) => (image as HTMLImageElement).naturalWidth),
        )
        .toBeGreaterThan(0);
    const sources = page.getByTestId("card-detail-sources");
    await expect(sources).toContainText("Migration notes");
    await expect(sources).toContainText("Astro documentation");
    const sourceLink = sources.getByRole("link");
    await expect(sourceLink).toHaveAttribute(
        "href",
        "https://docs.astro.build/en/guides/",
    );
    const fullscreen = page.getByTestId("card-detail-fullscreen");
    await expect(
        fullscreen.getByTestId(`review-dismiss-${HUGE_ID}`),
    ).toBeVisible();
    await expect(fullscreen.getByTestId(`pin-${HUGE_ID}`)).toBeVisible();
    await expect(
        fullscreen.getByTestId(`manage-images-${HUGE_ID}`),
    ).toBeVisible();
    await expect(sourceLink).toHaveAttribute("target", "_blank");
    await expect(sourceLink).toHaveAttribute(
        "rel",
        /^(?=.*\bnoopener\b)(?=.*\bnoreferrer\b)/,
    );
    await expect(
        sources.getByText("Migration notes").locator("xpath=ancestor::a"),
    ).toHaveCount(0);
    expect(detailIds).toEqual([HUGE_ID]);

    const bounds = await fullscreen.boundingBox();
    const contentBounds = await fullscreen
        .locator(":scope > div")
        .boundingBox();
    if (bounds === null || contentBounds === null) {
        throw new TypeError("fullscreen detail bounds unavailable");
    }
    await page.mouse.click(bounds.x + 20, contentBounds.y + 40);
    await expect(page.getByTestId("card-detail-fullscreen")).toHaveCount(0);

    await openDetail(page, HUGE_ID);

    await closeDetail(page);
    await openDetail(page, HUGE_ID);
    await expect(page.getByTestId("card-detail-content")).toContainText(
        "complete content",
    );
    expect(detailIds).toEqual([HUGE_ID]);
    expect(listCalls).toBe(1);

    await page.keyboard.press("Escape");
    await expect(page.getByTestId("card-detail-fullscreen")).toHaveCount(0);
    await expect(page.getByTestId(`detail-open-${HUGE_ID}`)).toBeFocused();
});

test("detail failure remains visible and retries only the lazy read", async ({
    page,
}) => {
    const id = 41n;
    let listCalls = 0;
    let detailCalls = 0;
    await page.route("**/api/v1", async (route) => {
        const request = decode(route);
        if (request.call.op.case === "listFlowCards") {
            listCalls += 1;
            await list(route, request, [card(id, "Retry card")]);
            return;
        }
        if (request.call.op.case !== "getTipcard") {
            throw new TypeError(
                `unexpected operation ${String(request.call.op.case)}`,
            );
        }
        detailCalls += 1;
        if (detailCalls === 1) {
            await detailError(route, request, "detail temporarily unavailable");
        } else {
            await detailSuccess(route, request, detail(id, "Retry card"));
        }
    });

    await page.goto("/flow");
    await openDetail(page, id);
    await expect(page.getByTestId("card-detail-error")).toContainText(
        "detail temporarily unavailable",
    );
    await page.getByTestId("card-detail-retry").click();
    await expect(page.getByTestId("card-detail-content")).toContainText(
        "complete content",
    );
    expect(detailCalls).toBe(2);
    expect(listCalls).toBe(1);
});

test("closing A while its read is held prevents stale A from overwriting B", async ({
    page,
}) => {
    const a = 101n;
    const b = 202n;
    let listCalls = 0;
    let releaseA!: () => void;
    const aGate = new Promise<void>((resolve) => {
        releaseA = resolve;
    });
    await page.route("**/api/v1", async (route) => {
        const request = decode(route);
        if (request.call.op.case === "listFlowCards") {
            listCalls += 1;
            await list(route, request, [card(a, "Card A"), card(b, "Card B")]);
            return;
        }
        if (request.call.op.case !== "getTipcard") {
            throw new TypeError(
                `unexpected operation ${String(request.call.op.case)}`,
            );
        }
        if (request.call.op.value.id === a) {
            await aGate;
            await detailSuccess(route, request, detail(a, "Card A"));
        } else if (request.call.op.value.id === b) {
            await detailSuccess(route, request, detail(b, "Card B"));
        } else {
            throw new TypeError(
                `unexpected card id ${request.call.op.value.id}`,
            );
        }
    });

    await page.goto("/flow");
    await openDetail(page, a);
    await expect(page.getByTestId("card-detail-loading")).toBeVisible();
    await closeDetail(page);
    await openDetail(page, b);
    await expect(page.getByTestId("card-detail-content")).toContainText(
        "Card B complete content",
    );

    releaseA();
    await expect(page.getByTestId("card-detail-content")).toContainText(
        "Card B complete content",
    );
    await expect(page.getByTestId("card-detail-content")).not.toContainText(
        "Card A complete content",
    );
    expect(listCalls).toBe(1);
});

test("desktop fullscreen leaves the sidebar unblurred and usable", async ({
    page,
}) => {
    const id = 77n;
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.route("**/api/v1", async (route) => {
        const request = decode(route);
        if (request.call.op.case === "listFlowCards") {
            await list(route, request, [card(id, "Sidebar card")]);
            return;
        }
        if (request.call.op.case === "getTipcard") {
            await detailSuccess(route, request, detail(id, "Sidebar card"));
            return;
        }
        await route.continue();
    });

    await page.goto("/flow");
    await openDetail(page, id);

    const sidebar = page.getByTestId("desktop-sidebar");
    const overlay = page.locator('[data-slot="dialog-overlay"]');
    const fullscreen = page.getByTestId("card-detail-fullscreen");
    await expect(sidebar).toBeVisible();
    await expect(overlay).toBeVisible();
    const sidebarBox = await sidebar.boundingBox();
    const overlayBox = await overlay.boundingBox();
    const fullscreenBox = await fullscreen.boundingBox();
    if (
        sidebarBox === null ||
        overlayBox === null ||
        fullscreenBox === null
    ) {
        throw new TypeError("sidebar or fullscreen bounds unavailable");
    }
    expect(overlayBox.x).toBeGreaterThanOrEqual(
        sidebarBox.x + sidebarBox.width - 1,
    );
    expect(fullscreenBox.x).toBeGreaterThanOrEqual(
        sidebarBox.x + sidebarBox.width - 1,
    );
    expect(overlayBox.x).toBeGreaterThan(0);

    await sidebar.getByRole("link", { name: "Settings", exact: true }).click();
    await expect(page).toHaveURL(/\/settings$/);
    await expect(page.getByTestId("card-detail-fullscreen")).toBeHidden();
    await expect(
        page.getByRole("heading", { name: "Settings", exact: true }),
    ).toBeVisible();
});
