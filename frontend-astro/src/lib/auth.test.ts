import { describe, expect, test } from "bun:test";
import { applyLogout, createAuthClient, toSessionState } from "./auth-client";
import { parseSessionUser, type SessionUser } from "./auth-types";
import type { PasskeyAssertion, PasskeyRegistration } from "./webauthn-client";

const validUser: SessionUser = {
    id: "u-1",
    username: "alice",
    role: "user",
    display_name: "Alice",
    avatar_data: null,
    build_sha: "abc123",
};

function jsonResponse(body: unknown, status = 200): Response {
    return new Response(body === undefined ? "" : JSON.stringify(body), {
        status,
        headers: { "content-type": "application/json" },
    });
}

function emptyResponse(status = 200): Response {
    return new Response("", { status });
}

describe("parseSessionUser", () => {
    test("accepts the exact /auth/me shape", () => {
        expect(parseSessionUser({ ...validUser })).toEqual(validUser);
    });

    test("rejects non-object bodies", () => {
        expect(() => parseSessionUser(null)).toThrow();
        expect(() => parseSessionUser([validUser])).toThrow();
        expect(() => parseSessionUser("nope")).toThrow();
    });

    test("rejects missing or mistyped fields", () => {
        const { build_sha: _dropped, ...missing } = validUser;
        expect(() => parseSessionUser(missing)).toThrow();
        expect(() => parseSessionUser({ ...validUser, id: 42 })).toThrow();
        expect(() =>
            parseSessionUser({ ...validUser, display_name: 7 }),
        ).toThrow();
    });

    test("rejects unexpected extra fields (strict wire contract)", () => {
        expect(() =>
            parseSessionUser({ ...validUser, is_admin: true }),
        ).toThrow(/is_admin/);
    });
});

