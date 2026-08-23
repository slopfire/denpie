import { describe, expect, test } from "bun:test";
import {
    TOPIC_ICON_DATA,
    TOPIC_ICON_FALLBACK,
    TOPIC_ICON_IDS,
    lookupTopicIconData,
    lookupTopicIconId,
    normalizeTopicIconId,
} from "./topic-icons.generated";

describe("generated topic icons", () => {
    test("normalizes only the boundary string", () => {
        expect(normalizeTopicIconId("  lucide:book ")).toBe("lucide:book");
        expect(normalizeTopicIconId(" ")).toBe("");
        expect(normalizeTopicIconId(null)).toBe("");
        expect(normalizeTopicIconId(42)).toBe("");
    });

    test("falls back for empty and unknown IDs", () => {
        expect(lookupTopicIconId("")).toBe(TOPIC_ICON_FALLBACK);
        expect(lookupTopicIconId("  unknown:icon ")).toBe(TOPIC_ICON_FALLBACK);
        expect(lookupTopicIconId(undefined)).toBe(TOPIC_ICON_FALLBACK);
        expect(lookupTopicIconData("unknown:icon")).toEqual(
            TOPIC_ICON_DATA[TOPIC_ICON_FALLBACK],
        );
    });

    test("keeps every configured ID in the generated offline subset", () => {
        expect(TOPIC_ICON_IDS).toHaveLength(76);
        for (const id of TOPIC_ICON_IDS) {
            expect(TOPIC_ICON_DATA[id]).toBeDefined();
        }
        expect(TOPIC_ICON_DATA[TOPIC_ICON_FALLBACK]).toBeDefined();
    });

    test("returns the configured ID after trimming", () => {
        expect(lookupTopicIconId("  tabler:brand-rust")).toBe(
            "tabler:brand-rust",
        );
        expect(lookupTopicIconData("lucide:book").body).toContain("<path");
        expect(lookupTopicIconData("radix-icons:backpack")).toMatchObject({
            width: 15,
            height: 15,
        });
        expect(lookupTopicIconData("mdi:language-python")).toMatchObject({
            width: 24,
            height: 24,
        });
    });
});
