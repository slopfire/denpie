import { expect, test } from "@playwright/test";

const TEST_USER = "test";
const TEST_PASSWORD = "23452345";

test("every authenticated route renders its page in the shared shell", async ({
    page,
}) => {
    await page.goto("/");
    await page.getByLabel("Username").fill(TEST_USER);
    await page.getByLabel("Password").fill(TEST_PASSWORD);
    await page.getByTestId("login-submit").click();
    await expect(page.locator("#auth-session")).toBeVisible();

    for (const [path, heading, testId] of [
        ["/grounding", "Grounding", "grounding-page"],
        ["/settings", "Settings", null],
        ["/keys", "API Keys", "api-keys-page"],
        ["/archive", "Archive", "archive-page"],
        ["/account", "Account Settings", null],
    ] as const) {
        await page.goto(path);
        await expect(
            page.getByRole("heading", { name: heading, exact: true }),
        ).toBeVisible();
        await expect(page.getByTestId("desktop-sidebar")).toBeVisible();
        if (testId !== null) {
            await expect(page.getByTestId(testId)).toBeVisible();
        }
        if (path === "/grounding") {
            await expect(
                page.getByRole("heading", { name: "Overview", exact: true }),
            ).toBeVisible();
            await expect(
                page.getByRole("heading", { name: "Topics", exact: true }),
            ).toBeVisible();
            await expect(
                page.getByRole("heading", { name: "Sources", exact: true }),
            ).toBeVisible();
            await expect(
                page.getByRole("heading", {
                    name: "Image pool",
                    exact: true,
                }),
            ).toBeVisible();
            await expect(page.locator('[data-slot="tabs-list"]')).toHaveCount(
                0,
            );
        }
        if (path === "/settings") {
            for (const section of [
                "LLM and compression",
                "Grounding, search, and vision",
                "Daily refresh and limits",
                "Appearance",
                "Server self-updates",
            ]) {
                await expect(
                    page.getByRole("region", { name: section, exact: true }),
                ).toBeVisible();
            }
            await expect(page.locator('[data-slot="tabs-list"]')).toHaveCount(
                0,
            );
        }
    }

    await expect(page.getByLabel("Display name")).toBeVisible();
    await expect(
        page.getByRole("button", { name: "Add Passkey" }),
    ).toBeVisible();
    await expect(
        page.getByRole("button", { name: "Delete account" }),
    ).toBeVisible();
});
