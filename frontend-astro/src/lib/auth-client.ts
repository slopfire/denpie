/**
 * Auth HTTP client (same-origin cookies).
 *
 * Thin transport only: login/logout success bodies are empty; `/auth/me` is
 * parsed via `parseSessionUser`. `fetch` is injectable so transitions are
 * testable without a live server. No DOM/window access — Bun-safe.
 */
import {
    parseSessionUser,
    type SessionState,
    type SessionUser,
} from "./auth-types";
import {
    parsePasskeyLoginChallenge,
    parsePasskeyRegistrationChallenge,
    type PasskeyAssertion,
    type PasskeyRegistration,
} from "./webauthn-client";

/** Result of an auth client call, mapped to the UI's session union. */
export type AuthResult =
    | { ok: true; user: SessionUser }
    | {
          ok: false;
          reason: "unauthorized" | "network" | "invalid-response";
          message: string;
      };

export type LogoutResult = { ok: true } | { ok: false; message: string };

export type SetupInput = {
    readonly username: string;
    readonly password: string;
    readonly setupToken: string;
};

export type PasskeyLoginStartResult =
    | { readonly kind: "challenge"; readonly request: CredentialRequestOptions }
    | {
          readonly kind: "error";
          readonly reason: "network" | "invalid-response";
          readonly message: string;
      };

export interface PasskeyInfo {
    readonly id: string;
    readonly name: string;
}

export type PasskeyListResult =
    | { readonly kind: "passkeys"; readonly passkeys: readonly PasskeyInfo[] }
    | { readonly kind: "error"; readonly message: string };

export type PasskeyRegistrationStartResult =
    | {
          readonly kind: "challenge";
          readonly request: CredentialCreationOptions;
      }
    | {
          readonly kind: "error";
          readonly reason: "network" | "invalid-response";
          readonly message: string;
      };

export type PasskeyMutationResult =
    | { readonly kind: "success" }
    | { readonly kind: "error"; readonly message: string };

export interface ProfileUpdateInput {
    readonly displayName?: string;
    readonly avatarData?: string;
    readonly password?: string;
}

export type ProfileUpdateResult =
    | { readonly kind: "updated"; readonly user: SessionUser }
    | { readonly kind: "error"; readonly message: string };

export interface AuthClient {
    /** GET /auth/me */
    fetchMe(): Promise<AuthResult>;
    /** POST /auth/login with JSON {username, password}; empty success body. */
    login(username: string, password: string): Promise<AuthResult>;
    /** POST /auth/setup with JSON {username, password, admin_token}; then GET /auth/me. */
    setup(input: SetupInput): Promise<AuthResult>;
    /** POST /auth/passkeys/login/start and parse its WebAuthn challenge. */
    startPasskeyLogin(): Promise<PasskeyLoginStartResult>;
    /** POST /auth/passkeys/login/finish with an assertion; then GET /auth/me. */
    finishPasskeyLogin(assertion: PasskeyAssertion): Promise<AuthResult>;
    /** GET the current account's registered passkeys. */
    listPasskeys(): Promise<PasskeyListResult>;
    /** POST /auth/passkeys/register/start and parse its WebAuthn creation challenge. */
    startPasskeyRegistration(): Promise<PasskeyRegistrationStartResult>;
    /** POST a WebAuthn credential to /auth/passkeys/register/finish. */
    finishPasskeyRegistration(
        registration: PasskeyRegistration,
    ): Promise<PasskeyMutationResult>;
    /** DELETE one registered passkey by its server-issued base64url ID. */
    deletePasskey(id: string): Promise<PasskeyMutationResult>;
    /** PATCH the current account profile and parse the authoritative user. */
    updateProfile(input: ProfileUpdateInput): Promise<ProfileUpdateResult>;
    /** DELETE the current account and its session. */
    deleteAccount(): Promise<PasskeyMutationResult>;
    /** POST /auth/logout; empty success body. */
    logout(): Promise<LogoutResult>;
}

