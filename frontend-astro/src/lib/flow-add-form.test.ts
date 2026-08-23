import { describe, expect, test } from "bun:test";
import { TipcardTypeValue } from "../generated/denpie_pb";
import {
    AddValidationError,
    MAX_IMAGE_BYTES,
    MAX_MANUAL_IMAGES,
    REPEATABLE_COUNT,
    buildTipsRequest,
    fitWithin,
    isAllowedImageType,
    parseAddCardKind,
    parseStoredCardKind,
    parseTopicsCsv,
    selectImages,
    tipcardTypeFor,
} from "./flow-add-form";
import type { AddCardKind, AddTipsPayload } from "./flow-add-form";

function payload(
    kind: AddCardKind,
    overrides: Partial<AddTipsPayload> = {},
): AddTipsPayload {
    return {
        kind,
        topics: [" Rust ", "Rust", " Systems "],
        manualContent: "A manually supplied card.",
        manualImageData: ["data:image/png;base64,AA=="],
        idempotencyKey: "add-key-1",
        ...overrides,
    };
}

describe("flow add form", () => {
    test("parses CSV topics by trimming/filtering without deduplication", () => {
        expect(parseTopicsCsv(" Rust, ,Python,Rust,  Systems ")).toEqual([
            "Rust",
            "Python",
            "Rust",
            "Systems",
        ]);
    });

    test("unknown or missing stored kind falls back to Casual", () => {
        expect(parseStoredCardKind(null)).toBe("casual");
        expect(parseStoredCardKind(undefined)).toBe("casual");
        expect(parseStoredCardKind("new_protocol_kind")).toBe("casual");
        expect(parseStoredCardKind("casual_tip")).toBe("casual");
        expect(parseStoredCardKind("repeatable_tip")).toBe("repeatable");
        expect(parseStoredCardKind("manual_tip")).toBe("manual");
        expect(parseAddCardKind("unexpected")).toBe("casual");
    });

    test("maps every UI kind exhaustively to the generated enum", () => {
        expect(tipcardTypeFor("casual")).toBe(TipcardTypeValue.CASUAL);
        expect(tipcardTypeFor("repeatable")).toBe(TipcardTypeValue.REPEATABLE);
        expect(tipcardTypeFor("manual")).toBe(TipcardTypeValue.MANUAL);
    });

    test("casual sends normalized topics and no repeat/manual fields", () => {
        const request = buildTipsRequest(
            payload("casual", {
                topics: [" Rust ", "", "Rust", "  "],
                manualContent: "ignored",
                manualImageData: ["ignored"],
            }),
        );
        expect(request.count).toBe(0);
        expect(request.topics).toEqual(["Rust", "Rust"]);
        expect(request.tipcardType).toBe(TipcardTypeValue.CASUAL);
        expect(request.excludeCardIds).toEqual([]);
        expect(request.manualContent).toBe("");
        expect(request.manualCompressedContent).toBe("");
        expect(request.manualImageData).toEqual([]);
    });

    test("repeatable sends exactly five and keeps duplicate topic order", () => {
        const request = buildTipsRequest(payload("repeatable"));
        expect(request.count).toBe(REPEATABLE_COUNT);
        expect(request.topics).toEqual(["Rust", "Rust", "Systems"]);
        expect(request.tipcardType).toBe(TipcardTypeValue.REPEATABLE);
        expect(request.manualContent).toBe("");
        expect(request.manualImageData).toEqual([]);
    });

    test("manual sends content and image data only for manual cards", () => {
        const request = buildTipsRequest(payload("manual"));
        expect(request.count).toBe(0);
        expect(request.tipcardType).toBe(TipcardTypeValue.MANUAL);
        expect(request.manualContent).toBe("A manually supplied card.");
        expect(request.manualImageData).toEqual(["data:image/png;base64,AA=="]);
        expect(request.manualCompressedContent).toBe("");
    });

    test("rejects blank topics, blank manual content, and blank keys", () => {
        expect(() =>
            buildTipsRequest(payload("casual", { topics: [" ", ""] })),
        ).toThrow(AddValidationError);
        expect(() =>
            buildTipsRequest(payload("manual", { manualContent: "\n\t" })),
        ).toThrow(/Manual cards need content/);
        expect(() =>
            buildTipsRequest(payload("casual", { idempotencyKey: "  " })),
        ).toThrow(/non-empty idempotency key/);
    });

    test("accepts only backend-supported image MIME types", () => {
        expect(isAllowedImageType("image/png")).toBe(true);
        expect(isAllowedImageType("image/jpeg")).toBe(true);
        expect(isAllowedImageType("image/webp")).toBe(true);
        expect(isAllowedImageType("image/gif")).toBe(true);
        expect(isAllowedImageType("image/svg+xml")).toBe(false);
        expect(isAllowedImageType("image/PNG")).toBe(false);
    });

    test("rejects unsupported, oversized, and over-count selections as a batch", () => {
        const valid = { type: "image/png", size: 10 };
        expect(selectImages([valid], 0)).toEqual({
            kind: "ok",
            accepted: [valid],
        });
        expect(
            selectImages([{ type: "image/svg+xml", size: 10 }], 0).kind,
        ).toBe("rejected");
        expect(
            selectImages([{ type: "image/png", size: MAX_IMAGE_BYTES + 1 }], 0)
                .kind,
        ).toBe("rejected");
        expect(
            selectImages(
                Array.from({ length: MAX_MANUAL_IMAGES }, () => valid),
                1,
            ).kind,
        ).toBe("rejected");
    });

    test("fits dimensions by scaling only the longest edge", () => {
        expect(fitWithin(800, 600, 2048)).toEqual({ width: 800, height: 600 });
        expect(fitWithin(4096, 2048, 2048)).toEqual({
            width: 2048,
            height: 1024,
        });
        expect(fitWithin(2160, 3840, 2048)).toEqual({
            width: 1152,
            height: 2048,
        });
        expect(fitWithin(0, 100, 2048)).toEqual({ width: 0, height: 100 });
    });
});
