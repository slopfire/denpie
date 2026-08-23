// Focused Bun tests for the /api/v1 transport: fake fetch only, no live
// server or browser.
import { describe, expect, test } from "bun:test";
import {
  ApiErrorSchema,
  ApiErrorCode,
  ApiInfoSchema,
  ApiV1RequestSchema,
  ApiV1ResponseSchema,
  ApiResponseSchema,
  EmptySchema,
} from "../../generated/denpie_pb";
import type { ApiV1Request, ApiV1Response } from "../../generated/denpie_pb";
import {
  API_V1_PATH,
  TransportError,
  buildEnvelope,
  callEnvelope,
  callMutationWithKey,
  callRead,
  decodeHttpResponse,
  encodeRequest,
  newIdempotencyKey,
  type FetchLike,
} from "./transport";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";

const getApiInfoCall = {
  case: "getApiInfo" as const,
  value: create(EmptySchema, {}),
};

function apiInfo() {
  return create(ApiInfoSchema, {
    apiVersion: "v1",
    serverVersion: "test-server",
    buildSha: "abc123",
    capabilities: ["cards:read"],
  });
}

function successEnvelope(requestId: string): ApiV1Response {
  return create(ApiV1ResponseSchema, {
    requestId,
    outcome: {
      case: "success",
      value: create(ApiResponseSchema, {
        result: { case: "apiInfo", value: apiInfo() },
      }),
    },
  });
}

function errorEnvelope(init?: {
  code?: ApiErrorCode;
  message?: string;
  retryable?: boolean;
}): ApiV1Response {
  return create(ApiV1ResponseSchema, {
    requestId: "srv-req-1",
    outcome: {
      case: "error",
      value: create(ApiErrorSchema, {
        code: init?.code ?? ApiErrorCode.INTERNAL,
        message: init?.message ?? "boom",
        retryable: init?.retryable ?? false,
      }),
    },
  });
}

function protobufHttpResponse(
  status: number,
  envelope: ApiV1Response,
  contentType = "application/x-protobuf",
): Response {
  return new Response(toBinary(ApiV1ResponseSchema, envelope), {
    status,
    headers: contentType === "" ? {} : { "Content-Type": contentType },
  });
}

/** Records each decoded request and replays queued responses in order. */
function fakeFetch(
  responses: Array<Response | ((req: ApiV1Request) => Response)>,
): {
  fetch: FetchLike;
  requests: ApiV1Request[];
  inits: RequestInit[];
} {
  const requests: ApiV1Request[] = [];
  const inits: RequestInit[] = [];
  const queue = [...responses];
  const fetch: FetchLike = async (_input, init) => {
    inits.push(init);
    if (!(init.body instanceof ArrayBuffer)) {
      throw new TypeError("expected protobuf request body as ArrayBuffer");
    }
    const req = fromBinary(ApiV1RequestSchema, new Uint8Array(init.body));
    requests.push(req);
    const next = queue.shift();
    if (!next) throw new Error("fake fetch exhausted");
    return typeof next === "function" ? next(req) : next;
  };
  return { fetch, requests, inits };
}

describe("buildEnvelope", () => {
  test("read envelope carries request id and no idempotency key", () => {
    const envelope = buildEnvelope(getApiInfoCall, "read-1", null, false);
    expect(envelope.requestId).toBe("read-1");
    expect(envelope.idempotencyKey).toBe("");
    expect(envelope.call?.op.case).toBe("getApiInfo");
    expect(envelope.call?.auth).toBe("");
  });

  test("mutation requires non-empty idempotency key", () => {
    for (const bad of [null, "", "   "]) {
      try {
        buildEnvelope(getApiInfoCall, "mut-1", bad as string | null, true);
        throw new Error(`should have thrown for ${JSON.stringify(bad)}`);
      } catch (err) {
        expect(err).toBeInstanceOf(TransportError);
        expect((err as TransportError).message).toContain(
          "Idempotency key is required",
        );
        expect((err as TransportError).retryable).toBe(false);
        expect((err as TransportError).mutationOutcomeIndeterminate).toBe(false);
      }
    }
  });

  test("mutation keeps a caller-supplied key verbatim", () => {
    const envelope = buildEnvelope(getApiInfoCall, "mut-1", " key-1 ", true);
    expect(envelope.idempotencyKey).toBe(" key-1 ");
  });
});

