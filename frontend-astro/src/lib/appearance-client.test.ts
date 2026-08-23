import { describe, expect, test } from "bun:test";
import {
    fetchAppearancePreferences,
    parseAppearancePreferences,
} from "./appearance-client";

describe("appearance settings boundary", () => {
    test("projects the legacy settings JSON into the three global preferences", () => {
        expect(
            parseAppearancePreferences({
                color_scheme: "carbonfox",
                transparency: "medium",
                blur_intensity: "full",
                ignored: true,
            }),
        ).toEqual({
            theme: "carbonfox",
            transparency: "medium",
            blur: "full",
        });
        expect(() => parseAppearancePreferences({ color_scheme: 7 })).toThrow(
            /invalid appearance/,
        );
    });

    test("uses the authenticated JSON endpoint with same-origin cookies", async () => {
        let captured: { url: string; init?: RequestInit } | undefined;
        const fetchImpl: typeof fetch = async (input, init) => {
            captured = { url: String(input), init };
            return Response.json({
                color_scheme: "slate",
                transparency: "low",
                blur_intensity: "medium",
            });
        };
        await expect(fetchAppearancePreferences(fetchImpl)).resolves.toEqual({
            theme: "slate",
            transparency: "low",
            blur: "medium",
        });
        expect(captured?.url).toBe("/admin/settings");
        expect(captured?.init?.credentials).toBe("same-origin");
    });
});
