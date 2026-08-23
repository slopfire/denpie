// Pure, fetch-injectable /api/v1 transport. Messages come from the generated
// canonical proto contract; this module does not duplicate them.
import {
  ApiRequestSchema,
  ApiV1RequestSchema,
  ApiV1ResponseSchema,
} from "../../generated/denpie_pb";
import type {
  ApiError,
  ApiRequest,
  ApiResponse,
  ApiV1Request,
  ApiV1Response,
} from "../../generated/denpie_pb";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";

export const API_V1_PATH = "/api/v1";
const PROTOBUF_CONTENT_TYPE = "application/x-protobuf";

/** Failure classification shared by every transport path. */
export class TransportError extends Error {
  readonly status: number;
  readonly retryable: boolean;
  /**
   * The request may have reached the server, so retrying a mutation must keep
   * its original idempotency key and payload.
   */
  readonly mutationOutcomeIndeterminate: boolean;
  readonly requestId: string;

  constructor(init: {
    status: number;
    message: string;
    retryable: boolean;
    mutationOutcomeIndeterminate: boolean;
    requestId: string;
  }) {
    super(init.message);
    this.name = "TransportError";
    this.status = init.status;
    this.retryable = init.retryable;
    this.mutationOutcomeIndeterminate = init.mutationOutcomeIndeterminate;
    this.requestId = init.requestId;
  }
}

/** Build a versioned envelope. Mutations require a non-empty idempotency key. */
export function buildEnvelope(
  op: ApiRequest["op"],
  requestId: string,
  idempotencyKey: string | null,
  isMutation: boolean,
): ApiV1Request {
  if (isMutation && (idempotencyKey === null || idempotencyKey.trim() === "")) {
    throw new TransportError({
      status: 0,
      message: "Idempotency key is required for mutations",
      retryable: false,
      mutationOutcomeIndeterminate: false,
      requestId: "",
    });
  }
  return create(ApiV1RequestSchema, {
    requestId,
    call: create(ApiRequestSchema, { auth: "", op }),
    idempotencyKey: idempotencyKey ?? "",
  });
}

export function encodeRequest(envelope: ApiV1Request): Uint8Array {
  return toBinary(ApiV1RequestSchema, envelope);
}

export function decodeResponse(bytes: Uint8Array): ApiV1Response {
  try {
    return fromBinary(ApiV1ResponseSchema, bytes);
  } catch (err) {
    throw new TransportError({
      status: 0,
      message: `Invalid protobuf response: ${err instanceof Error ? err.message : String(err)}`,
      retryable: true,
      mutationOutcomeIndeterminate: true,
      requestId: "",
    });
  }
}

function isProtobufContentType(contentType: string | null): boolean {
  if (contentType === null) return false;
  const first = contentType.split(";")[0]?.trim().toLowerCase();
  return (
    first === "application/x-protobuf" || first === "application/protobuf"
  );
}

function errorFromNonProtobuf(status: number, bytes: Uint8Array): TransportError {
  const text = new TextDecoder("utf-8", { fatal: false }).decode(bytes).trim();
  let message: string;
  if (status === 429) {
    message = "Too many requests; retry shortly";
  } else if (text.startsWith("<")) {
    message = `HTTP ${status} returned a web page instead of an API response`;
  } else if (text.length > 0 && text.length <= 300 && !text.includes("\u0000")) {
    message = text;
  } else if (status === 0) {
    message = "Invalid protobuf response";
  } else {
    message = `HTTP ${status} response was not protobuf`;
  }
  return new TransportError({
    status,
    message,
    retryable: status === 429 || (status >= 500 && status < 600),
    // 4xx bodies (including 429) were rejected before the handler ran.
    mutationOutcomeIndeterminate: status >= 500 && status < 600,
    requestId: "",
  });
}

/**
 * Decode an HTTP response into the versioned envelope. A missing
 * Content-Type is tolerated; anything explicitly non-protobuf is rejected
 * without trusting the bytes.
 */
export function decodeHttpResponse(
  status: number,
  contentType: string | null,
  bytes: Uint8Array,
): ApiV1Response {
  const declaredProtobuf = isProtobufContentType(contentType);
  if (contentType !== null && !declaredProtobuf) {
    throw errorFromNonProtobuf(status, bytes);
  }
  try {
    return decodeResponse(bytes);
  } catch (err) {
    if (err instanceof TransportError) {
      if (declaredProtobuf) {
        throw new TransportError({
          status,
          message: err.message,
          retryable: err.retryable,
          mutationOutcomeIndeterminate: err.mutationOutcomeIndeterminate,
          requestId: err.requestId,
        });
      }
      throw errorFromNonProtobuf(status, bytes);
    }
    throw err;
  }
}

/** Minimal fetch surface the transport depends on; injectable in tests. */
export type FetchLike = (input: string, init: RequestInit) => Promise<Response>;