describe("encode/decode round trip", () => {
  test("generated request and response messages round trip with their own schemas", () => {
    const request = buildEnvelope(getApiInfoCall, "read-42", null, false);
    const requestBytes = encodeRequest(request);
    expect(fromBinary(ApiV1RequestSchema, requestBytes).requestId).toBe("read-42");

    const response = successEnvelope("read-42");
    const responseBytes = toBinary(ApiV1ResponseSchema, response);
    const decoded = decodeHttpResponse(
      200,
      "application/x-protobuf",
      responseBytes,
    );
    expect(decoded.requestId).toBe("read-42");
    expect(decoded.outcome.case).toBe("success");
  });

  test("invalid protobuf fails closed with retryable+indeterminate", () => {
    try {
      decodeHttpResponse(200, "application/x-protobuf", new Uint8Array([0xff, 0xff, 0xff]));
      throw new Error("should have thrown");
    } catch (err) {
      expect(err).toBeInstanceOf(TransportError);
      const e = err as TransportError;
      expect(e.retryable).toBe(true);
      expect(e.mutationOutcomeIndeterminate).toBe(true);
      expect(e.message).toContain("Invalid protobuf response");
    }
  });

  test("non-protobuf content types are classified before protobuf decode", () => {
    const cases = [
      {
        contentType: "text/html",
        body: "<html>oops</html>",
        expected: "returned a web page instead of an API response",
      },
      {
        contentType: "application/json",
        body: '{"error":"bad request"}',
        expected: '{"error":"bad request"}',
      },
    ];
    for (const { contentType, body, expected } of cases) {
      try {
        decodeHttpResponse(502, contentType, new TextEncoder().encode(body));
        throw new Error("should have thrown");
      } catch (err) {
        expect(err).toBeInstanceOf(TransportError);
        expect((err as TransportError).message).toContain(expected);
        expect((err as TransportError).retryable).toBe(true); // 5xx
        expect((err as TransportError).mutationOutcomeIndeterminate).toBe(true);
      }
    }
  });

  test("missing content type still decodes a protobuf body", () => {
    const bytes = encodeRequest(buildEnvelope(getApiInfoCall, "r", null, false));
    const decoded = decodeHttpResponse(200, null, bytes);
    expect(decoded.requestId).toBe("r");
  });
});

