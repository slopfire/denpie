import { describe, expect, test } from "bun:test";
import {
  parsePasskeyLoginChallenge,
  parsePasskeyRegistrationChallenge,
  requestPasskeyAssertion,
  requestPasskeyRegistration,
  type PasskeyCredentialCreator,
  type PasskeyCredentialGetter,
} from "./webauthn-client";

function bytes(...values: number[]): ArrayBuffer {
  return new Uint8Array(values).buffer;
}

describe("parsePasskeyLoginChallenge", () => {
  test("converts the server's base64url fields into WebAuthn request buffers", () => {
    const parsed = parsePasskeyLoginChallenge({
      publicKey: {
        challenge: "AQID",
        timeout: 30_000,
        rpId: "denpie.example",
        userVerification: "preferred",
        allowCredentials: [
          { id: "BAU", type: "public-key", transports: ["internal", "usb"] },
        ],
      },
    });

    expect(parsed.kind).toBe("valid");
    if (parsed.kind !== "valid" || parsed.request.publicKey === undefined) {
      throw new Error("expected a parsed public key request");
    }
    expect([...new Uint8Array(parsed.request.publicKey.challenge)]).toEqual([1, 2, 3]);
    expect(parsed.request.publicKey.rpId).toBe("denpie.example");
    expect(parsed.request.publicKey.allowCredentials).toHaveLength(1);
    expect([...new Uint8Array(parsed.request.publicKey.allowCredentials?.[0]?.id)]).toEqual([4, 5]);
  });

  test("rejects malformed binary fields instead of passing them to WebAuthn", () => {
    expect(
      parsePasskeyLoginChallenge({ publicKey: { challenge: "not+base64" } }),
    ).toEqual({ kind: "invalid", message: "Passkey login challenge has invalid binary data" });
  });
});

describe("requestPasskeyAssertion", () => {
  const request: CredentialRequestOptions = { publicKey: { challenge: bytes(1, 2, 3) } };

  test("serializes a credential into the Axum passkey-finish wire shape", async () => {
    const credentialGetter: PasskeyCredentialGetter = {
      async get() {
        return {
          id: "BAU",
          rawId: bytes(4, 5),
          type: "public-key",
          response: {
            authenticatorData: bytes(6),
            clientDataJSON: bytes(7, 8),
            signature: bytes(9),
            userHandle: null,
          },
        };
      },
    };

    await expect(requestPasskeyAssertion({ credentialGetter, request })).resolves.toEqual({
      kind: "assertion",
      assertion: {
        id: "BAU",
        rawId: "BAU",
        type: "public-key",
        response: {
          authenticatorData: "Bg",
          clientDataJSON: "Bwg",
          signature: "CQ",
          userHandle: null,
        },
      },
    });
  });

  test("treats a null browser result as a cancelled ceremony", async () => {
    const credentialGetter: PasskeyCredentialGetter = { async get() { return null; } };

    await expect(requestPasskeyAssertion({ credentialGetter, request })).resolves.toEqual({
      kind: "cancelled",
      message: "No passkey was selected",
    });
  });
});

describe("passkey registration", () => {
  test("converts registration challenge fields and serializes the created credential", async () => {
    const parsed = parsePasskeyRegistrationChallenge({
      publicKey: {
        challenge: "AQID",
        rp: { id: "denpie.example", name: "Denpie" },
        user: { id: "BAU", name: "alice", displayName: "Alice" },
        pubKeyCredParams: [{ type: "public-key", alg: -7 }],
        authenticatorSelection: { residentKey: "required", requireResidentKey: true },
      },
    });
    expect(parsed.kind).toBe("valid");
    if (parsed.kind !== "valid" || parsed.request.publicKey === undefined) {
      throw new Error("expected a parsed registration request");
    }
    expect([...new Uint8Array(parsed.request.publicKey.challenge)]).toEqual([1, 2, 3]);
    expect([...new Uint8Array(parsed.request.publicKey.user.id)]).toEqual([4, 5]);
    expect(parsed.request.publicKey.authenticatorSelection?.residentKey).toBe("required");
    expect(parsed.request.publicKey.authenticatorSelection?.requireResidentKey).toBe(true);

    const credentialCreator: PasskeyCredentialCreator = {
      async create() {
        return {
          id: "BAU",
          rawId: bytes(4, 5),
          type: "public-key",
          response: {
            attestationObject: bytes(6),
            clientDataJSON: bytes(7, 8),
          },
        };
      },
    };
    await expect(
      requestPasskeyRegistration({ credentialCreator, request: parsed.request }),
    ).resolves.toEqual({
      kind: "registration",
      registration: {
        id: "BAU",
        rawId: "BAU",
        type: "public-key",
        response: { attestationObject: "Bg", clientDataJSON: "Bwg" },
      },
    });
  });

  test("rejects a registration challenge missing its required public-key parameters", () => {
    expect(
      parsePasskeyRegistrationChallenge({
        publicKey: {
          challenge: "AQID",
          rp: { name: "Denpie" },
          user: { id: "BAU", name: "alice", displayName: "Alice" },
        },
      }),
    ).toEqual({
      kind: "invalid",
      message: "Passkey registration challenge has invalid binary data",
    });
  });
});
