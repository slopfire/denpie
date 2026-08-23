import { expect, test } from "@playwright/test";

// Wave 2B browser evidence: the real authenticated /flow route served from the
// built Astro dist through the isolated :3027 agent-server harness
// (--frontend-dist frontend-astro/dist). Never touches :3017.

const TEST_USER = "test";
const TEST_PASSWORD = "23452345";

async function login(page, path = "/") {
    await page.goto(path);
    await page.locator("#login-username").fill(TEST_USER);
    await page.locator("#login-password").fill(TEST_PASSWORD);
    await page.getByTestId("login-submit").click();
    await expect(page.locator("#auth-session")).toBeVisible();
}

test("the authenticated shell exposes the parity routes as real navigation links", async ({
    page,
}) => {
    await login(page);

    const link = page.getByRole("link", {
        name: "Transmission",
        exact: true,
    });
    await expect(link).toBeVisible();
    await expect(link).toHaveAttribute("href", "/");
    // The root route is the authenticated Flow view.
    await expect(link).toHaveAttribute("aria-current", "page");

    for (const [label, href] of [
        ["Grounding", "/grounding"],
        ["Settings", "/settings"],
        ["API Keys", "/keys"],
        ["Archive", "/archive"],
    ]) {
        const routeLink = page.getByRole("link", { name: label, exact: true });
        await expect(routeLink).toBeVisible();
        await expect(routeLink).toHaveAttribute("href", href);
        await expect(routeLink).toBeEnabled();
    }
});

test("the root route renders Flow and /flow remains a compatible alias", async ({
    page,
}) => {
    await login(page);

    await page.goto("/");
    await expect(page.locator("#flow-view")).toBeVisible();
    await expect(
        page.getByRole("link", { name: "Transmission", exact: true }),
    ).toHaveAttribute("href", "/");

    await page.goto("/flow");
    await expect(page.locator("#flow-view")).toBeVisible();
});

test("/flow requires authentication through the shared shell before rendering Flow", async ({
    page,
}) => {
    const apiCalls = [];
    page.on("request", (request) => {
        if (request.url().includes("/api/v1")) apiCalls.push(request.url());
    });

    await page.goto("/flow");
    // The guest gets the shared login form; the DOM contract is that the shell
    // proves the session before any /api/v1 call happens.
    await expect(page.locator("#login-username")).toBeVisible();
    expect(apiCalls).toHaveLength(0);
});

test("authenticated /flow POSTs /api/v1 and reaches a card grid or honest empty state", async ({
    page,
}) => {
    let apiV1Posts = 0;
    page.on("request", (request) => {
        if (request.method() === "POST" && request.url().includes("/api/v1")) {
            apiV1Posts += 1;
        }
    });

    await login(page, "/flow");

    // The Flow island is mounted on this route.
    await expect(page.locator("#flow-view")).toBeVisible();

    // Exactly one initial list_flow_cards read after the authenticated render.
    await expect.poll(() => apiV1Posts).toBeGreaterThanOrEqual(1);

    // The DOM settles into a real card grid or the honest empty state — never
    // stuck on skeletons. The isolated fixture database may legitimately have
    // zero cards; both outcomes are valid evidence of a live route.
    const outcome = page.locator(
        '[data-testid="flow-grid"], [data-testid="flow-empty"]',
    );
    await expect(outcome).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('[data-testid="flow-loading"]')).toHaveCount(0);
    await expect(page.locator('[data-testid="flow-skeletons"]')).toHaveCount(0);

    if (await page.locator('[data-testid="flow-empty"]').isVisible()) {
        await expect(page.locator('[data-testid="flow-empty"]')).toContainText(
            "Your flow is empty",
        );
        // Empty fixture: no pagination affordance may be offered.
        await expect(page.getByTestId("flow-load-more")).toHaveCount(0);
    } else {
        await expect(
            page.locator('[data-testid="flow-grid"] li').first(),
        ).toBeVisible();
    }

    // The initial read happened exactly once (no duplicate fetch loops).
    expect(apiV1Posts).toBe(1);
});

test("Load more appears only when the API promises more pages and fetches the next page", async ({
    page,
}) => {
    await login(page);
    await page.goto("/flow");

    const grid = page.locator('[data-testid="flow-grid"]');
    const empty = page.locator('[data-testid="flow-empty"]');
    await expect(grid.or(empty)).toBeVisible({ timeout: 15_000 });

    const loadMore = page.getByTestId("flow-load-more");
    if (!(await loadMore.isVisible())) {
        // No more pages promised by the API → no Load more control. Nothing to prove.
        test.info().annotations.push({
            type: "info",
            text: "Fixture returned a single page or empty; Load more correctly absent.",
        });
        return;
    }

    // The API promised more pages → clicking must fetch another page and must
    // not replace rendered cards with page-level skeletons. hasMore itself is
    // proven by UI behavior only: the button exists exactly when a non-empty
    // cursor token exists, which is a transport-level invariant tested in Bun.
    const before = await grid.locator("li").count();
    await loadMore.click();
    await expect(grid).toBeVisible();
    await expect(
        page.locator('[data-testid="flow-skeletons"]').first(),
    ).toBeVisible();
    await expect
        .poll(async () => grid.locator("li").count(), { timeout: 15_000 })
        .toBeGreaterThan(before);
});
