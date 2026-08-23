import { describe, expect, test } from "bun:test";
import {
    classifyToast,
    looksLikeError,
    splitToastParts,
    toastTimeoutMs,
} from "./toast";

describe("toast classification", () => {
    test("splits multiline copy into summary and detail", () => {
        expect(splitToastParts("Failed to save\nHTTP 500 body here")).toEqual({
            summary: "Failed to save",
            detail: "HTTP 500 body here",
        });
    });

    test("truncates long single-line copy", () => {
        const long = "x".repeat(200);
        const parts = splitToastParts(long);
        expect(parts.summary.endsWith("…")).toBe(true);
        expect([...parts.summary].length).toBeLessThanOrEqual(121);
        expect(parts.detail).toBe(long);
    });

    test("detects errorish messages", () => {
        expect(looksLikeError("Failed to parse settings response")).toBe(true);
        expect(looksLikeError("LLM Error: HTTP 401")).toBe(true);
        expect(looksLikeError("API key missing")).toBe(true);
        expect(looksLikeError("Profile refreshed")).toBe(false);
        expect(looksLikeError("Cards added")).toBe(false);
    });

    test("error toasts never auto-hide", () => {
        expect(toastTimeoutMs("error")).toBeNull();
        expect(toastTimeoutMs("success")).toBe(2400);
        expect(toastTimeoutMs("info")).toBe(2800);
        expect(classifyToast("Saved", "success").kind).toBe("success");
        expect(classifyToast("Failed to save", "success").kind).toBe("error");
    });
});
