import { describe, expect, test } from "bun:test";
import {
    INITIAL_ICON_PICKER_STATE,
    SET_TOPIC_ICON_PATH,
    SUGGEST_TOPIC_ICONS_PATH,
    applyTopicIcon,
    closeIconPicker,
    iconShortName,
    jsonTopicId,
    openIconPicker,
    parseSetTopicIcon,
    parseSuggestedIcons,
    pickFailed,
    pickSucceeded,
    rerollIconPicker,
    setTopicIcon,
    startPickingIcon,
    suggestTopicIcons,
    suggestionsFailed,
    suggestionsReceived,
    type IconPickerTopic,
} from "./topic-icon-picker";

const topic: IconPickerTopic = {
    id: 11n,
    name: "Rust",
    iconId: "lucide:book",
    topicColor: "#aabbcc",
};

describe("iconShortName", () => {
    test("uses the last Iconify path segment and replaces hyphens", () => {
        expect(iconShortName("lucide:book")).toBe("book");
        expect(iconShortName("tabler:brand-rust")).toBe("brand rust");
        expect(iconShortName("no-colon")).toBe("no colon");
    });
});

describe("jsonTopicId", () => {
    test("accepts a positive safe integer", () => {
        expect(jsonTopicId(11n)).toBe(11);
    });

    test("rejects zero, negative, and oversized ids", () => {
        expect(() => jsonTopicId(0n)).toThrow(/positive/);
        expect(() => jsonTopicId(-1n)).toThrow(/positive/);
        expect(() => jsonTopicId(BigInt(Number.MAX_SAFE_INTEGER) + 1n)).toThrow(
            /JSON integer range/,
        );
    });
});

describe("dashboard JSON parsers", () => {
    test("reads the suggest-icons array", () => {
        expect(
            parseSuggestedIcons({
                icons: ["lucide:book", "lucide:brain"],
                ignored: true,
            }),
        ).toEqual(["lucide:book", "lucide:brain"]);
    });

    test("rejects a missing or mistyped icons array", () => {
        expect(() => parseSuggestedIcons({ icon: "lucide:book" })).toThrow(
            /invalid JSON/,
        );
        expect(() =>
            parseSuggestedIcons({ icons: ["", "lucide:book"] }),
        ).toThrow(/invalid icon id/);
    });

    test("reads the set-icon id", () => {
        expect(parseSetTopicIcon({ icon_id: "lucide:flame" })).toBe(
            "lucide:flame",
        );
        expect(() => parseSetTopicIcon({ iconId: "lucide:flame" })).toThrow(
            /invalid JSON/,
        );
        expect(() => parseSetTopicIcon({ icon_id: "  " })).toThrow(
            /invalid JSON/,
        );
    });
});

