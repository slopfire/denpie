import { expect, test, type Route } from "@playwright/test";
import {
    create,
    fromBinary,
    toBinary,
} from "../../frontend-astro/node_modules/@bufbuild/protobuf/dist/esm/index.js";
import {
    ApiResponseSchema,
    ApiV1RequestSchema,
    ApiV1ResponseSchema,
    TipcardInfoSchema,
    TipcardsSchema,
    type ApiResponse,
    type ApiV1Request,
} from "../../frontend-astro/src/generated/denpie_pb";

test.beforeEach(async ({ page }) => {
    await page.route("**/auth/me", async (route) => {
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({
                id: "archive-fixture",
                username: "test",
                role: "user",
                display_name: "Archive fixture",
                avatar_data: null,
                build_sha: "playwright",
            }),
        });
    });
});

function card(
    id: bigint,
    title: string,
    status: string,
    repeatCount: number,
    fullContent: string,
) {
    return create(TipcardInfoSchema, {
        id,
        topicName: "Astro migration",
        title,
        status,
        repeatCount,
        fullContent,
        compressedContent: "short preview",
        tipcardType: "repeatable_tip",
        createdAt: "2026-08-23T00:00:00Z",
    });
}

function decode(route: Route): ApiV1Request {
    const bytes = route.request().postDataBuffer();
    if (bytes === null) throw new TypeError("missing protobuf request body");
    return fromBinary(ApiV1RequestSchema, bytes);
}

async function fulfill(
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

function installArchiveFixture(
    page: import("@playwright/test").Page,
    cards: ReturnType<typeof card>[],
) {
    return page.route("**/api/v1", async (route) => {
        const request = decode(route);
        if (request.call.op.case !== "listTipcards") {
            await route.continue();
            return;
        }
        await fulfill(
            route,
            request,
            create(ApiResponseSchema, {
                result: {
                    case: "tipcards",
                    value: create(TipcardsSchema, { cards }),
                },
            }),
        );
    });
}

test("topic archive links filter pending/scheduled cards and keep full details reachable", async ({
    page,
}) => {
    const longContent = Array.from(
        { length: 24 },
        (_, index) => `Long paragraph ${index + 1} remains readable.`,
    ).join("\n\n");
    await installArchiveFixture(page, [
        card(1n, "Pending card", "pending", 0, longContent),
        card(2n, "Scheduled card", "active", 3, "Scheduled content."),
    ]);

    await page.goto("/archive?status=pending&topic=Astro%20migration");
    await expect(
        page.getByRole("heading", { name: "Pending card" }),
    ).toBeVisible();
    await expect(
        page.getByRole("heading", { name: "Scheduled card" }),
    ).toHaveCount(0);
    await expect(page.getByTestId("archive-topic-back")).toContainText(
        "Pending cards for Astro migration",
    );
    const cardContent = page.getByTestId("archive-card-content-1");
    await expect(cardContent).toHaveCSS("overflow-y", "auto");
    await cardContent.evaluate((element) => {
        element.scrollTop = element.scrollHeight;
    });
    await expect(
        cardContent.getByText("Long paragraph 24 remains readable."),
    ).toBeVisible();

    await page.getByRole("button", { name: "Details" }).click();
    const dialog = page.getByTestId("archive-detail-dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("Long paragraph 24 remains readable.");
    const box = await dialog.boundingBox();
    expect(box?.width).toBeGreaterThan(600);

    await dialog.getByRole("button", { name: "Close" }).last().click();
    await page.evaluate(() =>
        Object.assign(window, { archiveNavigationMarker: "still-mounted" }),
    );
    await page.getByTestId("archive-topic-back").click();
    await expect(page).toHaveURL(/\/grounding\/?$/);
    expect(
        await page.evaluate(
            () =>
                Reflect.get(window, "archiveNavigationMarker") ===
                "still-mounted",
        ),
    ).toBe(true);
});
