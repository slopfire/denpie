/**
 * Browser-side WebAuthn boundary helpers.
 *
 * The server serializes binary WebAuthn fields as unpadded base64url strings.
 * Parse that external JSON before presenting it to `navigator.credentials`,
 * then serialize the browser credential back into the exact wire shape Axum
 * accepts.
 */

export type PasskeyLoginChallengeResult =
  | { readonly kind: "valid"; readonly request: CredentialRequestOptions }
  | { readonly kind: "invalid"; readonly message: string };

export type PasskeyRegistrationChallengeResult =
  | { readonly kind: "valid"; readonly request: CredentialCreationOptions }
  | { readonly kind: "invalid"; readonly message: string };

export interface PasskeyAssertion {
  readonly id: string;
  readonly rawId: string;
  readonly type: "public-key";
  readonly response: {
    readonly authenticatorData: string;
    readonly clientDataJSON: string;
    readonly signature: string;
    readonly userHandle: string | null;
  };
}

export type PasskeyAssertionResult =
  | { readonly kind: "assertion"; readonly assertion: PasskeyAssertion }
  | { readonly kind: "cancelled"; readonly message: string }
  | { readonly kind: "error"; readonly message: string };

export interface PasskeyRegistration {
  readonly id: string;
  readonly rawId: string;
  readonly type: "public-key";
  readonly response: {
    readonly attestationObject: string;
    readonly clientDataJSON: string;
  };
}

export type PasskeyRegistrationResult =
  | { readonly kind: "registration"; readonly registration: PasskeyRegistration }
  | { readonly kind: "cancelled"; readonly message: string }
  | { readonly kind: "error"; readonly message: string };

export interface PasskeyCredentialGetter {
  get(options: CredentialRequestOptions): Promise<unknown>;
}

export interface PasskeyCredentialCreator {
  create(options: CredentialCreationOptions): Promise<unknown>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function decodeBase64Url(value: string): ArrayBuffer | null {
  if (!/^[A-Za-z0-9_-]*$/.test(value) || value.length % 4 === 1) {
    return null;
  }

  try {
    const padded = value.padEnd(Math.ceil(value.length / 4) * 4, "=");
    const binary = atob(padded.replace(/-/g, "+").replace(/_/g, "/"));
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes.buffer;
  } catch {
    return null;
  }
}

function encodeBase64Url(value: ArrayBuffer): string {
  const bytes = new Uint8Array(value);
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=/g, "");
}

function parseUserVerification(value: unknown): UserVerificationRequirement | null {
  switch (value) {
    case "required":
    case "preferred":
    case "discouraged":
      return value;
    default:
      return null;
  }
}

function parseTransport(value: unknown): AuthenticatorTransport | null {
  switch (value) {
    case "ble":
    case "hybrid":
    case "internal":
    case "nfc":
    case "usb":
      return value;
    default:
      return null;
  }
}

function parseAllowCredentials(value: unknown): PublicKeyCredentialDescriptor[] | null {
  if (!Array.isArray(value)) {
    return null;
  }

  const credentials: PublicKeyCredentialDescriptor[] = [];
  for (const candidate of value) {
    if (!isRecord(candidate) || candidate.type !== "public-key" || typeof candidate.id !== "string") {
      return null;
    }
    const id = decodeBase64Url(candidate.id);
    if (id === null) {
      return null;
    }

    const descriptor: PublicKeyCredentialDescriptor = { id, type: "public-key" };
    if (candidate.transports !== undefined) {
      if (!Array.isArray(candidate.transports)) {
        return null;
      }
      const transports: AuthenticatorTransport[] = [];
      for (const transport of candidate.transports) {
        const parsedTransport = parseTransport(transport);
        if (parsedTransport === null) {
          return null;
        }
        transports.push(parsedTransport);
      }
      descriptor.transports = transports;
    }
    credentials.push(descriptor);
  }
  return credentials;
}

function parseAuthenticatorAttachment(value: unknown): AuthenticatorAttachment | null {
  switch (value) {
    case "cross-platform":
    case "platform":
      return value;
    default:
      return null;
  }
}

function parseResidentKey(value: unknown): ResidentKeyRequirement | null {
  switch (value) {
    case "discouraged":
    case "preferred":
    case "required":
      return value;
    default:
      return null;
  }
}

function parseAuthenticatorSelection(value: unknown): AuthenticatorSelectionCriteria | null {
  if (!isRecord(value)) {
    return null;
  }

  const parsed: AuthenticatorSelectionCriteria = {};
  if (value.authenticatorAttachment !== undefined) {
    const attachment = parseAuthenticatorAttachment(value.authenticatorAttachment);
    if (attachment === null) return null;
    parsed.authenticatorAttachment = attachment;
  }
  if (value.residentKey !== undefined) {
    const residentKey = parseResidentKey(value.residentKey);
    if (residentKey === null) return null;
    parsed.residentKey = residentKey;
  }
  if (value.requireResidentKey !== undefined) {
    if (typeof value.requireResidentKey !== "boolean") return null;
    parsed.requireResidentKey = value.requireResidentKey;
  }
  if (value.userVerification !== undefined) {
    const userVerification = parseUserVerification(value.userVerification);
    if (userVerification === null) return null;
    parsed.userVerification = userVerification;
  }
  return parsed;
}

