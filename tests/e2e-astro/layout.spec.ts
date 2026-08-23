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

const ABOVE_SAFE = 9_007_199_254_740_993n;

test.beforeEach(async ({ page }) => {
  await page.route("**/auth/me", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        id: "layout-fixture",
        username: "test",
        role: "user",
        display_name: "Layout fixture",
        avatar_data: null,
        build_sha: "playwright",
      }),
    });
  });
});

function layoutCard(id: bigint, title: string, pinned = false) {
  return create(FlowCardInfoSchema, {
    id,
    title,
    topicName: `Topic ${title}`,
    fullContent: `${title} body`,
    tipcardType: "repeatable_tip",
    status: "active",
    createdAt: `2026-08-${String(id % 20n).padStart(2, "0")}T00:00:00Z`,
    pinned,
  });
}

function layoutCards() {
  return [
    layoutCard(11n, "Alpha"),
    layoutCard(12n, "Beta"),
    layoutCard(13n, "Gamma"),
    layoutCard(14n, "Delta"),
    layoutCard(1n, "Pinned one", true),
    layoutCard(ABOVE_SAFE, "Pinned exact bigint", true),
    layoutCard(3n, "Pinned new", true),
  ];
}

async function installListFixture(page: import("@playwright/test").Page) {
  let listCalls = 0;
  await page.route("**/api/v1", async (route) => {
    const bytes = route.request().postDataBuffer();
    if (bytes === null) throw new TypeError("missing protobuf request body");
    const envelope = fromBinary(ApiV1RequestSchema, bytes);
    if (envelope.call.op.case !== "listFlowCards") {
      throw new TypeError(
        `unexpected API operation ${String(envelope.call.op.case)}`,
      );
    }
    listCalls += 1;
    const response = create(ApiResponseSchema, {
      result: {
        case: "flowCardPage",
        value: create(FlowCardPageSchema, {
          cards: layoutCards(),
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
            requestId: envelope.requestId,
            outcome: { case: "success", value: response },
          }),
        ),
      ),
    });
  });
  return () => listCalls;
}

async function cardPositions(list: Locator) {
  const positions: Array<{ x: number; y: number }> = [];
  for (const card of await list.locator(":scope > li").all()) {
    const box = await card.boundingBox();
    if (box === null) throw new TypeError("rendered card has no bounding box");
    positions.push({ x: Math.round(box.x), y: Math.round(box.y) });
  }
  return positions;
}

async function expectPinnedOrder(list: Locator, ids: readonly string[]) {
  await expect
    .poll(() =>
      list.evaluate((element) =>
        Array.from(element.children).map(
          (item) => item.getAttribute("data-testid") ?? "",
        ),
      ),
    )
    .toEqual(ids.map((id) => `flow-slot-${id}`));
}

async function visualOrder(list: Locator) {
  const cards: Array<{ id: string; x: number; y: number }> = [];
  for (const card of await list.locator(":scope > li").all()) {
    const box = await card.boundingBox();
    if (box === null) throw new TypeError("rendered card has no bounding box");
    cards.push({
      id: (await card.getAttribute("data-testid")) ?? "",
      x: box.x,
      y: box.y,
    });
  }
  return cards
    .sort((left, right) => left.y - right.y || left.x - right.x)
    .map(({ id }) => id);
}

test("grid ceiling, responsive reduction, list mode, and persistence render without refetch", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1600, height: 900 });
  const listCalls = await installListFixture(page);
  await page.goto("/flow");

  const grid = page.getByTestId("flow-grid");
  await expect(grid.locator(":scope > li")).toHaveCount(4);
  await expect(page.getByTestId("flow-grid-btn")).toHaveAttribute(
    "data-pressed",
    "",
  );
  const wide = await cardPositions(grid);
  expect(new Set(wide.map(({ y }) => y)).size).toBe(1);
  expect(new Set(wide.map(({ x }) => x)).size).toBe(4);

  await page.setViewportSize({ width: 900, height: 900 });
  const medium = await cardPositions(grid);
  expect(medium[0]?.y).toBe(medium[1]?.y);
  expect(medium[2]?.y).toBe(medium[3]?.y);
  expect(medium[2]?.y).toBeGreaterThan(medium[0]?.y ?? 0);
  expect(new Set(medium.map(({ x }) => x)).size).toBe(2);

  await page.getByTestId("flow-list-btn").click();
  await expect(page.getByTestId("flow-list-btn")).toHaveAttribute(
    "data-pressed",
    "",
  );
  const stacked = await cardPositions(grid);
  expect(new Set(stacked.map(({ x }) => x)).size).toBe(1);
  expect(new Set(stacked.map(({ y }) => y)).size).toBe(4);
  expect(listCalls()).toBe(1);

  await page.reload();
  await expect(page.getByTestId("flow-list-btn")).toHaveAttribute(
    "data-pressed",
    "",
  );
  expect(new Set((await cardPositions(grid)).map(({ x }) => x)).size).toBe(1);
  expect(listCalls()).toBe(2);

  await page.getByTestId("flow-grid-btn").click();
  await page.getByTestId("flow-columns-2").click();
  await page.setViewportSize({ width: 1600, height: 900 });
  const twoColumns = await cardPositions(grid);
  expect(twoColumns[0]?.y).toBe(twoColumns[1]?.y);
  expect(twoColumns[2]?.y).toBe(twoColumns[3]?.y);
  expect(twoColumns[2]?.y).toBeGreaterThan(twoColumns[0]?.y ?? 0);
  expect(new Set(twoColumns.map(({ x }) => x)).size).toBe(2);
  expect(listCalls()).toBe(2);
  expect(
    await page.evaluate(() => ({
      layout: window.localStorage.getItem("denpie-flow-layout"),
      columns: window.localStorage.getItem("denpie-flow-grid-columns"),
    })),
  ).toEqual({ layout: "grid", columns: "2" });
});

