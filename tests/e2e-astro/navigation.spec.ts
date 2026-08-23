import { expect, test } from "@playwright/test";

const TEST_USER = "test";
const TEST_PASSWORD = "23452345";

test("authenticated navigation keeps the document and session mounted", async ({
    page,
}) => {
    let sessionReads = 0;
    page.on("request", (request) => {
        if (new URL(request.url()).pathname === "/auth/me") sessionReads += 1;
    });

    await page.goto("/");
    await page.getByLabel("Username").fill(TEST_USER);
    await page.getByLabel("Password").fill(TEST_PASSWORD);
    await page.getByTestId("login-submit").click();
    await expect(page.locator("#auth-session")).toBeVisible();

    const initialTimeOrigin = await page.evaluate(() => {
        Object.assign(window, { denpieNavigationMarker: "still-mounted" });
        return performance.timeOrigin;
    });
    const initialSessionReads = sessionReads;

    await page.getByRole("link", { name: "Settings", exact: true }).click();
    await expect(page).toHaveURL(/\/settings$/);
    await expect(
        page.getByRole("heading", { name: "Settings", exact: true }),
    ).toBeVisible();
    await expect(page.getByText("Checking session…")).toHaveCount(0);
    expect(sessionReads).toBe(initialSessionReads);
    expect(
        await page.evaluate(
            () =>
                Reflect.get(window, "denpieNavigationMarker") ===
                    "still-mounted" && performance.timeOrigin,
        ),
    ).toBe(initialTimeOrigin);

    await page.evaluate(() => window.history.back());
    await expect(page).toHaveURL(/\/$/);
    await expect(
        page.getByRole("heading", { name: "Transmission", exact: true }),
    ).toBeVisible();
    expect(sessionReads).toBe(initialSessionReads);
});
