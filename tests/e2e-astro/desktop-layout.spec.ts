import { expect, test, type Page, type Route } from "@playwright/test";
import {
    create,
    fromBinary,
    toBinary,
} from "../../frontend-astro/node_modules/@bufbuild/protobuf/dist/esm/index.js";
import {
    ApiResponseSchema,
    ApiV1RequestSchema,
    ApiV1ResponseSchema,
    FlowCardInfoSchema,
    FlowCardPageSchema,
} from "../../frontend-astro/src/generated/denpie_pb";

function flowCard(
    id: bigint,
    topicName: string,
    title: string,
    options: { pinned?: boolean; repeatable?: boolean } = {},
) {
    return create(FlowCardInfoSchema, {
        id,
        topicName,
        title,
        fullContent:
            id === 1n
                ? "Full text restored inside the compact card."
                : `${title} body`,
        compressedContent:
            id === 1n
                ? Array.from(
                      { length: 24 },
                      (_, index) => `Compact text line ${index + 1}.`,
                  ).join("\n")
                : "",
        topicIcon: id === 1n ? "lucide:book" : "",
        topicColor: id === 1n ? "#ff7a00" : "",
        tipcardType: options.repeatable ? "repeatable_tip" : "casual_tip",
        status: "active",
        pinned: options.pinned ?? false,
        pendingCount: options.repeatable ? 5n : 0n,
        createdAt: `2026-08-${String(Number(id % 20n) + 1).padStart(2, "0")}T00:00:00Z`,
    });
}

function fixtureCards() {
    const cards = [
        flowCard(100n, "Pinned topic", "Pinned repeatable", {
            pinned: true,
            repeatable: true,
        }),
    ];
    for (const [topicIndex, topic] of ["Alpha", "Beta", "Gamma"].entries()) {
        for (let cardIndex = 0; cardIndex < 4; cardIndex += 1) {
            const id = BigInt(topicIndex * 4 + cardIndex + 1);
            cards.push(flowCard(id, topic, `${topic} card ${cardIndex + 1}`));
        }
    }
    return cards;
}

async function fulfillFlow(route: Route) {
    const body = route.request().postDataBuffer();
    if (body === null) throw new TypeError("missing protobuf request body");
    const request = fromBinary(ApiV1RequestSchema, body);
    if (request.call.op.case !== "listFlowCards") {
        throw new TypeError(
            `unexpected operation ${String(request.call.op.case)}`,
        );
    }
    const response = create(ApiResponseSchema, {
        result: {
            case: "flowCardPage",
            value: create(FlowCardPageSchema, {
                cards: fixtureCards(),
                hasMore: false,
            }),
        },
    });
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

async function openFixture(page: Page) {
    await page.route("**/auth/me", async (route) => {
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({
                id: "desktop-layout",
                username: "test",
                role: "user",
                display_name: "Desktop layout",
                avatar_data: null,
                build_sha: "playwright",
            }),
        });
    });
    await page.route("**/api/v1", fulfillFlow);
    await page.goto("/flow");
    await expect(page.getByTestId("flow-grid")).toBeVisible();
}