function parsePublicKeyCredentialParameters(value: unknown): PublicKeyCredentialParameters[] | null {
  if (!Array.isArray(value) || value.length === 0) {
    return null;
  }

  const parameters: PublicKeyCredentialParameters[] = [];
  for (const candidate of value) {
    if (
      !isRecord(candidate) ||
      candidate.type !== "public-key" ||
      typeof candidate.alg !== "number" ||
      !Number.isInteger(candidate.alg)
    ) {
      return null;
    }
    parameters.push({ type: "public-key", alg: candidate.alg });
  }
  return parameters;
}

/** Parse the WebAuthn request options returned by `/auth/passkeys/login/start`. */
export function parsePasskeyLoginChallenge(value: unknown): PasskeyLoginChallengeResult {
  if (!isRecord(value) || !isRecord(value.publicKey)) {
    return { kind: "invalid", message: "Passkey login returned an invalid challenge" };
  }

  const { publicKey } = value;
  if (typeof publicKey.challenge !== "string") {
    return { kind: "invalid", message: "Passkey login challenge is missing a challenge" };
  }
  const challenge = decodeBase64Url(publicKey.challenge);
  if (challenge === null || challenge.byteLength === 0) {
    return { kind: "invalid", message: "Passkey login challenge has invalid binary data" };
  }

  const parsed: PublicKeyCredentialRequestOptions = { challenge };
  if (publicKey.timeout !== undefined) {
    if (typeof publicKey.timeout !== "number" || !Number.isFinite(publicKey.timeout) || publicKey.timeout < 0) {
      return { kind: "invalid", message: "Passkey login challenge has an invalid timeout" };
    }
    parsed.timeout = publicKey.timeout;
  }
  if (publicKey.rpId !== undefined) {
    if (typeof publicKey.rpId !== "string" || publicKey.rpId.length === 0) {
      return { kind: "invalid", message: "Passkey login challenge has an invalid relying party" };
    }
    parsed.rpId = publicKey.rpId;
  }
  if (publicKey.userVerification !== undefined) {
    const userVerification = parseUserVerification(publicKey.userVerification);
    if (userVerification === null) {
      return { kind: "invalid", message: "Passkey login challenge has invalid user verification" };
    }
    parsed.userVerification = userVerification;
  }
  if (publicKey.allowCredentials !== undefined) {
    const allowCredentials = parseAllowCredentials(publicKey.allowCredentials);
    if (allowCredentials === null) {
      return { kind: "invalid", message: "Passkey login challenge has invalid allowed credentials" };
    }
    parsed.allowCredentials = allowCredentials;
  }

  return { kind: "valid", request: { publicKey: parsed } };
}

/** Parse the WebAuthn creation options returned by `/auth/passkeys/register/start`. */
export function parsePasskeyRegistrationChallenge(
  value: unknown,
): PasskeyRegistrationChallengeResult {
  if (!isRecord(value) || !isRecord(value.publicKey)) {
    return { kind: "invalid", message: "Passkey registration returned an invalid challenge" };
  }

  const { publicKey } = value;
  if (
    typeof publicKey.challenge !== "string" ||
    !isRecord(publicKey.rp) ||
    typeof publicKey.rp.name !== "string" ||
    !isRecord(publicKey.user) ||
    typeof publicKey.user.id !== "string" ||
    typeof publicKey.user.name !== "string" ||
    typeof publicKey.user.displayName !== "string"
  ) {
    return { kind: "invalid", message: "Passkey registration challenge is missing required fields" };
  }

  const challenge = decodeBase64Url(publicKey.challenge);
  const userId = decodeBase64Url(publicKey.user.id);
  const parameters = parsePublicKeyCredentialParameters(publicKey.pubKeyCredParams);
  if (challenge === null || challenge.byteLength === 0 || userId === null || userId.byteLength === 0 || parameters === null) {
    return { kind: "invalid", message: "Passkey registration challenge has invalid binary data" };
  }

  const rp: PublicKeyCredentialRpEntity = { name: publicKey.rp.name };
  if (publicKey.rp.id !== undefined) {
    if (typeof publicKey.rp.id !== "string" || publicKey.rp.id.length === 0) {
      return { kind: "invalid", message: "Passkey registration challenge has an invalid relying party" };
    }
    rp.id = publicKey.rp.id;
  }

  const parsed: PublicKeyCredentialCreationOptions = {
    challenge,
    rp,
    user: {
      id: userId,
      name: publicKey.user.name,
      displayName: publicKey.user.displayName,
    },
    pubKeyCredParams: parameters,
  };
  if (publicKey.timeout !== undefined) {
    if (typeof publicKey.timeout !== "number" || !Number.isFinite(publicKey.timeout) || publicKey.timeout < 0) {
      return { kind: "invalid", message: "Passkey registration challenge has an invalid timeout" };
    }
    parsed.timeout = publicKey.timeout;
  }
  if (publicKey.excludeCredentials !== undefined) {
    const excludeCredentials = parseAllowCredentials(publicKey.excludeCredentials);
    if (excludeCredentials === null) {
      return { kind: "invalid", message: "Passkey registration challenge has invalid excluded credentials" };
    }
    parsed.excludeCredentials = excludeCredentials;
  }
  const selection: AuthenticatorSelectionCriteria | null =
    publicKey.authenticatorSelection === undefined
      ? {}
      : parseAuthenticatorSelection(publicKey.authenticatorSelection);
  if (selection === null) {
    return { kind: "invalid", message: "Passkey registration challenge has invalid authenticator selection" };
  }
  // Keep the discoverable-passkey guarantee even when the server challenge
  // does not explicitly include an authenticator selection.
  selection.residentKey = "required";
  selection.requireResidentKey = true;
  parsed.authenticatorSelection = selection;
  if (publicKey.attestation !== undefined) {
    if (
      publicKey.attestation !== "direct" &&
      publicKey.attestation !== "enterprise" &&
      publicKey.attestation !== "indirect" &&
      publicKey.attestation !== "none"
    ) {
      return { kind: "invalid", message: "Passkey registration challenge has invalid attestation" };
    }
    parsed.attestation = publicKey.attestation;
  }

  return { kind: "valid", request: { publicKey: parsed } };
}

