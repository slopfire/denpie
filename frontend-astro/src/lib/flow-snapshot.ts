/**
 * Session-scoped snapshot of the first Flow page, written after a successful
 * load and read back before the network on the next visit in the same tab.
 * Serialization goes through the generated protobuf JSON mapping so bigint
 * card IDs survive the round trip exactly.
 */
import { fromJson, toJson } from "@bufbuild/protobuf";
import {
    FlowCardInfoSchema,
    type FlowCardInfo,
} from "../generated/denpie_pb";
import type { FlowCursor } from "./flow-state";

export const FLOW_SNAPSHOT_KEY = "denpie-flow-snapshot";

/** Snapshots older than this are treated as missing. */
export const MAX_AGE_MS = 10 * 60_000;

/** Minimal storage surface; tests inject fakes, browsers get sessionStorage. */
type SnapshotStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

function resolveStorage(storage?: SnapshotStorage): SnapshotStorage | null {
    if (storage !== undefined) return storage;
    // SSR/tests without DOM: no snapshot.
    return typeof window === "undefined" ? null : window.sessionStorage;
}

/** A saved first Flow page: cards plus the cursor for page 2. */
export interface SavedFlowPage {
    savedAt: number;
    cards: FlowCardInfo[];
    cursor: FlowCursor;
}

interface StoredSnapshot {
    version: typeof SNAPSHOT_VERSION;
    savedAt: number;
    cursor: FlowCursor;
    cards: unknown[];
}

const SNAPSHOT_VERSION = 1;

function isFlowCursor(value: unknown): value is FlowCursor {
    if (typeof value !== "object" || value === null) return false;
    const candidate = value as Record<string, unknown>;
    if (candidate.kind === "end") return true;
    return candidate.kind === "more" && typeof candidate.pageToken === "string";
}

/**
 * Persist `page` under {@link FLOW_SNAPSHOT_KEY}. Cards are serialized via
 * protobuf JSON (`toJson(FlowCardInfoSchema)`), which maps int64 IDs to
 * decimal strings — no precision loss. Quota errors are swallowed.
 */
export function saveFlowSnapshot(page: SavedFlowPage, storage?: SnapshotStorage): void {
    const target = resolveStorage(storage);
    if (target === null) return;
    try {
        const stored: StoredSnapshot = {
            version: SNAPSHOT_VERSION,
            savedAt: page.savedAt,
            cursor: page.cursor,
            cards: page.cards.map((card) =>
                toJson(FlowCardInfoSchema, card),
            ),
        };
        target.setItem(FLOW_SNAPSHOT_KEY, JSON.stringify(stored));
    } catch {
        // Quota exceeded or serialization failure: the snapshot is best-effort.
    }
}

/**
 * Load the snapshot saved by {@link saveFlowSnapshot}, or `null` when it is
 * absent, older than {@link MAX_AGE_MS} (removed on sight), malformed, or
 * fails schema validation. Never throws.
 */
export function loadFlowSnapshot(now?: number, storage?: SnapshotStorage): SavedFlowPage | null {
    const target = resolveStorage(storage);
    if (target === null) return null;
    let raw: string | null;
    try {
        raw = target.getItem(FLOW_SNAPSHOT_KEY);
    } catch {
        return null;
    }
    if (raw === null) return null;

    // Read-and-clear semantics: every non-reusable outcome drops the entry.
    const drop = () => {
        try {
            target.removeItem(FLOW_SNAPSHOT_KEY);
        } catch {
            // Removal failure must not mask the null result.
        }
    };

    try {
        const parsed: unknown = JSON.parse(raw);
        if (typeof parsed !== "object" || parsed === null) {
            drop();
            return null;
        }
        const record = parsed as Record<string, unknown>;
        const { version, savedAt, cursor, cards } = record as {
            version?: unknown;
            savedAt?: unknown;
            cursor?: unknown;
            cards?: unknown;
        };
        if (
            version !== SNAPSHOT_VERSION ||
            typeof savedAt !== "number" ||
            !Number.isFinite(savedAt)
        ) {
            drop();
            return null;
        }
        if ((now ?? Date.now()) - savedAt > MAX_AGE_MS) {
            drop();
            return null;
        }
        if (!isFlowCursor(cursor) || !Array.isArray(cards)) {
            drop();
            return null;
        }
        const restored = cards.map((entry) => fromJson(FlowCardInfoSchema, entry));
        return { savedAt, cards: restored, cursor };
    } catch {
        drop();
        return null;
    }
}
