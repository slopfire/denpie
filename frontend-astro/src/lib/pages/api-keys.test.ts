import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { ApiKeyInfoSchema } from "@/generated/denpie_pb";
import { apiKeyDomId, formatApiKeyDate, sortApiKeys } from "./api-keys";

function key(id: bigint, createdAt: string) {
    return create(ApiKeyInfoSchema, { id, createdAt });
}

describe("API key page helpers", () => {
    test("sorts by creation time without converting bigint IDs to numbers", () => {
        const newest = 9_007_199_254_740_993n;
        const keys = [
            key(2n, "2026-01-01T00:00:00Z"),
            key(newest, "2026-01-01T00:00:00Z"),
            key(3n, "2027-01-01T00:00:00Z"),
        ];

        expect(sortApiKeys(keys).map((item) => item.id)).toEqual([
            3n,
            newest,
            2n,
        ]);
        expect(keys[0]?.id).toBe(2n);
    });

    test("builds stable bigint-safe DOM IDs and date labels", () => {
        expect(apiKeyDomId(9_007_199_254_740_993n)).toBe(
            "api-key-9007199254740993",
        );
        expect(formatApiKeyDate("")).toBe("Never");
        expect(formatApiKeyDate("not-a-date")).toBe("not-a-date");
    });
});
