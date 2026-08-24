import { expect, test } from "@playwright/test";

test("review workbench loads real run artifacts and exports structured judgments", async ({
    page,
}) => {
    await page.goto("/lab-review");
    await expect(
        page.getByRole("heading", { name: "Baseline and candidate review" }),
    ).toBeVisible();
    await expect(page.getByText("Ownership before")).toBeAttached();
    await expect(page.getByText("Ownership after")).toBeAttached();
    await expect(page.getByRole("heading", { name: "A", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "B", exact: true })).toBeVisible();

    await page.getByTestId("lab-verdict-overall-tie").click();
    await expect(page.getByTestId("lab-verdict-overall-tie")).toHaveAttribute(
        "aria-pressed",
        "true",
    );
    await page.getByTestId("lab-verdict-correctness-b").click();
    await page.getByTestId("lab-review-note").fill("The candidate is more concrete.");
    await page.getByRole("button", { name: "Reveal labels" }).click();
    await expect(page.getByRole("heading", { name: "before" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "after" })).toBeVisible();

    const downloadPromise = page.waitForEvent("download");
    await page.getByRole("button", { name: "Export review.json" }).click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe("review.json");
    const stream = await download.createReadStream();
    let contents = "";
    for await (const chunk of stream) contents += chunk.toString();
    const review: unknown = JSON.parse(contents);
    expect(review).toMatchObject({
        version: 1,
        judgments: [
            {
                key: "rust/1",
                note: "The candidate is more concrete.",
                dimensions: { overall: "tie" },
            },
        ],
    });
});
