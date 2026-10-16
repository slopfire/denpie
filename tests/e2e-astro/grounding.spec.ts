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
    DocumentInfoSchema,
    DocumentsSchema,
    EmptySchema,
    EnhancePromptTemplateResultSchema,
    GroundingEvidenceSchema,
    GroundingSearchProfileSchema,
    PoolImagesSchema,
    PreviewGroundingResultSchema,
    SettingsSchema,
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
    const sourceMutations: Array<{
        operation: "attachDocumentTopic" | "detachDocumentTopic";
        documentId: bigint;
        topicId: bigint;
    }> = [];
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
        groundingStrategy: "rag",
        groundingSourceMode: "documents",
        groundingVerificationMode: "standard",
        groundingBehavior: "standard",
        groundingSearchProfile: create(GroundingSearchProfileSchema, {
            depth: "balanced",
            freshness: "any",
            failurePolicy: "stop",
        }),
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
            case "getSettings":
                await fulfill(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "settings",
                            value: create(SettingsSchema, {
                                template:
                                    "Write useful daily tip cards about {topic}.",
                                groundingSourceMode: "web",
                                groundingBehavior: "standard",
                                searchProvider: "tavily",
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
                            value: create(DocumentsSchema, {
                                docs: [
                                    create(DocumentInfoSchema, {
                                        id: 51n,
                                        sourceType: "link",
                                        title: "Rust Reference",
                                        topicIds: [11n],
                                    }),
                                    create(DocumentInfoSchema, {
                                        id: 52n,
                                        sourceType: "document",
                                        title: "Compiler Notes",
                                    }),
                                ],
                            }),
                        },
                    }),
                );
                return;
            case "attachDocumentTopic":
            case "detachDocumentTopic":
                sourceMutations.push({
                    operation: request.call.op.case,
                    documentId: request.call.op.value.documentId,
                    topicId: request.call.op.value.topicId,
                });
                await fulfill(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "ok",
                            value: create(EmptySchema, {}),
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
            case "enhancePromptTemplate":
                await fulfill(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "enhancePromptTemplate",
                            value: create(EnhancePromptTemplateResultSchema, {
                                promptTemplate:
                                    "Write useful daily tip cards about {topic}. Go deeper than known titles.",
                                groundingStrategy: "agentic",
                                rationale:
                                    "Looked at recent titles: known cards should not be repeated.",
                            }),
                        },
                    }),
                );
                return;
            case "previewGrounding":
                await fulfill(
                    route,
                    request,
                    create(ApiResponseSchema, {
                        result: {
                            case: "previewGrounding",
                            value: create(PreviewGroundingResultSchema, {
                                groundingSourceMode:
                                    request.call.op.value.groundingSourceMode ||
                                    "documents",
                                groundingVerificationMode:
                                    request.call.op.value
                                        .groundingVerificationMode ||
                                    "standard",
                                groundingBehavior:
                                    request.call.op.value.groundingBehavior ||
                                    "standard",
                                groundingSearchProfile: create(
                                    GroundingSearchProfileSchema,
                                    {
                                        depth: "balanced",
                                        freshness: "any",
                                        failurePolicy: "stop",
                                    },
                                ),
                                backend: "documents",
                                queries: ["Rust"],
                                evidence: [
                                    create(GroundingEvidenceSchema, {
                                        sourceType: "document",
                                        title: "Rust Reference",
                                        excerpt:
                                            "Ownership is Rust's central memory model.",
                                        documentId: 51n,
                                    }),
                                ],
                            }),
                        },
                    }),
                );
                return;
            default:
                await route.continue();
        }
    });
    return { sourceMutations };
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
    await expect(card.getByText("Casual", { exact: true })).toBeVisible();
    await expect(page.getByTestId("topic-due-total-11")).toHaveText(
        "3 due / 10 total",
    );
    await expect(page.getByTestId("topic-11-pending-archive")).toBeVisible();
    await expect(page.getByTestId("topic-11-scheduled-archive")).toBeVisible();
    await expect(card.getByRole("button", { name: "Load" })).toBeVisible();
    await expect(card.getByRole("button", { name: "Delete" })).toHaveCount(0);
    await page.getByTestId("topic-more-11").click();
    await expect(page.getByTestId("topic-delete-11")).toBeVisible();
    await page.keyboard.press("Escape");
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

