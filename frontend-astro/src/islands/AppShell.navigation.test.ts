import { describe, expect, test } from "bun:test";
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
});
