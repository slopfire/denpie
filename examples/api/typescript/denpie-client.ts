import { randomUUID } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";
import protobuf from "protobufjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const schemaPath = process.env.DENPIE_SCHEMA ?? path.resolve(here, "../../../proto/denpie.proto");
const root = await protobuf.load(schemaPath);
const ApiV1Request = root.lookupType("denpie.ApiV1Request");
const ApiV1Response = root.lookupType("denpie.ApiV1Response");

type Payload = Record<string, unknown>;

async function apiCall(payload: Payload): Promise<Record<string, unknown>> {
  const validationError = ApiV1Request.verify(payload);
  if (validationError) throw new Error(`invalid request: ${validationError}`);

  const encoded = ApiV1Request.encode(ApiV1Request.create(payload)).finish();
  const headers: Record<string, string> = {
    "Content-Type": "application/x-protobuf",
  };
  if (process.env.DENPIE_API_KEY) {
    headers.Authorization = `Bearer ${process.env.DENPIE_API_KEY}`;
  }
  const response = await fetch(
    process.env.DENPIE_URL ?? "http://127.0.0.1:3017/api/v1",
    { method: "POST", headers, body: Buffer.from(encoded) },
  );
  const bytes = new Uint8Array(await response.arrayBuffer());
  const decoded = ApiV1Response.toObject(ApiV1Response.decode(bytes), {
    enums: String,
    longs: String,
    oneofs: true,
  }) as Record<string, unknown>;
  if (!response.ok || decoded.outcome === "error") {
    throw new Error(`Denpie returned HTTP ${response.status}: ${JSON.stringify(decoded)}`);
  }
  return decoded;
}

function infoRequest(): Payload {
  return {
    requestId: "typescript-get-api-info",
    call: { getApiInfo: {} },
  };
}

function cardsRequest(): Payload {
  return {
    requestId: "typescript-list-flow-cards",
    call: { listFlowCards: { pageSize: 12 } },
  };
}

function createDocumentRequest(idempotencyKey: string): Payload {
  return {
    requestId: "typescript-create-document",
    idempotencyKey,
    call: {
      createDocument: {
        sourceType: "document",
        title: "TypeScript API example",
        content: "Created by the checked-in TypeScript client example.",
      },
    },
  };
}

function selfTest(): void {
  const encoded = ApiV1Request.encode(ApiV1Request.create(infoRequest())).finish();
  const decoded = ApiV1Request.toObject(ApiV1Request.decode(encoded), { oneofs: true });
  if (decoded.call?.op !== "getApiInfo") throw new Error("oneof did not round-trip");
  console.log("TypeScript protobuf client self-test passed");
}

const command = process.argv[2] ?? "info";
if (command === "--self-test") {
  selfTest();
} else if (command === "info") {
  console.log(await apiCall(infoRequest()));
} else if (command === "cards") {
  console.log(await apiCall(cardsRequest()));
} else if (command === "create-document") {
  const idempotencyKey = process.env.DENPIE_IDEMPOTENCY_KEY ?? randomUUID();
  console.error(`idempotency_key=${idempotencyKey}`);
  console.log(await apiCall(createDocumentRequest(idempotencyKey)));
} else {
  console.error("usage: denpie-client.ts [info|cards|create-document|--self-test]");
  process.exitCode = 2;
}