test("topic editor can reset and enhance the prompt template", async ({
    page,
}) => {
    await installGroundingFixture(page);
    await page.goto("/grounding");
    await page.getByTestId("topic-edit-11").click();
    const editor = page.getByTestId("topic-editor");
    await expect(editor).toBeVisible();
    const prompt = editor.locator("#topic-prompt");
    await prompt.fill("Custom {topic} brief");
    await editor.getByTestId("topic-prompt-reset").click();
    await expect(prompt).toHaveValue(
        "Write useful daily tip cards about {topic}.",
    );
    await editor.getByTestId("topic-prompt-enhance").click();
    await expect(prompt).toHaveValue(
        "Write useful daily tip cards about {topic}. Go deeper than known titles.",
    );
    await expect(
        editor.getByText("known cards should not be repeated"),
    ).toBeVisible();
});

test("topic editor source choices stay inside desktop and mobile dialogs", async ({
    page,
}) => {
    await page.setViewportSize({ width: 800, height: 900 });
    await installGroundingFixture(page);
    await page.goto("/grounding");
    await page.getByTestId("topic-edit-11").click();
    const editor = page.getByTestId("topic-editor");
    await expect(editor).toBeVisible();

    const expectNoHorizontalOverflow = async () => {
        const viewport = page.viewportSize();
        const bounds = await editor.boundingBox();
        expect(viewport).not.toBeNull();
        expect(bounds).not.toBeNull();
        if (viewport === null || bounds === null) return;

        expect(bounds.x).toBeGreaterThanOrEqual(0);
        expect(bounds.x + bounds.width).toBeLessThanOrEqual(viewport.width);
        expect(
            await editor.evaluate(
                (element) => element.scrollWidth - element.clientWidth,
            ),
        ).toBeLessThanOrEqual(1);
    };

    await expectNoHorizontalOverflow();
    await page.setViewportSize({ width: 320, height: 800 });
    await expectNoHorizontalOverflow();

    const sourceChoices = editor.getByLabel("Information source");
    for (const name of [
        "Use Grounding defaults",
        "Model knowledge",
        "Web",
        "My sources",
        "Web + my sources",
    ]) {
        await expect(
            sourceChoices.getByRole("button", { name, exact: true }),
        ).toBeVisible();
    }
    const behaviorChoices = editor.getByLabel("Behavior depth");
    for (const name of [
        "Use Grounding defaults",
        "Standard",
        "Fact Check",
        "Agentic",
    ]) {
        await expect(
            behaviorChoices.getByRole("button", { name, exact: true }),
        ).toBeVisible();
    }
});

test("topic editor saves changed grounding source assignments", async ({
    page,
}) => {
    const fixture = await installGroundingFixture(page);
    await page.goto("/grounding");
    await page.getByTestId("topic-edit-11").click();
    const editor = page.getByTestId("topic-editor");
    await expect(editor.getByText("Grounding sources")).toBeVisible();
    await editor.getByRole("button", { name: "Web", exact: true }).click();
    await expect(editor.getByText("Grounding sources")).toHaveCount(0);
    const webSearch = editor.getByTestId("grounding-web-search");
    await expect(webSearch.getByText("Search provider")).toBeVisible();
    await expect(webSearch.getByText("Results per search")).toBeVisible();
    await webSearch
        .getByRole("button", { name: "Use Grounding defaults" })
        .click();
    await expect(webSearch.getByText("Results per search")).toHaveCount(0);
    await webSearch
        .getByRole("button", { name: "Customize for this topic" })
        .click();
    await expect(webSearch.getByText("Results per search")).toBeVisible();
    await editor
        .getByRole("button", { name: "My sources", exact: true })
        .click();
    await expect(editor.getByText("Grounding sources")).toBeVisible();

    const rustReference = editor.getByRole("switch", {
        name: "Rust Reference",
    });
    const compilerNotes = editor.getByRole("switch", {
        name: "Compiler Notes",
    });
    await expect(rustReference).toBeChecked();
    await expect(compilerNotes).not.toBeChecked();

    await rustReference.click();
    await compilerNotes.click();
    await editor.getByTestId("grounding-preview").click();
    const preview = editor.getByTestId("grounding-preview-ready");
    await expect(preview).toBeVisible();
    await expect(preview.getByText("Rust Reference")).toBeVisible();
    await expect(
        preview.getByText("Ownership is Rust's central memory model."),
    ).toBeVisible();
    await editor.getByRole("button", { name: "Save topic" }).click();
    await expect(editor).toBeHidden();
    expect(fixture.sourceMutations).toEqual([
        {
            operation: "detachDocumentTopic",
            documentId: 51n,
            topicId: 11n,
        },
        {
            operation: "attachDocumentTopic",
            documentId: 52n,
            topicId: 11n,
        },
    ]);
});