describe("callEnvelope over fake fetch", () => {
  test("posts protobuf to /api/v1 with same-origin cookies", async () => {
    const { fetch, inits, requests } = fakeFetch([
      protobufHttpResponse(200, successEnvelope("srv-1")),
    ]);
    const result = await callEnvelope(
      buildEnvelope(getApiInfoCall, "read-1", null, false),
      { fetch },
    );
    expect(result.result.case).toBe("apiInfo");
    expect(result.result.value.apiVersion).toBe("v1");
    expect(inits[0]?.method).toBe("POST");
    expect(inits[0]?.credentials).toBe("same-origin");
    expect((inits[0]?.headers as Record<string, string>)["Content-Type"]).toBe(
      "application/x-protobuf",
    );
    expect(requests[0]?.requestId).toBe("read-1");
    expect(API_V1_PATH).toBe("/api/v1");
  });

  test("success decode returns typed ApiResponse via callRead", async () => {
    const { fetch } = fakeFetch([
      protobufHttpResponse(200, successEnvelope("srv-2")),
    ]);
    const result = await callRead(getApiInfoCall, { fetch });
    expect(result.result.case).toBe("apiInfo");
    expect(result.result.value.buildSha).toBe("abc123");
  });

  test("protobuf error decodes into typed failure flags", async () => {
    const { fetch } = fakeFetch([
      protobufHttpResponse(
        403,
        errorEnvelope({
          code: ApiErrorCode.PERMISSION_DENIED,
          message: "nope",
          retryable: false,
        }),
      ),
    ]);
    try {
      await callRead(getApiInfoCall, { fetch });
      throw new Error("should have thrown");
    } catch (err) {
      const e = err as TransportError;
      expect(e.status).toBe(403);
      expect(e.message).toBe("nope");
      expect(e.retryable).toBe(false);
      expect(e.mutationOutcomeIndeterminate).toBe(false);
      expect(e.requestId).toBe("srv-req-1");
    }
  });

  test("429 is retryable but not indeterminate", async () => {
    const { fetch } = fakeFetch([
      protobufHttpResponse(
        429,
        errorEnvelope({
          code: ApiErrorCode.RATE_LIMITED,
          message: "slow down",
          retryable: true,
        }),
      ),
    ]);
    try {
      await callRead(getApiInfoCall, { fetch });
      throw new Error("should have thrown");
    } catch (err) {
      const e = err as TransportError;
      expect(e.status).toBe(429);
      expect(e.retryable).toBe(true);
      expect(e.mutationOutcomeIndeterminate).toBe(false);
    }
  });

  test("HTML error body becomes web-page failure", async () => {
    const { fetch } = fakeFetch([
      new Response("<!doctype html><html><body>login</body></html>", {
        status: 500,
        headers: { "Content-Type": "text/html" },
      }),
    ]);
    try {
      await callRead(getApiInfoCall, { fetch });
      throw new Error("should have thrown");
    } catch (err) {
      const e = err as TransportError;
      expect(e.message).toContain(
        "returned a web page instead of an API response",
      );
      expect(e.retryable).toBe(true);
      expect(e.mutationOutcomeIndeterminate).toBe(true);
    }
  });

  test("network failure is retryable and indeterminate", async () => {
    const failing: FetchLike = async () => {
      throw new TypeError("fetch failed");
    };
    try {
      await callRead(getApiInfoCall, { fetch: failing });
      throw new Error("should have thrown");
    } catch (err) {
      const e = err as TransportError;
      expect(e.status).toBe(0);
      expect(e.retryable).toBe(true);
      expect(e.mutationOutcomeIndeterminate).toBe(true);
    }
  });
});

describe("callMutationWithKey", () => {
  test("rejects empty key before any network call", async () => {
    let called = 0;
    const counting: FetchLike = async () => {
      called += 1;
      return protobufHttpResponse(200, successEnvelope("srv-3"));
    };
    try {
      await callMutationWithKey(getApiInfoCall, "", { fetch: counting });
      throw new Error("should have thrown");
    } catch (err) {
      expect((err as TransportError).message).toContain(
        "Idempotency key is required",
      );
    }
    expect(called).toBe(0);
  });

  test("retries indeterminate mutation with identical encoded envelope and key", async () => {
    // Declared protobuf content type but garbage bytes: invalid protobuf on a
    // 502 is retryable AND mutation-outcome-indeterminate.
    const dropped: Response = new Response(new Uint8Array([0xde, 0xad]), {
      status: 502,
      headers: { "Content-Type": "application/x-protobuf" },
    });
    const { fetch, requests } = fakeFetch([
      dropped,
      protobufHttpResponse(200, successEnvelope("srv-4")),
    ]);
    const key = newIdempotencyKey();
    const result = await callMutationWithKey(getApiInfoCall, key, { fetch });
    expect(result.result.case).toBe("apiInfo");
    expect(requests.length).toBe(2);
    expect(requests[0]?.idempotencyKey).toBe(key);
    expect(requests[1]?.idempotencyKey).toBe(key);
    // Exact same encoded envelope bytes on both attempts.
    expect(toBinary(ApiV1RequestSchema, requests[0]!)).toEqual(
      toBinary(ApiV1RequestSchema, requests[1]!),
    );
  });

  test("does not retry non-indeterminate failures", async () => {
    const { fetch, requests } = fakeFetch([
      protobufHttpResponse(
        403,
        errorEnvelope({ code: ApiErrorCode.PERMISSION_DENIED }),
      ),
    ]);
    await expect(
      callMutationWithKey(getApiInfoCall, "key-9", { fetch }),
    ).rejects.toBeInstanceOf(TransportError);
    expect(requests.length).toBe(1);
  });
});
