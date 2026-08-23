import { describe, expect, test } from "bun:test";
import {
    createAdminUser,
    fetchAutoupdateStatus,
    fetchTokenSpend,
    isAutoupdateActive,
    parseAdminUsers,
    parseAutoupdateStatus,
    parseTokenSpend,
    triggerAutoupdate,
} from "./dashboard-session";

describe("dashboard session JSON", () => {
    test("parses admin users with nullable display names", () => {
        expect(
            parseAdminUsers([
                {
                    id: "u-1",
                    username: "alice",
                    role: "admin",
                    display_name: "Alice",
                    created_at: "2026-01-01T00:00:00Z",
                },
                {
                    id: "u-2",
                    username: "bob",
                    role: "user",
                    display_name: null,
                    created_at: "2026-01-02T00:00:00Z",
                },
            ]),
        ).toEqual([
            {
                id: "u-1",
                username: "alice",
                role: "admin",
                displayName: "Alice",
                createdAt: "2026-01-01T00:00:00Z",
            },
            {
                id: "u-2",
                username: "bob",
                role: "user",
                displayName: null,
                createdAt: "2026-01-02T00:00:00Z",
            },
        ]);
    });

    test("parses token spend counters", () => {
        expect(parseTokenSpend({ daily: 1, monthly: 2, total: 3 })).toEqual({
            daily: 1,
            monthly: 2,
            total: 3,
        });
    });

    test("treats unknown empty autoupdate status as absent", async () => {
        const fetchImpl: typeof fetch = async () =>
            Response.json({
                phase: "unknown",
                message: "",
                target_sha: "",
                updated_at: "",
            });
        await expect(fetchAutoupdateStatus({ fetchImpl })).resolves.toBeNull();
        expect(
            isAutoupdateActive(
                parseAutoupdateStatus({
                    phase: "compiling",
                    message: "Building",
                    target_sha: "abc",
                    updated_at: "now",
                }),
            ),
        ).toBe(true);
    });

    test("posts JSON to the admin user and autoupdate endpoints", async () => {
        const calls: Array<{ url: string; method?: string }> = [];
        const fetchImpl: typeof fetch = async (input, init) => {
            calls.push({ url: String(input), method: init?.method });
            if (String(input) === "/admin/users") {
                return Response.json({
                    id: "u-3",
                    username: "cara",
                    role: "user",
                    display_name: null,
                    created_at: "now",
                });
            }
            return Response.json({
                message: "Checking",
                restarting: false,
                updating: true,
                target_sha: "deadbeef",
                build_sha: "abc",
            });
        };
        const created = await createAdminUser({
            username: "cara",
            password: "23452345",
            role: "user",
            displayName: "",
            fetchImpl,
        });
        expect(created.username).toBe("cara");
        const triggered = await triggerAutoupdate({ fetchImpl });
        expect(triggered.updating).toBe(true);
        expect(calls.map((call) => `${call.method} ${call.url}`)).toEqual([
            "POST /admin/users",
            "POST /admin/autoupdate",
        ]);
    });

    test("reads token spend from the session JSON route", async () => {
        const fetchImpl: typeof fetch = async (input) => {
            expect(String(input)).toBe("/admin/token-spend");
            return Response.json({ daily: 10, monthly: 20, total: 30 });
        };
        await expect(fetchTokenSpend({ fetchImpl })).resolves.toEqual({
            daily: 10,
            monthly: 20,
            total: 30,
        });
    });
});
