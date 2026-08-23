import { t, type MessageKey } from "@/lib/i18n";

const TIP_TYPE_KEYS = {
    casual_tip: "tip_type.casual_tip",
    repeatable_tip: "tip_type.repeatable_tip",
    manual_tip: "tip_type.manual_tip",
    custom_tip: "tip_type.custom_tip",
} as const satisfies Record<string, MessageKey>;

/** Map known protocol tip types; unknown IDs stay visible as the raw value. */
export function tipTypeLabel(tipcardType: string): string {
    if (tipcardType === "") return t("common.unspecified");
    const key = TIP_TYPE_KEYS[tipcardType as keyof typeof TIP_TYPE_KEYS];
    return key === undefined ? tipcardType : t(key);
}
