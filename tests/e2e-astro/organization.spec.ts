import { expect, test, type Locator } from "@playwright/test";
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

// Authentication itself has dedicated live-server coverage. These
// organization cases isolate the authenticated UI state and protobuf API so
// repeated contexts do not compete with the server's auth rate limiter.
test.beforeEach(async ({ page }) => {
    await page.route("**/auth/me", async (route) => {
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify({
                id: "organization-fixture",
                username: "test",
                role: "user",
                display_name: "Organization fixture",
                avatar_data: null,
                build_sha: "playwright",
            }),
        });
    });
});

function orgCard(
    id: bigint,
    topicName: string,
    title: string,
    createdAt: string,
    pinned = false,
) {
    return create(FlowCardInfoSchema, {
        id,
        title,
        topicName,
        fullContent: `${title} body`,
        tipcardType: "casual_tip",
        status: "active",
        createdAt,
        pinned,
    });
}

// Scrambled server order. Topic mode must yield alpha(3), beta-new(4),
// beta-old(2); date mode yields 4, 3, 2. Card 1 starts pinned and lives in
// the dedicated pinned section regardless of mode.
function orgCards() {
    return [
        orgCard(4n, "beta", "Beta new", "2026-03-03T00:00:00Z"),
        orgCard(3n, "alpha", "Alpha only", "2026-02-02T00:00:00Z"),
        orgCard(2n, "beta", "Beta old", "2026-01-01T00:00:00Z"),
        orgCard(1n, "zeta", "Pinned card", "2025-12-01T00:00:00Z", true),
    ];
}

function protobufListResponse(envelopeBody: Buffer): Buffer {
    const envelope = fromBinary(ApiV1RequestSchema, envelopeBody);
    const response = create(ApiResponseSchema, {
        result: {
            case: "flowCardPage",
            value: create(FlowCardPageSchema, {
                cards: orgCards(),
                hasMore: false,
            }),
        },
    });
    return Buffer.from(
        toBinary(
            ApiV1ResponseSchema,
            create(ApiV1ResponseSchema, {
                requestId: envelope.requestId,
                outcome: { case: "success", value: response },
            }),
        ),
    );
}

async function expectUnpinnedOrder(grid: Locator, ids: string[]) {
    const seen: string[] = [];
    for (const li of await grid.locator("li").all()) {
        seen.push((await li.getAttribute("data-testid")) ?? "");
    }
    expect(seen).toEqual(ids.map((id) => `flow-slot-${id}`));
}

test("initially pinned card appears only in the pinned section", async ({
    page,
}) => {
    let listCalls = 0;
    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        listCalls += 1;
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: protobufListResponse(bytes),
        });
    });

    await page.goto("/flow");
    const pinnedGrid = page.getByTestId("flow-pinned-grid");
    await expect(pinnedGrid).toBeVisible();
    await expect(pinnedGrid.locator("li")).toHaveCount(1);
    await expect(pinnedGrid.getByTestId("flow-slot-1")).toContainText(
        "Pinned card",
    );
    // The pinned card never leaks into the unpinned grid.
    await expect(
        page.getByTestId("flow-grid").getByTestId("flow-slot-1"),
    ).toHaveCount(0);
    expect(listCalls).toBe(1);
});

test("default Topic order is exact across sections", async ({ page }) => {
    let listCalls = 0;
    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        listCalls += 1;
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: protobufListResponse(bytes),
        });
    });

    await page.goto("/flow");
    const grid = page.getByTestId("flow-grid");
    await expect(grid).toBeVisible();
    await expectUnpinnedOrder(grid, ["3", "4", "2"]);
    await expect(page.getByTestId("flow-sort-topic")).toHaveAttribute(
        "data-pressed",
        "",
    );
    expect(listCalls).toBe(1);
});

test("clicking Date reorders only the unpinned list without another read", async ({
    page,
}) => {
    let listCalls = 0;
    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        listCalls += 1;
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: protobufListResponse(bytes),
        });
    });

    await page.goto("/flow");
    const grid = page.getByTestId("flow-grid");
    await expect(grid).toBeVisible();

    await page.getByTestId("flow-sort-date").click();
    await expect(page.getByTestId("flow-sort-date")).toHaveAttribute(
        "data-pressed",
        "",
    );
    await expectUnpinnedOrder(grid, ["4", "3", "2"]);
    // Pure client-side reorganization: no refetch of Flow.
    expect(listCalls).toBe(1);
    // The pinned section is untouched by the mode change.
    await expect(
        page.getByTestId("flow-pinned-grid").locator("li"),
    ).toHaveCount(1);
});

test("selected Date survives reload via localStorage with exact order", async ({
    page,
}) => {
    let listCalls = 0;
    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        listCalls += 1;
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: protobufListResponse(bytes),
        });
    });

    await page.goto("/flow");
    const grid = page.getByTestId("flow-grid");
    await expect(grid).toBeVisible();
    await page.getByTestId("flow-sort-date").click();
    await expectUnpinnedOrder(grid, ["4", "3", "2"]);

    await page.reload();
    await expect(grid).toBeVisible();
    await expectUnpinnedOrder(grid, ["4", "3", "2"]);
    await expect(page.getByTestId("flow-sort-date")).toHaveAttribute(
        "data-pressed",
        "",
    );
    await expect(page.getByTestId("flow-sort-topic")).not.toHaveAttribute(
        "data-pressed",
        "",
    );
    // Exactly two total reads: one per full load, none for the sort itself.
    expect(listCalls).toBe(2);
});

test("the active toggle cannot be deselected into an empty state", async ({
    page,
}) => {
    let listCalls = 0;
    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        listCalls += 1;
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: protobufListResponse(bytes),
        });
    });

    await page.goto("/flow");
    const grid = page.getByTestId("flow-grid");
    await expect(grid).toBeVisible();

    // Default selection is Topic; toggling it off must be rejected.
    await page.getByTestId("flow-sort-topic").click();
    await expect(page.getByTestId("flow-sort-topic")).toHaveAttribute(
        "data-pressed",
        "",
    );
    await expect(page.getByTestId("flow-sort-date")).not.toHaveAttribute(
        "data-pressed",
        "",
    );
    await expectUnpinnedOrder(grid, ["3", "4", "2"]);

    // Switching to Date then toggling Date off keeps Date selected too.
    await page.getByTestId("flow-sort-date").click();
    await page.getByTestId("flow-sort-date").click();
    await expect(page.getByTestId("flow-sort-date")).toHaveAttribute(
        "data-pressed",
        "",
    );
    await expectUnpinnedOrder(grid, ["4", "3", "2"]);
    expect(listCalls).toBe(1);
});

test("unknown stored preferences normalize to Topic", async ({ page }) => {
    await page.addInitScript(() => {
        window.localStorage.setItem("denpie-flow-sort", "title-evil");
    });
    let listCalls = 0;
    await page.route("**/api/v1", async (route) => {
        const bytes = route.request().postDataBuffer();
        if (bytes === null)
            throw new TypeError("missing protobuf request body");
        listCalls += 1;
        await route.fulfill({
            status: 200,
            contentType: "application/x-protobuf",
            body: protobufListResponse(bytes),
        });
    });

    await page.goto("/flow");
    const grid = page.getByTestId("flow-grid");
    await expect(grid).toBeVisible();
    await expectUnpinnedOrder(grid, ["3", "4", "2"]);
    await expect(page.getByTestId("flow-sort-topic")).toHaveAttribute(
        "data-pressed",
        "",
    );
    expect(listCalls).toBe(1);
});
