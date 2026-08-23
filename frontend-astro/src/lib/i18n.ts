import english from "../i18n/en.json";

export const supportedLocales = ["en"] as const;

export type Locale = (typeof supportedLocales)[number];
export type MessageKey = keyof typeof english;
export type MessageArguments = Readonly<Record<string, string | number>>;

const catalogs = { en: english } satisfies Record<
    Locale,
    Readonly<Record<string, string>>
>;

export function catalogFor(locale: Locale = "en") {
    return catalogs[locale];
}

export function t(key: MessageKey, locale: Locale = "en"): string {
    return catalogFor(locale)[key];
}

export function tf(
    key: MessageKey,
    args: MessageArguments,
    locale: Locale = "en",
): string {
    let message = t(key, locale);
    for (const [name, value] of Object.entries(args)) {
        message = message.replaceAll(`{${name}}`, String(value));
    }
    return message;
}