export interface AuthClientOptions {
    fetchImpl?: typeof fetch;
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function createAuthClient(options: AuthClientOptions = {}): AuthClient {
    const doFetch = options.fetchImpl ?? fetch;

    function networkMessage(error: unknown): string {
        return error instanceof Error
            ? error.message
            : "Network request failed";
    }

    async function responseMessage(response: Response): Promise<string> {
        const message = await response.text();
        return (
            message.trim() || `Request failed with status ${response.status}`
        );
    }

    async function readEmptySuccess(response: Response): Promise<boolean> {
        if (!response.ok) return false;
        // Success bodies are empty; anything non-empty here is a contract break.
        const text = await response.text();
        return text.trim() === "";
    }

    function parsePasskeyList(value: unknown): readonly PasskeyInfo[] | null {
        if (!Array.isArray(value)) return null;

        const passkeys: PasskeyInfo[] = [];
        for (const item of value) {
            if (
                !isRecord(item) ||
                typeof item.id !== "string" ||
                typeof item.name !== "string"
            ) {
                return null;
            }
            passkeys.push({ id: item.id, name: item.name });
        }
        return passkeys;
    }

    return {
        async fetchMe(): Promise<AuthResult> {
            try {
                const response = await doFetch("/auth/me", {
                    method: "GET",
                    credentials: "same-origin",
                    headers: { accept: "application/json" },
                });
                if (response.status === 401 || response.status === 403) {
                    return {
                        ok: false,
                        reason: "unauthorized",
                        message: "Not signed in",
                    };
                }
                if (!response.ok) {
                    return {
                        ok: false,
                        reason: "network",
                        message: `/auth/me failed with status ${response.status}`,
                    };
                }
                try {
                    return {
                        ok: true,
                        user: parseSessionUser(await response.json()),
                    };
                } catch (error) {
                    // Body arrived but was not valid JSON or not the /auth/me shape.
                    return {
                        ok: false,
                        reason: "invalid-response",
                        message:
                            error instanceof Error
                                ? `/auth/me returned an invalid body: ${error.message}`
                                : "/auth/me returned an invalid body",
                    };
                }
            } catch (error) {
                return {
                    ok: false,
                    reason: "network",
                    message: networkMessage(error),
                };
            }
        },

        async login(username, password): Promise<AuthResult> {
            try {
                const response = await doFetch("/auth/login", {
                    method: "POST",
                    credentials: "same-origin",
                    headers: { "content-type": "application/json" },
                    body: JSON.stringify({ username, password }),
                });
                if (!response.ok) {
                    return {
                        ok: false,
                        reason:
                            response.status === 401
                                ? "unauthorized"
                                : "network",
                        message:
                            response.status === 401
                                ? "Invalid username or password"
                                : `Login failed with status ${response.status}`,
                    };
                }
                if (!(await readEmptySuccess(response))) {
                    return {
                        ok: false,
                        reason: "invalid-response",
                        message: "Login returned an unexpected body",
                    };
                }
                // Login body is empty: resolve the session with /auth/me.
                return await this.fetchMe();
            } catch (error) {
                return {
                    ok: false,
                    reason: "network",
                    message: networkMessage(error),
                };
            }
        },

        async setup({ username, password, setupToken }): Promise<AuthResult> {
            try {
                const response = await doFetch("/auth/setup", {
                    method: "POST",
                    credentials: "same-origin",
                    headers: { "content-type": "application/json" },
                    body: JSON.stringify({
                        username,
                        password,
                        admin_token: setupToken,
                    }),
                });
                if (!response.ok) {
                    return {
                        ok: false,
                        reason: "network",
                        message: await responseMessage(response),
                    };
                }

                try {
                    parseSessionUser(await response.json());
                } catch (error) {
                    return {
                        ok: false,
                        reason: "invalid-response",
                        message:
                            error instanceof Error
                                ? `/auth/setup returned an invalid body: ${error.message}`
                                : "/auth/setup returned an invalid body",
                    };
                }
                return await this.fetchMe();
            } catch (error) {
                return {
                    ok: false,
                    reason: "network",
                    message: networkMessage(error),
                };
            }
        },

        async startPasskeyLogin(): Promise<PasskeyLoginStartResult> {
            try {
                const response = await doFetch("/auth/passkeys/login/start", {
                    method: "POST",
                    credentials: "same-origin",
                });
                if (!response.ok) {
                    return {
                        kind: "error",
                        reason: "network",
                        message: await responseMessage(response),
                    };
                }

                let body: unknown;
                try {
                    body = await response.json();
                } catch {
                    return {
                        kind: "error",
                        reason: "invalid-response",
                        message: "Passkey login returned an invalid challenge",
                    };
                }
                const challenge = parsePasskeyLoginChallenge(body);
                return challenge.kind === "valid"
                    ? { kind: "challenge", request: challenge.request }
                    : {
                          kind: "error",
                          reason: "invalid-response",
                          message: challenge.message,
                      };
            } catch (error) {
                return {
                    kind: "error",
                    reason: "network",
                    message: networkMessage(error),
                };
            }
        },

        async finishPasskeyLogin(
            assertion: PasskeyAssertion,
        ): Promise<AuthResult> {
            try {
                const response = await doFetch("/auth/passkeys/login/finish", {
                    method: "POST",
                    credentials: "same-origin",
                    headers: { "content-type": "application/json" },
                    body: JSON.stringify(assertion),
                });
                if (!response.ok) {
                    return {
                        ok: false,
                        reason: "network",
                        message: await responseMessage(response),
                    };
                }
                if (!(await readEmptySuccess(response))) {
                    return {
                        ok: false,
                        reason: "invalid-response",
                        message: "Passkey login returned an unexpected body",
                    };
                }
                return await this.fetchMe();
            } catch (error) {
                return {
                    ok: false,
                    reason: "network",
                    message: networkMessage(error),
                };
            }
        },

        async listPasskeys(): Promise<PasskeyListResult> {
            try {
                const response = await doFetch("/auth/passkeys", {
                    method: "GET",
                    credentials: "same-origin",
                    headers: { accept: "application/json" },
                });
                if (!response.ok) {
                    return {
                        kind: "error",
                        message: await responseMessage(response),
                    };
                }

                let body: unknown;
                try {
                    body = await response.json();
                } catch {
                    return {
                        kind: "error",
                        message: "Passkey list returned an invalid body",
                    };
                }
                const passkeys = parsePasskeyList(body);
                return passkeys === null
                    ? {
                          kind: "error",
                          message: "Passkey list returned an invalid body",
                      }
                    : { kind: "passkeys", passkeys };
            } catch (error) {
                return { kind: "error", message: networkMessage(error) };
            }
        },

        async startPasskeyRegistration(): Promise<PasskeyRegistrationStartResult> {
            try {
                const response = await doFetch(
                    "/auth/passkeys/register/start",
                    {
                        method: "POST",
                        credentials: "same-origin",
                    },
                );
                if (!response.ok) {
                    return {
                        kind: "error",
                        reason: "network",
                        message: await responseMessage(response),
                    };
                }

                let body: unknown;
                try {
                    body = await response.json();
                } catch {
                    return {
                        kind: "error",
                        reason: "invalid-response",
                        message:
                            "Passkey registration returned an invalid challenge",
                    };
                }
                const challenge = parsePasskeyRegistrationChallenge(body);
                return challenge.kind === "valid"
                    ? { kind: "challenge", request: challenge.request }
                    : {
                          kind: "error",
                          reason: "invalid-response",
                          message: challenge.message,
                      };
            } catch (error) {
                return {
                    kind: "error",
                    reason: "network",
                    message: networkMessage(error),
                };
            }
        },

        async finishPasskeyRegistration(
            registration: PasskeyRegistration,
        ): Promise<PasskeyMutationResult> {
            try {
                const response = await doFetch(
                    "/auth/passkeys/register/finish",
                    {
                        method: "POST",
                        credentials: "same-origin",
                        headers: { "content-type": "application/json" },
                        body: JSON.stringify(registration),
                    },
                );
                if (!response.ok) {
                    return {
                        kind: "error",
                        message: await responseMessage(response),
                    };
                }
                return (await readEmptySuccess(response))
                    ? { kind: "success" }
                    : {
                          kind: "error",
                          message:
                              "Passkey registration returned an unexpected body",
                      };
            } catch (error) {
                return { kind: "error", message: networkMessage(error) };
            }
        },

        async deletePasskey(id: string): Promise<PasskeyMutationResult> {
            try {
                const response = await doFetch(
                    `/auth/passkeys/${encodeURIComponent(id)}`,
                    {
                        method: "DELETE",
                        credentials: "same-origin",
                    },
                );
                if (!response.ok) {
                    return {
                        kind: "error",
                        message: await responseMessage(response),
                    };
                }
                return (await readEmptySuccess(response))
                    ? { kind: "success" }
                    : {
                          kind: "error",
                          message:
                              "Passkey deletion returned an unexpected body",
                      };
            } catch (error) {
                return { kind: "error", message: networkMessage(error) };
            }
        },

        async updateProfile(input): Promise<ProfileUpdateResult> {
            try {
                const response = await doFetch("/auth/me", {
                    method: "PATCH",
                    credentials: "same-origin",
                    headers: {
                        accept: "application/json",
                        "content-type": "application/json",
                    },
                    body: JSON.stringify({
                        display_name: input.displayName,
                        avatar_data: input.avatarData,
                        password: input.password,
                    }),
                });
                if (!response.ok) {
                    return {
                        kind: "error",
                        message: await responseMessage(response),
                    };
                }
                try {
                    return {
                        kind: "updated",
                        user: parseSessionUser(await response.json()),
                    };
                } catch (error) {
                    return {
                        kind: "error",
                        message:
                            error instanceof Error
                                ? `Profile update returned an invalid body: ${error.message}`
                                : "Profile update returned an invalid body",
                    };
                }
            } catch (error) {
                return { kind: "error", message: networkMessage(error) };
            }
        },

        async deleteAccount(): Promise<PasskeyMutationResult> {
            try {
                const response = await doFetch("/auth/me", {
                    method: "DELETE",
                    credentials: "same-origin",
                });
                if (!response.ok) {
                    return {
                        kind: "error",
                        message: await responseMessage(response),
                    };
                }
                return (await readEmptySuccess(response))
                    ? { kind: "success" }
                    : {
                          kind: "error",
                          message:
                              "Account deletion returned an unexpected body",
                      };
            } catch (error) {
                return { kind: "error", message: networkMessage(error) };
            }
        },

        async logout(): Promise<{ ok: true } | { ok: false; message: string }> {
            try {
                const response = await doFetch("/auth/logout", {
                    method: "POST",
                    credentials: "same-origin",
                });
                if (!response.ok) {
                    return {
                        ok: false,
                        message: `Logout failed with status ${response.status}`,
                    };
                }
                if (!(await readEmptySuccess(response))) {
                    return {
                        ok: false,
                        message: "Logout returned an unexpected body",
                    };
                }
                return { ok: true };
            } catch (error) {
                return { ok: false, message: networkMessage(error) };
            }
        },
    };
}

/** Reduce an {@link AuthResult} into the renderable {@link SessionState}. */
export function toSessionState(result: AuthResult): SessionState {
    if (result.ok) {
        return { status: "authenticated", user: result.user };
    }
    if (result.reason === "unauthorized") {
        return { status: "guest" };
    }
    return { status: "error", message: result.message };
}

/**
 * Pure transition for a logout attempt from any {@link SessionState}.
 * Success drops to guest; failure keeps an authenticated user visible with
 * the exact failure surfaced as a recoverable notice.
 */
export function applyLogout(
    state: SessionState,
    result: { ok: true } | { ok: false; message: string },
): SessionState {
    if (result.ok) {
        return { status: "guest" };
    }
    return state.status === "authenticated"
        ? { ...state, notice: result.message }
        : { status: "error", message: result.message };
}
