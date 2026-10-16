import { describe, expect, test } from "bun:test";
import { ForceDailyRefreshOutcome } from "../../generated/denpie_pb";
import { loadCardsOutcomeMessage } from "./load-cards-outcome";

describe("loadCardsOutcomeMessage", () => {
    test("every concrete outcome has its own message", () => {
        const messages = [
            ForceDailyRefreshOutcome.CARD_AVAILABLE,
            ForceDailyRefreshOutcome.QUEUE_REFILLED,
            ForceDailyRefreshOutcome.NO_CHANGE,
            ForceDailyRefreshOutcome.ACTIVE_LIMIT_REACHED,
        ].map(loadCardsOutcomeMessage);
        expect(messages.every((message) => message !== null)).toBe(true);
        expect(new Set(messages).size).toBe(4);
    });

    test("an unspecified outcome falls back to the generic completion text", () => {
        expect(
            loadCardsOutcomeMessage(ForceDailyRefreshOutcome.UNSPECIFIED),
        ).toBeNull();
    });
});