test("desktop keeps the shell, toolbar, Transmission sections, and compact card anatomy", async ({
    page,
}) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await openFixture(page);

    const sidebar = page.getByTestId("desktop-sidebar");
    const main = page.getByTestId("app-main");
    await expect(sidebar).toBeVisible();
    await expect(page.getByTestId("mobile-navigation")).toBeHidden();
    await expect(page.getByTestId("account-menu-btn")).toBeVisible();
    expect(await sidebar.boundingBox()).toMatchObject({ x: 0, width: 224 });
    expect((await main.boundingBox())?.x).toBe(224);

    const form = page.getByTestId("transmission-form-surface");
    const toolbar = page.getByTestId("flow-toolbar");
    const mainBox = await main.boundingBox();
    const formBox = await form.boundingBox();
    if (mainBox === null || formBox === null)
        throw new TypeError("missing layout box");
    const contentRight = mainBox.x + mainBox.width - 24;
    const contentLeft = mainBox.x + 24;
    expect(
        Math.abs(contentRight - (formBox.x + formBox.width)),
    ).toBeLessThanOrEqual(1);
    expect(formBox.width).toBeLessThan(mainBox.width - 160);
    expect(formBox.x).toBeGreaterThan(mainBox.x + 80);
    expect(formBox.x).toBeGreaterThanOrEqual(contentLeft - 1);
    const addBox = await page.getByTestId("tips-submit").boundingBox();
    const topicsBox = await page.getByTestId("tips-topics").boundingBox();
    const kindBox = await page.getByTestId("tips-kind").boundingBox();
    if (addBox === null || topicsBox === null || kindBox === null)
        throw new TypeError("missing add-form control box");
    expect(topicsBox.x).toBeGreaterThanOrEqual(contentLeft - 1);
    expect(kindBox.x).toBeGreaterThanOrEqual(contentLeft - 1);
    expect(addBox.x + addBox.width).toBeLessThanOrEqual(contentRight + 1);
    await expect(toolbar).toHaveCSS("position", "sticky");

    await page.setViewportSize({ width: 1280, height: 800 });
    const narrowMain = await main.boundingBox();
    const narrowAdd = await page.getByTestId("tips-submit").boundingBox();
    const narrowTopics = await page.getByTestId("tips-topics").boundingBox();
    if (narrowMain === null || narrowAdd === null || narrowTopics === null)
        throw new TypeError("missing narrow add-form box");
    const narrowRight = narrowMain.x + narrowMain.width - 24;
    const narrowLeft = narrowMain.x + 24;
    expect(narrowTopics.x).toBeGreaterThanOrEqual(narrowLeft - 1);
    expect(narrowAdd.x + narrowAdd.width).toBeLessThanOrEqual(narrowRight + 1);

    await expect(
        page.getByTestId("flow-pinned-grid").locator("li"),
    ).toHaveCount(1);
    await expect(page.getByTestId("flow-grid").locator("li")).toHaveCount(9);
    await expect(page.getByTestId("flow-other-grid").locator("li")).toHaveCount(
        3,
    );
    const sectionOrder = await page.evaluate(() => {
        const pins = document.querySelector("#flow-pins");
        const picks = document.querySelector('[data-testid="flow-grid"]');
        const other = document.querySelector(
            '[data-testid="flow-other-cards"]',
        );
        if (pins === null || picks === null || other === null) return false;
        return (
            Boolean(
                pins.compareDocumentPosition(picks) &
                Node.DOCUMENT_POSITION_FOLLOWING,
            ) &&
            Boolean(
                picks.compareDocumentPosition(other) &
                Node.DOCUMENT_POSITION_FOLLOWING,
            )
        );
    });
    expect(sectionOrder).toBe(true);

    const pinned = page.getByTestId("flow-slot-100");
    await expect(pinned.locator('[data-repeatable-stack="3"]')).toBeVisible();
    const first = page.getByTestId("flow-slot-1");
    await expect(first.getByTestId("card-title-bar-1")).toContainText("Alpha");
    await expect(first.locator('[data-slot="card-title"]')).toHaveCount(0);
    await expect(first.getByTestId("card-actions-1")).toBeVisible();
    await expect(first.getByTestId("topic-icon-1")).toHaveCSS(
        "color",
        "rgb(255, 122, 0)",
    );
    const body = first.getByTestId("card-body-1");
    await expect(body).toContainText("Compact text line 1.");
    await expect(body).toHaveCSS("overflow-y", "auto");
    await body.getByRole("button", { name: "Expand text for card 1" }).click();
    await expect(first).toContainText(
        "Full text restored inside the compact card.",
    );
    await first
        .getByRole("button", { name: "Show information for card 1" })
        .click();
    await expect(page.getByText("Scheduled repeat")).toBeVisible();
    await expect(page.getByText("Not scheduled")).toBeVisible();
});

test("mobile keeps one-column cards above the in-flow five-item dock", async ({
    page,
}) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await openFixture(page);

    await expect(page.getByTestId("desktop-sidebar")).toBeHidden();
    const main = page.getByTestId("app-main");
    const dock = page.getByTestId("mobile-navigation");
    await expect(dock).toBeVisible();
    await expect(dock.locator("a, [aria-disabled=true]")).toHaveCount(5);
    await expect(dock.getByText("Transmission", { exact: true })).toBeVisible();
    await expect(dock.getByText("Grounding", { exact: true })).toBeVisible();
    const dockLabels = await dock.locator("a").allTextContents();
    expect(dockLabels.map((label) => label.trim())).toEqual([
        "Transmission",
        "Grounding",
        "Settings",
        "API Keys",
        "Archive",
    ]);
    const mainBox = await main.boundingBox();
    const dockBox = await dock.boundingBox();
    if (mainBox === null || dockBox === null)
        throw new TypeError("missing mobile boxes");
    expect(mainBox.y + mainBox.height).toBeLessThanOrEqual(dockBox.y + 1);

    const form = page.getByTestId("transmission-form-surface");
    const formBox = await form.boundingBox();
    if (formBox === null) throw new TypeError("missing mobile form box");
    expect(formBox.x).toBeLessThan(mainBox.x + 32);
    expect(formBox.width).toBeGreaterThan((mainBox.width - 48) * 0.9);

    const grid = page.getByTestId("flow-grid");
    await expect(grid).toHaveCSS("grid-template-columns", /^(?!.* ).+$/);
    const firstCard = page.getByTestId("flow-slot-1");
    await expect(firstCard.getByTestId("card-title-bar-1")).toBeVisible();
    expect((await firstCard.boundingBox())?.width).toBeLessThanOrEqual(358);
});
