import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import {
    FlowCardInfoSchema,
    TipcardImageInfoSchema,
    type FlowCardInfo,
} from "../generated/denpie_pb";
import {
    cardImageUrls,
    mergeCardsById,
    repeatableStackLayers,
    toFlowCardView,
} from "./flow-view";

function card(overrides: Partial<FlowCardInfo> = {}): FlowCardInfo {
    return create(FlowCardInfoSchema, { id: 1n, title: "t", ...overrides });
}

describe("mergeCardsById", () => {
    test("appends new pages in order", () => {
        const merged = mergeCardsById([card({ id: 1n })], [card({ id: 2n })]);
        expect(merged.map((c) => c.id)).toEqual([1n, 2n]);
    });

    test("deduplicates overlapping bigint IDs (first occurrence wins)", () => {
        const existing = [card({ id: 1n }), card({ id: 2n })];
        const incoming = [card({ id: 2n, title: "dup" }), card({ id: 3n })];
        const merged = mergeCardsById(existing, incoming);
        expect(merged.map((c) => c.id)).toEqual([1n, 2n, 3n]);
        expect(merged[1].title).toBe("t");
    });

    test("does not mutate inputs", () => {
        const existing = [card({ id: 1n })];
        mergeCardsById(existing, [card({ id: 2n })]);
        expect(existing).toHaveLength(1);
    });
});

describe("toFlowCardView", () => {
    test("compressed content with full-content fallback", () => {
        expect(
            toFlowCardView(card({ compressedContent: "c", fullContent: "f" }))
                .content,
        ).toBe("c");
        expect(
            toFlowCardView(card({ compressedContent: "", fullContent: "f" }))
                .content,
        ).toBe("f");
    });

    test("protocol type/status map to human labels; unknown raw values pass through", () => {
        const view = toFlowCardView(
            card({ tipcardType: "repeatable_tip", status: "pending" }),
        );
        expect(view.typeLabel).toBe("Repeatable tip");
        expect(view.statusLabel).toBe("Pending");
        expect(
            toFlowCardView(card({ tipcardType: "manual_tip" })).typeLabel,
        ).toBe("Manual tip");
        expect(
            toFlowCardView(card({ tipcardType: "custom_tip" })).typeLabel,
        ).toBe("Custom tip");
        expect(toFlowCardView(card({ status: "learned" })).statusLabel).toBe(
            "Learned",
        );
        expect(toFlowCardView(card({ status: "dismissed" })).statusLabel).toBe(
            "Dismissed",
        );
        expect(toFlowCardView(card({ status: "custom" })).statusLabel).toBe(
            "Custom",
        );
        const unknown = toFlowCardView(
            card({ tipcardType: "exotic", status: "" }),
        );
        // Unknown nonempty values fall back to the raw protocol text; an empty
        // value never renders as a blank footer label.
        expect(unknown.typeLabel).toBe("exotic");
        expect(unknown.statusLabel).toBe("Unspecified");
    });

    test("pinned and repeatable metadata are preserved", () => {
        const view = toFlowCardView(
            card({ pinned: true, repeatCount: 4, pendingCount: 3n }),
        );
        expect(view.pinned).toBe(true);
        expect(view.repeatCount).toBe(4);
        expect(view.pendingCount).toBe(3n);
    });

    test("image URLs come from each generated downloadPath", () => {
        const images = [
            create(TipcardImageInfoSchema, {
                id: 7n,
                downloadPath: "/img/a.png",
            }),
            create(TipcardImageInfoSchema, { id: 9n }),
        ];
        expect(cardImageUrls({ images })).toEqual([
            "/img/a.png",
            "/api/v1/tipcard-images/9",
        ]);
    });
});

describe("repeatableStackLayers", () => {
    test("shows one through three pending layers only for repeatable cards", () => {
        expect(
            repeatableStackLayers({
                tipcardType: "repeatable_tip",
                pendingCount: 1n,
            }),
        ).toBe(1);
        expect(
            repeatableStackLayers({
                tipcardType: "repeatable_tip",
                pendingCount: 2n,
            }),
        ).toBe(2);
        expect(
            repeatableStackLayers({
                tipcardType: "repeatable_tip",
                pendingCount: 8n,
            }),
        ).toBe(3);
        expect(
            repeatableStackLayers({
                tipcardType: "casual_tip",
                pendingCount: 8n,
            }),
        ).toBe(0);
        expect(
            repeatableStackLayers({
                tipcardType: "repeatable_tip",
                pendingCount: 0n,
            }),
        ).toBe(0);
    });
});
