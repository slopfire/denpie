import { expect, test } from "@playwright/test";

test("image viewer closes on empty stage click and zooms with the wheel", async ({
    page,
}) => {
    await page.goto("/lab-cards");

    await page
        .getByRole("button", {
            name: "Open image 1 for Prepositions of movement",
        })
        .click();

    const dialog = page.getByTestId("image-lightbox");
    await expect(dialog).toBeVisible();
    await expect(page.getByTestId("image-lightbox-zoom")).toHaveText("100%");

    const stage = page.getByTestId("image-lightbox-stage");
    const header = page.getByTestId("image-lightbox-header");
    const picture = dialog.getByRole("img", {
        name: "Illustration for Prepositions of movement",
    });
    await expect(header).toHaveCSS("position", "absolute");
    await expect(stage).toHaveCSS("position", "absolute");
    await expect
        .poll(async () => {
            const imageBox = await picture.boundingBox();
            return imageBox?.height ?? 0;
        })
        .toBeGreaterThan(100);
    const headerBox = await header.boundingBox();
    const imageBox = await picture.boundingBox();
    if (headerBox === null || imageBox === null) {
        throw new TypeError("lightbox bounds unavailable");
    }
    expect(imageBox.y).toBeLessThan(headerBox.y + headerBox.height);
    expect(imageBox.y + imageBox.height).toBeGreaterThan(
        headerBox.y + headerBox.height,
    );
    const box = await stage.boundingBox();
    if (box === null) throw new TypeError("lightbox stage bounds unavailable");

    await page.mouse.move(
        imageBox.x + imageBox.width / 2,
        imageBox.y + imageBox.height / 2,
    );
    await page.mouse.wheel(0, -480);
    await expect(page.getByTestId("image-lightbox-zoom")).not.toHaveText(
        "100%",
    );

    await page.getByRole("button", { name: "Reset image view" }).click();
    await expect(page.getByTestId("image-lightbox-zoom")).toHaveText("100%");

    await picture.click();
    await expect(dialog).toBeVisible();

    await page.mouse.click(box.x + 16, box.y + 16);
    await expect(dialog).toHaveCount(0);
});
