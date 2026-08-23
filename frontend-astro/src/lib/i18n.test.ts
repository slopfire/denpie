import { describe, expect, test } from "bun:test";

import { catalogFor, supportedLocales, t, tf } from "./i18n";

describe("Astro i18n", () => {
    test("loads the English catalog", () => {
        expect(supportedLocales).toEqual(["en"]);
        expect(t("nav.flow")).toBe("Transmission");
        expect(catalogFor()).toBe(catalogFor("en"));
    });

    test("formats every matching placeholder", () => {
        expect(tf("format.archive_card_count", { shown: 12, total: 48 })).toBe(
            "12 of 48 cards",
        );
    });
});