function readBuffer(value: unknown): ArrayBuffer | null {
  return value instanceof ArrayBuffer ? value : null;
}

/**
 * Run a parsed challenge through a credential getter and return the exact
 * assertion shape expected by `/auth/passkeys/login/finish`.
 */
export async function requestPasskeyAssertion({
  credentialGetter,
  request,
}: {
  credentialGetter: PasskeyCredentialGetter;
  request: CredentialRequestOptions;
}): Promise<PasskeyAssertionResult> {
  let credential: unknown;
  try {
    credential = await credentialGetter.get(request);
  } catch (error) {
    const message = error instanceof Error ? error.message : "Passkey error";
    return { kind: "error", message };
  }

  if (credential === null) {
    return { kind: "cancelled", message: "No passkey was selected" };
  }
  if (!isRecord(credential) || credential.type !== "public-key" || typeof credential.id !== "string") {
    return { kind: "error", message: "Passkey returned an invalid credential" };
  }
  if (!isRecord(credential.response)) {
    return { kind: "error", message: "Passkey returned an invalid assertion" };
  }

  const rawId = readBuffer(credential.rawId);
  const authenticatorData = readBuffer(credential.response.authenticatorData);
  const clientDataJSON = readBuffer(credential.response.clientDataJSON);
  const signature = readBuffer(credential.response.signature);
  const userHandle = credential.response.userHandle;
  const parsedUserHandle =
    userHandle === null || userHandle === undefined ? null : readBuffer(userHandle);
  if (
    rawId === null ||
    authenticatorData === null ||
    clientDataJSON === null ||
    signature === null ||
    parsedUserHandle === null && userHandle !== null && userHandle !== undefined
  ) {
    return { kind: "error", message: "Passkey returned an invalid assertion" };
  }

  return {
    kind: "assertion",
    assertion: {
      id: credential.id,
      rawId: encodeBase64Url(rawId),
      type: "public-key",
      response: {
        authenticatorData: encodeBase64Url(authenticatorData),
        clientDataJSON: encodeBase64Url(clientDataJSON),
        signature: encodeBase64Url(signature),
        userHandle: parsedUserHandle === null ? null : encodeBase64Url(parsedUserHandle),
      },
    },
  };
}

/**
 * Run a parsed registration challenge through a credential creator and return
 * the exact credential shape expected by `/auth/passkeys/register/finish`.
 */
export async function requestPasskeyRegistration({
  credentialCreator,
  request,
}: {
  credentialCreator: PasskeyCredentialCreator;
  request: CredentialCreationOptions;
}): Promise<PasskeyRegistrationResult> {
  let credential: unknown;
  try {
    credential = await credentialCreator.create(request);
  } catch (error) {
    const message = error instanceof Error ? error.message : "Passkey error";
    return { kind: "error", message };
  }

  if (credential === null) {
    return { kind: "cancelled", message: "No passkey was created" };
  }
  if (!isRecord(credential) || credential.type !== "public-key" || typeof credential.id !== "string") {
    return { kind: "error", message: "Passkey returned an invalid credential" };
  }
  if (!isRecord(credential.response)) {
    return { kind: "error", message: "Passkey returned an invalid registration" };
  }

  const rawId = readBuffer(credential.rawId);
  const attestationObject = readBuffer(credential.response.attestationObject);
  const clientDataJSON = readBuffer(credential.response.clientDataJSON);
  if (rawId === null || attestationObject === null || clientDataJSON === null) {
    return { kind: "error", message: "Passkey returned an invalid registration" };
  }

  return {
    kind: "registration",
    registration: {
      id: credential.id,
      rawId: encodeBase64Url(rawId),
      type: "public-key",
      response: {
        attestationObject: encodeBase64Url(attestationObject),
        clientDataJSON: encodeBase64Url(clientDataJSON),
      },
    },
  };
}