describe("auth client session transitions", () => {
    function clientWith(
        handler: (url: string, init?: RequestInit) => Response,
    ) {
        return createAuthClient({
            fetchImpl: (url, init) => handler(url, init) as Response,
        });
    }

    test("startup /auth/me 200 → authenticated with parsed user", async () => {
        let calledUrl = "";
        const client = clientWith((url) => {
            calledUrl = url;
            return jsonResponse(validUser);
        });
        const result = await client.fetchMe();
        expect(calledUrl).toBe("/auth/me");
        expect(result).toEqual({ ok: true, user: validUser });
        expect(toSessionState(result)).toEqual({
            status: "authenticated",
            user: validUser,
        });
    });

    test("startup /auth/me 401 → guest state, not error", async () => {
        const client = clientWith(() => jsonResponse({ error: "x" }, 401));
        const result = await client.fetchMe();
        expect(toSessionState(result)).toEqual({ status: "guest" });
    });

    test("startup network failure → error state with message", async () => {
        const client = createAuthClient({
            fetchImpl: () => Promise.reject(new Error("offline")),
        });
        const result = await client.fetchMe();
        expect(toSessionState(result)).toEqual({
            status: "error",
            message: "offline",
        });
    });

    test("startup invalid body → exact invalid-response reason", async () => {
        const client = clientWith(() => jsonResponse({ id: "only" }));
        const result = await client.fetchMe();
        expect(result).toEqual({
            ok: false,
            reason: "invalid-response",
            message: expect.stringMatching(/invalid body/),
        });
        expect(toSessionState(result).status).toBe("error");
    });

    test("non-JSON success body → invalid-response, not network", async () => {
        const client = clientWith(
            () => new Response("<html>oops</html>", { status: 200 }),
        );
        const result = await client.fetchMe();
        expect(result.ok === false && result.reason).toBe("invalid-response");
    });

    test("transport failure stays network even after a 200-shaped handler throws", async () => {
        const client = createAuthClient({
            fetchImpl: () => Promise.reject(new TypeError("Failed to fetch")),
        });
        const result = await client.fetchMe();
        expect(result.ok === false && result.reason).toBe("network");
    });

    test("all requests use same-origin credentials", async () => {
        const inits: RequestInit[] = [];
        const client = clientWith((url, init) => {
            if (init) inits.push(init);
            if (url === "/auth/login") return emptyResponse(401);
            return emptyResponse(401);
        });
        await client.fetchMe();
        await client.login("alice", "secret");
        await client.logout().catch(() => undefined);
        expect(inits.length).toBeGreaterThan(0);
        for (const init of inits) {
            expect(init.credentials).toBe("same-origin");
        }
    });

    test("guest login: POST JSON then GET /auth/me; empty success body required", async () => {
        const calls: Array<{ url: string; method?: string; body?: unknown }> =
            [];
        const client = clientWith((url, init) => {
            calls.push({
                url,
                method: init?.method,
                body:
                    typeof init?.body === "string"
                        ? JSON.parse(init.body)
                        : undefined,
            });
            if (url === "/auth/login") return emptyResponse();
            return jsonResponse(validUser);
        });
        const result = await client.login("alice", "secret");
        expect(calls.map((call) => call.url)).toEqual([
            "/auth/login",
            "/auth/me",
        ]);
        expect(calls[0].method).toBe("POST");
        expect(calls[0].body).toEqual({
            username: "alice",
            password: "secret",
        });
        expect(result).toEqual({ ok: true, user: validUser });
    });

    test("login rejects non-empty success body as contract break", async () => {
        const client = clientWith(() => jsonResponse({ token: "t" }));
        const result = await client.login("alice", "secret");
        expect(toSessionState(result).status).toBe("error");
    });

    test("login 401 maps to guest state with message available", async () => {
        const client = clientWith(() => emptyResponse(401));
        const state = toSessionState(await client.login("alice", "wrong"));
        expect(state).toEqual({ status: "guest" });
    });

    test("setup posts the exact setup-token payload, validates its JSON body, then refreshes the session", async () => {
        const calls: Array<{ url: string; method?: string; body?: unknown }> =
            [];
        const client = clientWith((url, init) => {
            calls.push({
                url,
                method: init?.method,
                body:
                    typeof init?.body === "string"
                        ? JSON.parse(init.body)
                        : undefined,
            });
            return jsonResponse(validUser);
        });

        await expect(
            client.setup({
                username: "alice",
                password: "at-least-eight",
                setupToken: "first-run-token",
            }),
        ).resolves.toEqual({ ok: true, user: validUser });
        expect(calls).toEqual([
            {
                url: "/auth/setup",
                method: "POST",
                body: {
                    username: "alice",
                    password: "at-least-eight",
                    admin_token: "first-run-token",
                },
            },
            { url: "/auth/me", method: "GET", body: undefined },
        ]);
    });

    test("setup preserves the server's token error body", async () => {
        const client = clientWith(
            () => new Response("Invalid setup token", { status: 401 }),
        );

        await expect(
            client.setup({
                username: "alice",
                password: "at-least-eight",
                setupToken: "wrong",
            }),
        ).resolves.toEqual({
            ok: false,
            reason: "network",
            message: "Invalid setup token",
        });
    });

    test("passkey start parses its challenge and finish posts the assertion before refreshing the session", async () => {
        const assertion: PasskeyAssertion = {
            id: "BAU",
            rawId: "BAU",
            type: "public-key",
            response: {
                authenticatorData: "Bg",
                clientDataJSON: "Bwg",
                signature: "CQ",
                userHandle: null,
            },
        };
        const calls: Array<{ url: string; method?: string; body?: unknown }> =
            [];
        const client = clientWith((url, init) => {
            calls.push({
                url,
                method: init?.method,
                body:
                    typeof init?.body === "string"
                        ? JSON.parse(init.body)
                        : undefined,
            });
            if (url === "/auth/passkeys/login/start") {
                return jsonResponse({ publicKey: { challenge: "AQID" } });
            }
            if (url === "/auth/passkeys/login/finish") return emptyResponse();
            return jsonResponse(validUser);
        });

        const started = await client.startPasskeyLogin();
        expect(started.kind).toBe("challenge");
        if (
            started.kind !== "challenge" ||
            started.request.publicKey === undefined
        ) {
            throw new Error("expected a parsed passkey challenge");
        }
        expect([
            ...new Uint8Array(started.request.publicKey.challenge),
        ]).toEqual([1, 2, 3]);
        await expect(client.finishPasskeyLogin(assertion)).resolves.toEqual({
            ok: true,
            user: validUser,
        });
        expect(calls.map((call) => call.url)).toEqual([
            "/auth/passkeys/login/start",
            "/auth/passkeys/login/finish",
            "/auth/me",
        ]);
        expect(calls[1]?.body).toEqual(assertion);
    });

    test("passkey start rejects a non-JSON challenge as an invalid response", async () => {
        const client = clientWith(
            () => new Response("not JSON", { status: 200 }),
        );

        await expect(client.startPasskeyLogin()).resolves.toEqual({
            kind: "error",
            reason: "invalid-response",
            message: "Passkey login returned an invalid challenge",
        });
    });

    test("passkey registration, list, and delete use their protected endpoint contracts", async () => {
        const registration: PasskeyRegistration = {
            id: "BAU",
            rawId: "BAU",
            type: "public-key",
            response: { attestationObject: "Bg", clientDataJSON: "Bwg" },
        };
        const calls: Array<{ url: string; method?: string; body?: unknown }> =
            [];
        const client = clientWith((url, init) => {
            calls.push({
                url,
                method: init?.method,
                body:
                    typeof init?.body === "string"
                        ? JSON.parse(init.body)
                        : undefined,
            });
            if (url === "/auth/passkeys/register/start") {
                return jsonResponse({
                    publicKey: {
                        challenge: "AQID",
                        rp: { name: "Denpie" },
                        user: {
                            id: "BAU",
                            name: "alice",
                            displayName: "Alice",
                        },
                        pubKeyCredParams: [{ type: "public-key", alg: -7 }],
                    },
                });
            }
            if (url === "/auth/passkeys")
                return jsonResponse([{ id: "BAU", name: "0405" }]);
            return emptyResponse();
        });

        const started = await client.startPasskeyRegistration();
        expect(started.kind).toBe("challenge");
        await expect(
            client.finishPasskeyRegistration(registration),
        ).resolves.toEqual({ kind: "success" });
        await expect(client.listPasskeys()).resolves.toEqual({
            kind: "passkeys",
            passkeys: [{ id: "BAU", name: "0405" }],
        });
        await expect(client.deletePasskey("BAU/with space")).resolves.toEqual({
            kind: "success",
        });
        expect(calls.map((call) => call.url)).toEqual([
            "/auth/passkeys/register/start",
            "/auth/passkeys/register/finish",
            "/auth/passkeys",
            "/auth/passkeys/BAU%2Fwith%20space",
        ]);
        expect(calls[1]?.body).toEqual(registration);
        expect(calls[3]?.method).toBe("DELETE");
    });

    test("profile update and account deletion use the authenticated account contract", async () => {
        const calls: Array<{ url: string; method?: string; body?: unknown }> =
            [];
        const updated = { ...validUser, display_name: "Ada", avatar_data: "" };
        const client = clientWith((url, init) => {
            calls.push({
                url,
                method: init?.method,
                body:
                    typeof init?.body === "string"
                        ? JSON.parse(init.body)
                        : undefined,
            });
            return init?.method === "PATCH"
                ? jsonResponse(updated)
                : emptyResponse();
        });

        await expect(
            client.updateProfile({
                displayName: "Ada",
                avatarData: "",
                password: "long-enough",
            }),
        ).resolves.toEqual({ kind: "updated", user: updated });
        await expect(client.deleteAccount()).resolves.toEqual({
            kind: "success",
        });
        expect(calls).toEqual([
            {
                url: "/auth/me",
                method: "PATCH",
                body: {
                    display_name: "Ada",
                    avatar_data: "",
                    password: "long-enough",
                },
            },
            { url: "/auth/me", method: "DELETE", body: undefined },
        ]);
    });

    test("authenticated logout posts and succeeds on empty body", async () => {
        let logoutMethod = "";
        const client = clientWith((url, init) => {
            if (url === "/auth/logout") {
                logoutMethod = init?.method ?? "";
                return emptyResponse();
            }
            throw new Error(`unexpected ${url}`);
        });
        const result = await client.logout();
        expect(logoutMethod).toBe("POST");
        expect(result).toEqual({ ok: true });
    });

    test("logout failure surfaces an error message", async () => {
        const client = clientWith(() => emptyResponse(500));
        const result = await client.logout();
        expect(result.ok).toBe(false);
        if (!result.ok) expect(result.message).toMatch(/500/);
    });

    test("applyLogout: failure keeps authenticated user visible with notice", () => {
        const authenticated = {
            status: "authenticated" as const,
            user: validUser,
        };
        const next = applyLogout(authenticated, {
            ok: false,
            message: "Logout failed with status 500",
        });
        expect(next.status).toBe("authenticated");
        if (next.status !== "authenticated") throw new Error("unreachable");
        expect(next.user).toEqual(validUser);
        expect(next.notice).toMatch(/500/);
    });

    test("applyLogout: success drops to guest from any state", () => {
        expect(applyLogout({ status: "guest" }, { ok: true })).toEqual({
            status: "guest",
        });
        expect(
            applyLogout(
                { status: "authenticated", user: validUser },
                { ok: true },
            ),
        ).toEqual({ status: "guest" });
    });

    test("applyLogout: failure outside authenticated maps to error", () => {
        expect(
            applyLogout({ status: "guest" }, { ok: false, message: "boom" }),
        ).toEqual({ status: "error", message: "boom" });
    });
});

describe("toSessionState mapping table", () => {
    test("covers all four union members deterministically", () => {
        expect(toSessionState({ ok: true, user: validUser }).status).toBe(
            "authenticated",
        );
        expect(
            toSessionState({ ok: false, reason: "unauthorized", message: "m" })
                .status,
        ).toBe("guest");
        expect(
            toSessionState({ ok: false, reason: "network", message: "m" })
                .status,
        ).toBe("error");
        expect(
            toSessionState({
                ok: false,
                reason: "invalid-response",
                message: "m",
            }).status,
        ).toBe("error");
    });
});