describe("picker transitions", () => {
    test("open claims a generation and starts suggesting with no exclusions", () => {
        const opened = openIconPicker(INITIAL_ICON_PICKER_STATE, topic);
        expect(opened).toEqual({
            kind: "suggesting",
            topic,
            request: { topicId: 11n, generation: 1 },
            excludedIcons: [],
        });
    });

    test("stale suggestion results do not replace a newer request", () => {
        const opened = openIconPicker(INITIAL_ICON_PICKER_STATE, topic);
        const rerolled = openIconPicker(opened, topic);
        expect(
            suggestionsReceived(rerolled, opened.request, ["lucide:book"]),
        ).toEqual(rerolled);
        expect(
            suggestionsReceived(rerolled, rerolled.request, ["lucide:brain"]),
        ).toMatchObject({
            kind: "ready",
            suggestions: ["lucide:brain"],
        });
    });

    test("empty suggestion lists become the empty kind", () => {
        const opened = openIconPicker(INITIAL_ICON_PICKER_STATE, topic);
        expect(suggestionsReceived(opened, opened.request, [])).toEqual({
            kind: "empty",
            topic,
            request: opened.request,
        });
    });

    test("reroll from ready excludes the currently shown icons", () => {
        const opened = openIconPicker(INITIAL_ICON_PICKER_STATE, topic);
        const ready = suggestionsReceived(opened, opened.request, [
            "lucide:book",
            "lucide:brain",
        ]);
        expect(rerollIconPicker(ready)).toEqual({
            kind: "suggesting",
            topic,
            request: { topicId: 11n, generation: 2 },
            excludedIcons: ["lucide:book", "lucide:brain"],
        });
    });

    test("close invalidates in-flight suggestions", () => {
        const opened = openIconPicker(INITIAL_ICON_PICKER_STATE, topic);
        const closed = closeIconPicker(opened);
        expect(closed).toEqual({ kind: "closed", generation: 2 });
        expect(
            suggestionsReceived(closed, opened.request, ["lucide:book"]),
        ).toEqual(closed);
    });

    test("pick success closes; pick failure returns to the same suggestions", () => {
        const opened = openIconPicker(INITIAL_ICON_PICKER_STATE, topic);
        const ready = suggestionsReceived(opened, opened.request, [
            "lucide:flame",
        ]);
        const picking = startPickingIcon(ready, "lucide:flame");
        expect(picking).toMatchObject({
            kind: "picking",
            iconId: "lucide:flame",
        });
        expect(pickSucceeded(picking, opened.request)).toEqual({
            kind: "closed",
            generation: 2,
        });
        expect(pickFailed(picking, opened.request, "denied")).toMatchObject({
            kind: "ready",
            suggestions: ["lucide:flame"],
            error: "denied",
        });
    });

    test("suggestion errors stay on the same request so reroll can retry", () => {
        const opened = openIconPicker(INITIAL_ICON_PICKER_STATE, topic);
        const failed = suggestionsFailed(opened, opened.request, "offline");
        expect(failed).toEqual({
            kind: "suggestError",
            topic,
            request: opened.request,
            message: "offline",
        });
        expect(rerollIconPicker(failed)).toMatchObject({
            kind: "suggesting",
            excludedIcons: [],
        });
    });
});

describe("applyTopicIcon", () => {
    test("patches only the matching topic", () => {
        const topics = [
            { id: 11n, iconId: "lucide:book", name: "Rust" },
            { id: 12n, iconId: "lucide:brain", name: "Go" },
        ];
        expect(applyTopicIcon(topics, 11n, "lucide:flame")).toEqual([
            { id: 11n, iconId: "lucide:flame", name: "Rust" },
            { id: 12n, iconId: "lucide:brain", name: "Go" },
        ]);
    });
});

describe("session JSON client", () => {
    test("suggest-icons posts the topic id and exclusions with cookies", async () => {
        let captured: { url: string; init?: RequestInit } | undefined;
        const fetchImpl: typeof fetch = async (input, init) => {
            captured = { url: String(input), init };
            return Response.json({
                icons: ["lucide:book", "lucide:brain"],
            });
        };
        await expect(
            suggestTopicIcons({
                id: 11n,
                excludedIcons: ["lucide:tag"],
                fetchImpl,
            }),
        ).resolves.toEqual(["lucide:book", "lucide:brain"]);
        expect(captured?.url).toBe(SUGGEST_TOPIC_ICONS_PATH);
        expect(captured?.init?.method).toBe("POST");
        expect(captured?.init?.credentials).toBe("same-origin");
        expect(JSON.parse(String(captured?.init?.body))).toEqual({
            id: 11,
            excluded_icons: ["lucide:tag"],
        });
    });

    test("set-icon posts icon_id and returns the persisted id", async () => {
        let captured: { url: string; init?: RequestInit } | undefined;
        const fetchImpl: typeof fetch = async (input, init) => {
            captured = { url: String(input), init };
            return Response.json({ icon_id: "lucide:flame" });
        };
        await expect(
            setTopicIcon({ id: 11n, iconId: "lucide:flame", fetchImpl }),
        ).resolves.toBe("lucide:flame");
        expect(captured?.url).toBe(SET_TOPIC_ICON_PATH);
        expect(JSON.parse(String(captured?.init?.body))).toEqual({
            id: 11,
            icon_id: "lucide:flame",
        });
    });

    test("non-OK bodies become Error messages", async () => {
        const fetchImpl: typeof fetch = async () =>
            new Response("unknown icon", { status: 400 });
        await expect(
            setTopicIcon({ id: 11n, iconId: "nope", fetchImpl }),
        ).rejects.toThrow("unknown icon");
    });
});
