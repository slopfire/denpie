// Pure pinned-card ordering model. Card IDs are generated bigints, so every
// operation preserves exact decimal identity beyond `Number.MAX_SAFE_INTEGER`.
// No mutation of any input.

/** Canonical localStorage key for pinned-card order. */
export const PINNED_CARD_ORDER_STORAGE_KEY = "denpie-pinned-card-order";

const I64_MAX = 9_223_372_036_854_775_807n;

/** Parse one untrusted card ID using the protobuf signed-i64 boundary. */
export function parsePinnedCardId(value: string): bigint | null {
    const trimmed = value.trim();
    if (!/^[1-9]\d*$/.test(trimmed)) return null;
    const id = BigInt(trimmed);
    return id <= I64_MAX ? id : null;
}

/**
 * Boundary parser for the stored order. The canonical form is a raw
 * numeric JSON array (e.g. `[3,2,1]`); it is parsed as text so decimal IDs
 * beyond `Number.MAX_SAFE_INTEGER` survive without a JavaScript `Number`
 * round-trip. Returns `null` for missing or malformed input; rejects
 * non-integers, non-positive values, and duplicates cleanly.
 */
export function parsePinnedCardOrder(value: string | null): bigint[] | null {
    if (value === null) return null;
    const trimmed = value.trim();
    const inner =
        trimmed.startsWith("[") && trimmed.endsWith("]")
            ? trimmed.slice(1, -1)
            : null;
    if (inner === null) return null;
    // Strict JSON grammar: zero or more comma-separated positive decimal
    // integers without leading zeroes.
    // Anything else (`1.5`, `-1`, strings, `null`, doubles) is malformed.
    if (!/^(\s*[1-9]\d*\s*(,\s*[1-9]\d*\s*)*)?$/.test(inner)) return null;
    const seen = new Set<bigint>();
    const order: bigint[] = [];
    for (const token of inner.match(/[1-9]\d*/g) ?? []) {
        const id = parsePinnedCardId(token);
        if (id === null || seen.has(id)) return null;
        seen.add(id);
        order.push(id);
    }
    return order;
}

/** Serialize to a raw JSON integer array so the parser can read it back exactly. */
export function serializePinnedCardOrder(order: readonly bigint[]): string {
    return `[${order.map((id) => id.toString()).join(",")}]`;
}

/**
 * Discard saved IDs that are not currently in `currentIds`, retain saved
 * relative order, and append newly seen current IDs in current source order.
 */
export function normalizeCardOrder(
    saved: readonly bigint[],
    currentIds: readonly bigint[],
): bigint[] {
    const currentSet = new Set(currentIds);
    const normalized: bigint[] = [];
    const present = new Set<bigint>();
    for (const id of saved) {
        if (currentSet.has(id) && !present.has(id)) {
            normalized.push(id);
            present.add(id);
        }
    }
    for (const id of currentIds) {
        if (!present.has(id)) {
            normalized.push(id);
            present.add(id);
        }
    }
    return normalized;
}

/**
 * Compute source and target indexes in the current order, remove the
 * source, then insert at the original target index. Unknown or equal IDs
 * are a no-op that returns the same input reference.
 */
export function movePinnedCard(
    order: readonly bigint[],
    sourceId: bigint,
    targetId: bigint,
): readonly bigint[] {
    if (sourceId === targetId) return order;
    const sourceIndex = order.indexOf(sourceId);
    const targetIndex = order.indexOf(targetId);
    if (sourceIndex < 0 || targetIndex < 0) return order;
    const next = [...order];
    const [moved] = next.splice(sourceIndex, 1);
    next.splice(targetIndex, 0, moved);
    return next;
}

/**
 * Replace an old card ID with a new one at the same saved-order position —
 * the stable review/Continue/refill slot replacement rule. Unknown old IDs
 * and IDs already present elsewhere are a no-op returning the same input
 * reference.
 */
export function replacePinnedCard(
    order: readonly bigint[],
    oldId: bigint,
    newId: bigint,
): readonly bigint[] {
    if (!order.includes(oldId) || order.includes(newId)) return order;
    return order.map((id) => (id === oldId ? newId : id));
}

/** One saved-order identity transfer, normally for repeatable replacement. */
export interface PinnedCardTransfer {
    from: bigint;
    to: bigint;
}

export type PinnedCardTransferResult =
    { kind: "applied"; order: readonly bigint[] } | { kind: "collision" };

/**
 * Atomically transfer multiple saved pinned positions. A destination that is
 * already retained by the order, or duplicate source/destination identities,
 * is a collision: no portion of the batch is applied. A source absent from
 * the saved order needs no transfer and is harmless.
 */
export function transferPinnedCards(
    order: readonly bigint[],
    transfers: readonly PinnedCardTransfer[],
): PinnedCardTransferResult {
    const active = transfers.filter(
        (transfer) =>
            transfer.from !== transfer.to && order.includes(transfer.from),
    );
    const sources = new Set<bigint>();
    const destinations = new Set<bigint>();
    for (const transfer of active) {
        if (sources.has(transfer.from) || destinations.has(transfer.to)) {
            return { kind: "collision" };
        }
        sources.add(transfer.from);
        destinations.add(transfer.to);
    }
    for (const transfer of active) {
        if (order.includes(transfer.to) && !sources.has(transfer.to)) {
            return { kind: "collision" };
        }
    }
    if (active.length === 0) return { kind: "applied", order };
    const replacements = new Map(
        active.map((transfer) => [transfer.from, transfer.to]),
    );
    return {
        kind: "applied",
        order: order.map((id) => replacements.get(id) ?? id),
    };
}

/**
 * Pin/unpin maintenance: pinning appends a new ID; unpinning removes it.
 * Appending an existing ID is a no-op returning the same input reference.
 */
export function setPinnedMembership(
    order: readonly bigint[],
    cardId: bigint,
    pinned: boolean,
): readonly bigint[] {
    if (pinned) {
        if (order.includes(cardId)) return order;
        return [...order, cardId];
    }
    if (!order.includes(cardId)) return order;
    return order.filter((id) => id !== cardId);
}
