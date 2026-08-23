import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { FlowCardInfoSchema, type FlowCardInfo } from "../generated/denpie_pb";
import {
    FLOW_SNAPSHOT_KEY,
    loadFlowSnapshot,
    saveFlowSnapshot,
    MAX_AGE_MS,
    type SavedFlowPage,
} from "./flow-snapshot";

/** In-memory Storage fake matching the surface the snapshot module uses. */
function fakeStorage(): Storage & { data: Map<string, string> } {
    const data = new Map<string, string>();
    return {
        data,
        getItem: (key: string) => data.get(key) ?? null,
        setItem: (key: string, value: string) => void data.set(key, value),
        removeItem: (key: string) => void data.delete(key),
        clear: () => data.clear(),
        key: () => null,
        get length() {
            return data.size;
        },
    };
}

function card(id: bigint): FlowCardInfo {
    return create(FlowCardInfoSchema, {
        id,
        topicName: "rust",
        title: `card-${id}`,
        pinned: id % 2n === 0n,
    });
}

const NOW = 1_000_000_000;

function page(cards: bigint[], cursor: SavedFlowPage["cursor"]): SavedFlowPage {
    return { savedAt: NOW, cards: cards.map(card), cursor };
}

describe("saveFlowSnapshot / loadFlowSnapshot", () => {
    test("round-trips cards with exact bigint IDs", () => {
        const storage = fakeStorage();
        const hugeId = 9223372036854775807n; // int64 max
        const original = page([1n, 2n, hugeId], {
            kind: "more",
            pageToken: "tok-1",
        });
        saveFlowSnapshot(original, storage);
        const loaded = loadFlowSnapshot(NOW, storage);
        expect(loaded).not.toBeNull();
        expect(loaded?.cards.map((c) => c.id)).toEqual([1n, 2n, hugeId]);
        // A JS-number round trip would have corrupted this ID.
        expect(Number.MAX_SAFE_INTEGER < hugeId).toBe(true);
        expect(loaded?.cursor).toEqual({ kind: "more", pageToken: "tok-1" });
        expect(loaded?.savedAt).toBe(NOW);
    });

    test("end cursor survives the round trip", () => {
        const storage = fakeStorage();
        saveFlowSnapshot(page([5n], { kind: "end" }), storage);
        expect(loadFlowSnapshot(NOW, storage)?.cursor).toEqual({
            kind: "end",
        });
    });

    test("uses the denpie-flow-snapshot key", () => {
        const storage = fakeStorage();
        saveFlowSnapshot(page([], { kind: "end" }), storage);
        expect(storage.data.has(FLOW_SNAPSHOT_KEY)).toBe(true);
    });

    test("no-op without a window and without injected storage", () => {
        // No globalThis.window under bun test: must not throw.
        saveFlowSnapshot(page([], { kind: "end" }));
        expect(loadFlowSnapshot()).toBeNull();
    });
});

describe("loadFlowSnapshot staleness", () => {
    test("MAX_AGE_MS is ten minutes", () => {
        expect(MAX_AGE_MS).toBe(10 * 60_000);
    });

    test("fresh within MAX_AGE_MS loads", () => {
        const storage = fakeStorage();
        saveFlowSnapshot(page([1n], { kind: "end" }), storage);
        expect(loadFlowSnapshot(NOW + MAX_AGE_MS, storage)?.cards).toHaveLength(
            1,
        );
    });

    test("older than MAX_AGE_MS returns null and removes the entry", () => {
        const storage = fakeStorage();
        saveFlowSnapshot(page([1n], { kind: "end" }), storage);
        expect(loadFlowSnapshot(NOW + MAX_AGE_MS + 1, storage)).toBeNull();
        expect(storage.data.has(FLOW_SNAPSHOT_KEY)).toBe(false);
    });

    test("negative age (clock skew into the future) still loads", () => {
        const storage = fakeStorage();
        saveFlowSnapshot(page([1n], { kind: "end" }), storage);
        expect(loadFlowSnapshot(NOW - 60_000, storage)?.cards).toHaveLength(1);
    });
});

describe("loadFlowSnapshot corruption handling", () => {
    test("malformed JSON returns null and removes the entry", () => {
        const storage = fakeStorage();
        storage.setItem(FLOW_SNAPSHOT_KEY, "{not json");
        expect(loadFlowSnapshot(NOW, storage)).toBeNull();
        expect(storage.data.has(FLOW_SNAPSHOT_KEY)).toBe(false);
    });

    test("wrong version returns null and removes the entry", () => {
        const storage = fakeStorage();
        storage.setItem(
            FLOW_SNAPSHOT_KEY,
            JSON.stringify({ version: 999, savedAt: NOW, cards: [] }),
        );
        expect(loadFlowSnapshot(NOW, storage)).toBeNull();
        expect(storage.data.has(FLOW_SNAPSHOT_KEY)).toBe(false);
    });

    test("invalid cursor shape is rejected", () => {
        const storage = fakeStorage();
        storage.setItem(
            FLOW_SNAPSHOT_KEY,
            JSON.stringify({
                version: 1,
                savedAt: NOW,
                cursor: { kind: "sideways" },
                cards: [],
            }),
        );
        expect(loadFlowSnapshot(NOW, storage)).toBeNull();
    });

    test("non-string pageToken cursor is rejected", () => {
        const storage = fakeStorage();
        storage.setItem(
            FLOW_SNAPSHOT_KEY,
            JSON.stringify({
                version: 1,
                savedAt: NOW,
                cursor: { kind: "more", pageToken: 42 },
                cards: [],
            }),
        );
        expect(loadFlowSnapshot(NOW, storage)).toBeNull();
    });

    test("schema-invalid cards are rejected wholesale", () => {
        const storage = fakeStorage();
        storage.setItem(
            FLOW_SNAPSHOT_KEY,
            JSON.stringify({
                version: 1,
                savedAt: NOW,
                cursor: { kind: "end" },
                cards: [{ id: "not-a-number", topicName: "x" }],
            }),
        );
        expect(loadFlowSnapshot(NOW, storage)).toBeNull();
        expect(storage.data.has(FLOW_SNAPSHOT_KEY)).toBe(false);
    });
});
