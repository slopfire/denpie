import { describe, expect, test } from "bun:test";
import { PERSISTED_APP_VIEWS, nextMountedViews } from "@/lib/keep-alive";
import { viewForPathname } from "./AppShell";

describe("AppShell in-app routes", () => {
    test("maps every static authenticated route, including trailing slashes", () => {
        expect(viewForPathname("/")).toBe("flow");
        expect(viewForPathname("/flow")).toBe("flow");
        expect(viewForPathname("/grounding/")).toBe("grounding");
        expect(viewForPathname("/settings")).toBe("settings");
        expect(viewForPathname("/keys")).toBe("keys");
        expect(viewForPathname("/archive")).toBe("archive");
        expect(viewForPathname("/account")).toBe("account");
    });

    test("does not claim unknown paths as in-app routes", () => {
        expect(viewForPathname("/not-a-view")).toBeNull();
    });

    test("keeps Flow mounted after archive and drops archive on leave", () => {
        const afterArchive = nextMountedViews(
            new Set(["flow"]),
            "archive",
            PERSISTED_APP_VIEWS,
        );
        expect(afterArchive.has("flow")).toBe(true);
        expect(afterArchive.has("archive")).toBe(true);
        const afterLeave = nextMountedViews(
            afterArchive,
            "grounding",
            PERSISTED_APP_VIEWS,
        );
        expect(afterLeave.has("archive")).toBe(false);
        expect(afterLeave.has("flow")).toBe(true);
        expect(afterLeave.has("grounding")).toBe(true);
    });
});
