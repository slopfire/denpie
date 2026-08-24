import { describe, expect, test } from "bun:test";
import {
    DEFAULT_LAB_GALLERY_SETTINGS,
    labGallerySearch,
    matchesLabFixture,
    parseLabGallerySettings,
} from "./lab-gallery-controls";

describe("lab gallery controls", () => {
    test("parses only canonical query values and serializes a stable link", () => {
        const settings = parseLabGallerySettings(
            "?fixture=error&layout=list&columns=4&viewport=mobile&theme=light",
        );
        expect(settings).toEqual({
            filter: "error",
            layout: "list",
            columns: 4,
            viewport: "mobile",
            theme: "light",
        });
        expect(labGallerySearch(settings)).toBe(
            "?fixture=error&layout=list&columns=4&viewport=mobile&theme=light",
        );
        expect(parseLabGallerySettings("?layout=table&columns=99")).toEqual(
            DEFAULT_LAB_GALLERY_SETTINGS,
        );
    });

    test("filters fixture metadata case-insensitively", () => {
        expect(
            matchesLabFixture("RUST", [
                "llm-error",
                "Rust",
                "Generation failed",
            ]),
        ).toBeTrue();
        expect(matchesLabFixture("api key", ["api-key-missing"])).toBeTrue();
        expect(
            matchesLabFixture("image", ["active", "English Grammar"]),
        ).toBeFalse();
    });
});
