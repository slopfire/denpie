import { mkdir } from "node:fs/promises";
import path from "node:path";
import { expect, test, type Page } from "@playwright/test";

const fixtureIds = [
    "active",
    "pinned",
    "reviewed-hold",
    "await-refill",
    "daily-complete",
    "stacked",
    "llm-error",
    "long-markdown",
    "two-images",
    "three-images",
    "broken-image",
    "api-key-missing",
    "manual-tip",
    "custom-tip",
    "casual-tip",
] as const;

async function openLab(page: Page) {
    await page.goto("/lab-cards");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
        "Production flow-card fixtures",
    );
    await expect(page.getByTestId("lab-fixture-active")).toBeVisible();
}

test("@smoke fixtures hydrate and local card actions work", async ({
    page,
}) => {
    await openLab(page);

    for (const id of fixtureIds) {
        await expect(page.getByTestId(`lab-fixture-${id}`)).toHaveCount(1);
    }

    const active = page.getByTestId("lab-fixture-active");
    const titleBar = active.getByTestId("card-title-bar-1");
    await expect(titleBar).toBeVisible();
    const titlePad = await titleBar.evaluate((el) => {
        const style = getComputedStyle(el);
        return { top: style.paddingTop, bottom: style.paddingBottom };
    });
    expect(titlePad.top).toBe(titlePad.bottom);
    const newChip = active.getByRole("button", {
        name: "Show information for card 1",
    });
    const expand = active.getByTestId("detail-open-1");
    const chipBox = await newChip.boundingBox();
    const expandBox = await expand.boundingBox();
    expect(chipBox).not.toBeNull();
    expect(expandBox).not.toBeNull();
    expect(expandBox?.height).toBe(chipBox?.height);
    await active
        .getByRole("button", { name: "Expand text for card 1" })
        .click();
    await expect(
        active.getByRole("button", { name: "Show compact text for card 1" }),
    ).toBeVisible();

    await active.getByTestId("detail-open-1").click();
    const detail = page.getByTestId("card-detail-fullscreen");
    await expect(detail).toBeVisible();
    await detail.getByRole("button", { name: /close/i }).click();
    await expect(detail).toHaveCount(0);

    await active.getByTestId("pin-1").click();
    await expect(active.getByTestId("pin-1")).toHaveAttribute(
        "aria-pressed",
        "true",
    );
    await expect(active.getByTestId("card-pinned-1")).toBeVisible();

    await active.getByTestId("review-learned-1").click();
    await expect(active.getByTestId("review-completed-1")).toBeVisible();
    await active.getByTestId("continue-1").click();
    await expect(active.getByTestId("review-completed-1")).toHaveCount(0);

    const removable = page.getByTestId("lab-fixture-casual-tip");
    await removable.getByTestId("card-more-15").click();
    await page.getByTestId("delete-card-15").click();
    await page.getByTestId("delete-confirm-15").click();
    await expect(removable).toHaveCount(0);
});

test("@smoke polish fixtures survive narrow layout and error expansion", async ({
    page,
}) => {
    await page.setViewportSize({ width: 360, height: 800 });
    await openLab(page);

    const markdown = page.getByTestId("lab-fixture-long-markdown");
    await expect(markdown.getByRole("table")).toBeVisible();
    await expect(
        markdown.getByText("borrow checker", { exact: false }),
    ).toBeVisible();

    const apiError = page.getByTestId("lab-fixture-api-key-missing");
    await expect(apiError.getByTestId(/card-error-/)).toHaveAttribute(
        "data-card-error",
        "api-key",
    );
    await apiError.getByRole("button", { name: "Show full error" }).click();
    await expect(
        apiError.getByText("OPENAI_API_KEY", { exact: false }),
    ).toBeVisible();

    const documentWidth = await page.evaluate(
        () => document.documentElement.scrollWidth,
    );
    expect(documentWidth).toBeLessThanOrEqual(360);
});

test("@smoke polish controls filter, resize, theme, and preserve a shareable query", async ({
    page,
}) => {
    await openLab(page);

    await page.getByTestId("lab-filter").fill("api key");
    await expect(page.getByTestId("lab-fixture-api-key-missing")).toBeVisible();
    await expect(page.getByTestId("lab-fixture-active")).toHaveCount(0);

    await page.getByTestId("lab-layout").selectOption("list");
    await page.getByTestId("lab-viewport").selectOption("mobile");
    await page.getByTestId("lab-theme").selectOption("light");
    const grid = page.getByTestId("lab-gallery-grid");
    await expect(grid).toHaveAttribute("data-layout", "list");
    await expect(grid).toHaveAttribute("data-viewport", "mobile");
    await expect(page.locator("html")).not.toHaveClass(/dark/);
    await expect(page).toHaveURL(/fixture=api\+key/);
    await expect(page).toHaveURL(/layout=list/);
    await expect(page).toHaveURL(/viewport=mobile/);
    await expect(page).toHaveURL(/theme=light/);

    await page.reload();
    await expect(page.getByTestId("lab-filter")).toHaveValue("api key");
    await expect(grid).toHaveAttribute("data-layout", "list");

    await page.getByTestId("lab-reset").click();
    await expect(page.getByTestId("lab-fixture-active")).toBeVisible();
    await expect(page).toHaveURL(/\/lab-cards\/?$/);
});

test("@screenshot deterministic responsive theme matrix", async ({ page }) => {
    test.skip(
        process.env.LAB_CARD_SCREENSHOT_DIR === undefined,
        "run `just lab-cards-shot` to capture the matrix",
    );

    const outputDir = path.resolve(process.env.LAB_CARD_SCREENSHOT_DIR!);
    await mkdir(outputDir, { recursive: true });

    for (const theme of ["light", "dark"] as const) {
        for (const viewport of [
            { name: "mobile", width: 360, height: 800 },
            { name: "tablet", width: 768, height: 900 },
            { name: "desktop", width: 1440, height: 1000 },
        ] as const) {
            await page.setViewportSize(viewport);
            await openLab(page);
            await page.evaluate((selectedTheme) => {
                document.documentElement.classList.toggle(
                    "dark",
                    selectedTheme === "dark",
                );
                document.documentElement.dataset.theme =
                    selectedTheme === "dark" ? "shadcn" : "shadcn-light";
            }, theme);
            await page.emulateMedia({
                colorScheme: theme,
                reducedMotion: "reduce",
            });
            await page.evaluate(() => document.fonts.ready);
            await page.screenshot({
                path: path.join(
                    outputDir,
                    `${theme}-${viewport.name}-${viewport.width}.png`,
                ),
                fullPage: true,
                animations: "disabled",
            });
        }
    }
});
