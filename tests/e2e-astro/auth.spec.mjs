import { expect, test } from "@playwright/test";

// Browser evidence for the Astro auth shell served on the isolated :3027
// agent-server harness.

const TEST_USER = "test";
const TEST_PASSWORD = "23452345";

test("guest page shows visible labeled username and password fields", async ({
    page,
}) => {
    await page.goto("/");

    const username = page.getByLabel("Username");
    await expect(username).toBeVisible();
    await expect(username).toBeEditable();

    const password = page.getByLabel("Password");
    await expect(password).toBeVisible();
    await expect(password).toHaveAttribute("type", "password");

    await expect(page.getByLabel("Setup token")).toBeVisible();
    await expect(page.getByTestId("passkey-login")).toBeVisible();
});

test("invalid credentials show a persistent accessible error and can recover", async ({
    page,
}) => {
    await page.goto("/");

    await page.locator("#login-username").fill(TEST_USER);
    await page.locator("#login-password").fill("wrong-password");
    await page.getByTestId("login-submit").click();

    const error = page.locator("#auth-error");
    await expect(error).toBeVisible();
    await expect(error).toHaveAttribute("role", "alert");

    // Persistent: still visible after a settle delay (no auto-dismiss).
    await page.waitForTimeout(1000);
    await expect(error).toBeVisible();

    // The error stays inside the usable sign-in form; credentials can be
    // corrected without leaving the page or replacing the form.
    await expect(page.locator("#login-username")).toBeVisible();
    await expect(page.locator("#login-password")).toBeVisible();
    await expect(page.getByTestId("login-submit")).toBeEnabled();
});

test("valid login transitions to authenticated view without full navigation", async ({
    page,
}) => {
    await page.goto("/");

    let navigated = false;
    page.on("framenavigated", (frame) => {
        if (frame === page.mainFrame()) navigated = true;
    });

    await page.locator("#login-username").fill(TEST_USER);
    await page.locator("#login-password").fill(TEST_PASSWORD);
    await page.getByTestId("login-submit").click();

    const session = page.locator("#auth-session");
    await expect(session).toBeVisible();
    await expect(page.getByTestId("account-menu-btn")).toContainText(TEST_USER);
    expect(navigated).toBe(false);
    expect(
        await page.evaluate(
            () => performance.getEntriesByType("navigation").length,
        ),
    ).toBe(1);
});

test("logout returns to the guest form", async ({ page }) => {
    await page.goto("/");
    await page.locator("#login-username").fill(TEST_USER);
    await page.locator("#login-password").fill(TEST_PASSWORD);
    await page.getByTestId("login-submit").click();
    await expect(page.locator("#auth-session")).toBeVisible();

    await page.getByTestId("account-menu-btn").click();
    await page.getByTestId("logout-btn").click();

    await expect(page.locator("#auth-session")).toHaveCount(0);
    await expect(page.locator("#login-username")).toBeVisible();
});
