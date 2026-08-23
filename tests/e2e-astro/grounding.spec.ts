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
    AppSummarySchema,
    AppTopicInfoSchema,
    AppTopicsSchema,
    DocumentsSchema,
    PoolImagesSchema,
    type ApiResponse,
    type ApiV1Request,
} from "../../frontend-astro/src/generated/denpie_pb";

test.beforeEach(async ({ page }) => {
    await page.route("**/auth/me", async (route) => {
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({
                id: "grounding-fixture",
                username: "test",
                role: "user",
                display_name: "Grounding fixture",
                avatar_data: null,
                build_sha: "playwright",
            }),
        });
    });
});

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

async function installGroundingFixture(page: import("@playwright/test").Page) {
    const topic = create(AppTopicInfoSchema, {
        id: 11n,
        name: "Rust",
        tipcardType: "casual_tip",
        iconId: "lucide:book",
        topicColor: "#cc5533",
        totalCards: 10n,
        dueCards: 3n,
        pendingCards: 2n,
        completedCards: 5n,
    });
    await page.route("**/api/v1", async (route) => {
        const request = decode(route);
        switch (request.call.op.case) {
            case "getSummary":
                await fulfill(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "summary",
                            value: create(AppSummarySchema, {
                                topics: 1n,
                                totalCards: 10n,
                                dueCards: 3n,
                                activeCards: 4n,
                            }),
                        },
                    }),
                );
                return;
            case "listAppTopics":
                await fulfill(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "appTopics",
                            value: create(AppTopicsSchema, {
                                topics: [topic],
                            }),
                        },
                    }),
                );
                return;
            case "listDocuments":
                await fulfill(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "documents",
                            value: create(DocumentsSchema, { docs: [] }),
                        },
                    }),
                );
                return;
            case "listPoolImages":
                await fulfill(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "poolImages",
                            value: create(PoolImagesSchema, { images: [] }),
                        },
                    }),
                );
                return;
            default:
                await route.continue();
        }
    });
}

test("topic cards match the compact layout and the icon picker can set an icon", async ({
    page,
}) => {
    const suggestBodies: unknown[] = [];
    const setBodies: unknown[] = [];
    await installGroundingFixture(page);
    await page.route("**/app/topics/suggest-icons", async (route) => {
        suggestBodies.push(route.request().postDataJSON());
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({
                icons: [
                    "lucide:brain",
                    "lucide:code",
                    "lucide:flame",
                    "lucide:compass",
                    "lucide:cpu",
                ],
            }),
        });
    });
    await page.route("**/app/topics/set-icon", async (route) => {
        const body = route.request().postDataJSON() as {
            id: number;
            icon_id: string;
        };
        setBodies.push(body);
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({ icon_id: body.icon_id }),
        });
    });

    await page.goto("/grounding");
    const grid = page.getByTestId("topics-grid");
    await expect(grid).toBeVisible();
    await expect(grid).toHaveClass(/md:grid-cols-2/);
    await expect(grid).toHaveClass(/xl:grid-cols-3/);
    await expect(grid).toHaveClass(/2xl:grid-cols-4/);

    const card = page.getByTestId("topic-card-11");
    await expect(card.getByText("Rust")).toBeVisible();
    await expect(card.getByText("casual tip")).toBeVisible();
    await expect(page.getByTestId("topic-due-total-11")).toHaveText(
        "3 due / 10 total",
    );
    await expect(page.getByTestId("topic-11-pending-archive")).toBeVisible();
    await expect(page.getByTestId("topic-11-scheduled-archive")).toBeVisible();
    await expect(card.getByRole("button", { name: "Load" })).toBeVisible();
    await expect(card.getByRole("button", { name: "Delete" })).toBeVisible();
    await expect(
        card.getByRole("button", { name: "Save defaults" }),
    ).toHaveCount(0);

    await page.getByTestId("topic-icon-picker-11").click();
    const dialog = page.getByTestId("topic-icon-picker");
    await expect(dialog).toBeVisible();
    await expect(
        dialog.getByRole("heading", { name: "Pick an icon" }),
    ).toBeVisible();
    await expect(dialog.getByText("Topic: Rust")).toBeVisible();
    await expect(
        dialog.getByRole("button", { name: "Use flame" }),
    ).toBeVisible();
    expect(suggestBodies).toEqual([{ id: 11, excluded_icons: [] }]);

    await dialog.getByRole("button", { name: "Use flame" }).click();
    await expect(dialog).toBeHidden();
    await expect(
        page.getByRole("status").filter({ hasText: "Topic icon updated" }),
    ).toBeVisible();
    expect(setBodies).toEqual([{ id: 11, icon_id: "lucide:flame" }]);
});