export interface CallDeps {
  fetch?: FetchLike;
  path?: string;
}

/** POST a built envelope and return the success `ApiResponse` body. */
export async function callEnvelope(
  envelope: ApiV1Request,
  deps: CallDeps = {},
): Promise<ApiResponse> {
  const doFetch = deps.fetch ?? fetch;
  const path = deps.path ?? API_V1_PATH;
  const requestBody = new Uint8Array(encodeRequest(envelope)).buffer;

  let response: Response;
  try {
    response = await doFetch(path, {
      method: "POST",
      headers: { "Content-Type": PROTOBUF_CONTENT_TYPE },
      body: requestBody,
      credentials: "same-origin",
    });
  } catch (err) {
    throw new TransportError({
      status: 0,
      message: `Network error: ${err instanceof Error ? err.message : String(err)}`,
      retryable: true,
      mutationOutcomeIndeterminate: true,
      requestId: envelope.requestId,
    });
  }

  const status = response.status;
  const contentType = response.headers.get("content-type");
  let bytes: Uint8Array;
  try {
    bytes = new Uint8Array(await response.arrayBuffer());
  } catch (err) {
    throw new TransportError({
      status,
      message: `Failed to read response body: ${err instanceof Error ? err.message : String(err)}`,
      retryable: true,
      mutationOutcomeIndeterminate: true,
      requestId: envelope.requestId,
    });
  }

  const decoded = decodeHttpResponse(status, contentType, bytes);
  switch (decoded.outcome.case) {
    case "success": {
      if (!(status >= 200 && status < 300)) {
        throw new TransportError({
          status,
          message: `Unexpected HTTP ${status} with success body`,
          retryable: true,
          mutationOutcomeIndeterminate: true,
          requestId: decoded.requestId,
        });
      }
      return decoded.outcome.value;
    }
    case "error": {
      const protoError: ApiError = decoded.outcome.value;
      const message =
        protoError.message === ""
          ? `API error (code=${protoError.code})`
          : protoError.message;
      throw new TransportError({
        status,
        message,
        retryable: protoError.retryable,
        mutationOutcomeIndeterminate: serverReportsIndeterminateOutcome(
          status,
          message,
          protoError.retryable,
        ),
        requestId: decoded.requestId,
      });
    }
    default:
      throw new TransportError({
        status,
        message: "Empty API v1 response outcome",
        retryable: true,
        mutationOutcomeIndeterminate: true,
        requestId: decoded.requestId,
      });
  }
}

/** Call a read-only operation (no idempotency key). */
export function callRead(
  op: ApiRequest["op"],
  deps: CallDeps = {},
): Promise<ApiResponse> {
  return callEnvelope(buildEnvelope(op, newRequestId("read"), null, false), deps);
}

/**
 * Call a mutation with a caller-owned idempotency key. Retryable and
 * outcome-indeterminate failures are retried once with the exact same encoded
 * envelope, so a response lost after commit cannot execute it twice.
 */
export async function callMutationWithKey(
  op: ApiRequest["op"],
  idempotencyKey: string,
  deps: CallDeps = {},
): Promise<ApiResponse> {
  const { response } = await callMutationWithKeyEnvelope(op, idempotencyKey, deps);
  return response;
}

/** Like {@link callMutationWithKey} but also returns the sent request id. */
export async function callMutationWithKeyEnvelope(
  op: ApiRequest["op"],
  idempotencyKey: string,
  deps: CallDeps = {},
): Promise<{ requestId: string; response: ApiResponse }> {
  const envelope = buildEnvelope(op, newRequestId("mut"), idempotencyKey, true);
  try {
    return { requestId: envelope.requestId, response: await callEnvelope(envelope, deps) };
  } catch (err) {
    if (
      err instanceof TransportError &&
      err.retryable &&
      err.mutationOutcomeIndeterminate
    ) {
      // Same envelope => identical protobuf bytes and idempotency key on wire.
      return { requestId: envelope.requestId, response: await callEnvelope(envelope, deps) };
    }
    throw err;
  }
}

function serverReportsIndeterminateOutcome(
  status: number,
  message: string,
  retryable: boolean,
): boolean {
  return (
    retryable &&
    (status === 409 ||
      message.includes("same idempotency key") ||
      message.includes("idempotency result"))
  );
}

const REQUEST_ID_MAX = 64;

/** Random request id within the server's 1-64 ASCII charset. */
export function newRequestId(prefix: string): string {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  let n = 0n;
  for (const b of bytes) n = (n << 8n) | BigInt(b);
  return `${prefix}-${n}`.slice(0, REQUEST_ID_MAX);
}

/** UUID-ish hex idempotency key within the server's 1-128 charset. */
export function newIdempotencyKey(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