test("numeric pinned order survives exact-bigint handle drag and reload", async ({
  page,
}) => {
  await page.addInitScript((aboveSafe) => {
    if (window.localStorage.getItem("denpie-pinned-card-order") === null) {
      window.localStorage.setItem(
        "denpie-pinned-card-order",
        `[${aboveSafe},1]`,
      );
    }
  }, String(ABOVE_SAFE));
  const listCalls = await installListFixture(page);
  await page.goto("/flow");

  const pinned = page.getByTestId("flow-pinned-grid");
  await expectPinnedOrder(pinned, [String(ABOVE_SAFE), "1", "3"]);
  await expect(pinned.getByTestId(/^pinned-drag-/)).toHaveCount(3);
  await expect(page.getByTestId("flow-grid").getByTestId(/^pinned-drag-/)).toHaveCount(
    0,
  );
  await expect(
    page.getByTestId(`pinned-drag-${ABOVE_SAFE}`),
  ).toHaveAttribute("draggable", "true");

  const sourceBox = await page
    .getByTestId(`pinned-drag-${ABOVE_SAFE}`)
    .boundingBox();
  const targetBox = await pinned.getByTestId("flow-slot-1").boundingBox();
  if (sourceBox === null || targetBox === null) {
    throw new TypeError("drag source or target has no bounding box");
  }
  await page.mouse.move(
    sourceBox.x + sourceBox.width / 2,
    sourceBox.y + sourceBox.height / 2,
  );
  await page.mouse.down();
  await page.mouse.move(
    sourceBox.x + sourceBox.width / 2 + 24,
    sourceBox.y + sourceBox.height / 2,
    { steps: 4 },
  );
  await expect(
    page.getByTestId(`pinned-drag-${ABOVE_SAFE}`),
  ).toHaveAttribute("aria-pressed", "true");
  await page.mouse.move(
    targetBox.x + targetBox.width / 2,
    targetBox.y + targetBox.height / 2,
    { steps: 12 },
  );
  await page.mouse.up();
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.localStorage.getItem("denpie-pinned-card-order"),
      ),
    )
    .toBe(`[1,${ABOVE_SAFE},3]`);
  await expect
    .poll(() => visualOrder(pinned))
    .toEqual([
      "flow-slot-1",
      `flow-slot-${ABOVE_SAFE}`,
      "flow-slot-3",
    ]);
  expect(listCalls()).toBe(1);

  await page.reload();
  await expectPinnedOrder(pinned, ["1", String(ABOVE_SAFE), "3"]);
  expect(listCalls()).toBe(2);
});

test("native scrollbars follow the ScrollArea thumb", async ({ page }) => {
  await installListFixture(page);
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto("/flow");
  await expect(page.getByTestId("flow-grid")).toBeVisible();
  const desktop = await page.evaluate(() => {
    const html = getComputedStyle(document.documentElement);
    const probe = document.createElement("div");
    probe.style.backgroundColor = "var(--border)";
    document.body.append(probe);
    const borderRgb = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return {
      width: html.scrollbarWidth,
      color: html.scrollbarColor,
      gutter: html.scrollbarGutter,
      borderRgb,
    };
  });
  expect(desktop.width).toBe("thin");
  expect(desktop.gutter).toBe("stable");
  expect(desktop.color.startsWith(desktop.borderRgb)).toBe(true);
  expect(desktop.color).toMatch(/rgba\(\s*0,\s*0,\s*0,\s*0\s*\)/);

  await page.setViewportSize({ width: 390, height: 500 });
  const mobile = await page.evaluate(() => {
    const main = document.querySelector("[data-testid=app-main]");
    if (!(main instanceof HTMLElement)) {
      throw new TypeError("app-main missing");
    }
    const styles = getComputedStyle(main);
    return {
      gutter: styles.scrollbarGutter,
      overflowY: styles.overflowY,
      scrolls: main.scrollHeight > main.clientHeight,
    };
  });
  expect(mobile.gutter).toBe("stable");
  expect(mobile.overflowY).toBe("auto");
  expect(mobile.scrolls).toBe(true);
});
