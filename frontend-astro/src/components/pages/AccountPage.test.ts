import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { AccountPage } from "./AccountPage";
import type {
    AuthClient,
    PasskeyListResult,
    PasskeyMutationResult,
    PasskeyRegistrationStartResult,
} from "@/lib/auth-client";
import type { SessionUser } from "@/lib/auth-types";

const user: SessionUser = {
    id: "usr_1",
    username: "alice",
    role: "user",
    display_name: "Alice",
    avatar_data: null,
    build_sha: "abc123",
};

const emptyPasskeys: PasskeyListResult = { kind: "passkeys", passkeys: [] };
const successfulMutation: PasskeyMutationResult = { kind: "success" };
const unsupportedRegistration: PasskeyRegistrationStartResult = {
    kind: "error",
    reason: "network",
    message: "not used during server rendering",
};

const authClient: AuthClient = {
    async fetchMe() {
        return { ok: true, user };
    },
    async login() {
        return { ok: true, user };
    },
    async setup() {
        return { ok: true, user };
    },
    async startPasskeyLogin() {
        return {
            kind: "error",
            reason: "network",
            message: "not used during server rendering",
        };
    },
    async finishPasskeyLogin() {
        return { ok: true, user };
    },
    async listPasskeys() {
        return emptyPasskeys;
    },
    async startPasskeyRegistration() {
        return unsupportedRegistration;
    },
    async finishPasskeyRegistration() {
        return successfulMutation;
    },
    async deletePasskey() {
        return successfulMutation;
    },
    async updateProfile() {
        return { kind: "updated", user };
    },
    async deleteAccount() {
        return successfulMutation;
    },
    async logout() {
        return { ok: true };
    },
};

describe("AccountPage", () => {
    test("renders the authenticated identity and an honest passkey-empty state", () => {
        const markup = renderToStaticMarkup(
            createElement(AccountPage, { user, authClient }),
        );

        expect(markup).toContain("Account Settings");
        expect(markup).toContain("Alice");
        expect(markup).toContain("Display name");
        expect(markup).toContain("Delete account");
        expect(markup).toContain("Passkeys");
        expect(markup).toContain("Add Passkey");
        expect(markup).toContain("Loading passkeys");
    });
});
