import { describe, expect, test } from "bun:test";
import { PERSISTED_APP_VIEWS, nextMountedViews } from "./keep-alive";

describe("nextMountedViews", () => {
    test("keeps Flow after leaving it and drops Archive", () => {
        const afterFlow = nextMountedViews(
            new Set<string>(),
            "flow",
            PERSISTED_APP_VIEWS,
        );
        expect([...afterFlow]).toEqual(["flow"]);

        const afterArchive = nextMountedViews(
            afterFlow,
            "archive",
            PERSISTED_APP_VIEWS,
        );
        expect(afterArchive.has("flow")).toBe(true);
        expect(afterArchive.has("archive")).toBe(true);

        const afterSettings = nextMountedViews(
            afterArchive,
            "settings",
            PERSISTED_APP_VIEWS,
        );
        expect([...afterSettings].sort()).toEqual(["flow", "settings"]);
        expect(afterSettings.has("archive")).toBe(false);
    });

    test("does not mount Flow until it is visited", () => {
        const mounted = nextMountedViews(
            new Set<string>(),
            "archive",
            PERSISTED_APP_VIEWS,
        );
        expect([...mounted]).toEqual(["archive"]);
    });
});
