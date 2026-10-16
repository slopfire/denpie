// What a Grounding "Load" click achieved, in the user's words. The server
// answers a load with an outcome; reporting every outcome as "complete" hid
// that nothing was loaded (limit reached, nothing pending).

import { ForceDailyRefreshOutcome } from "../../generated/denpie_pb";
import { t } from "../i18n";

export function loadCardsOutcomeMessage(
    outcome: ForceDailyRefreshOutcome,
): string | null {
    switch (outcome) {
        case ForceDailyRefreshOutcome.CARD_AVAILABLE:
            return t("toast.card_loaded");
        case ForceDailyRefreshOutcome.QUEUE_REFILLED:
            return t("toast.card_queue_refilled");
        case ForceDailyRefreshOutcome.NO_CHANGE:
            return t("toast.force_refresh_no_change");
        case ForceDailyRefreshOutcome.ACTIVE_LIMIT_REACHED:
            return t("toast.active_card_limit_reached");
        default:
            return null;
    }
}
