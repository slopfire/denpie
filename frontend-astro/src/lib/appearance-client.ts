export interface AppearancePreferences {
    readonly theme: string;
    readonly transparency: string;
    readonly blur: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseAppearancePreferences(
    value: unknown,
): AppearancePreferences {
    if (
        !isRecord(value) ||
        typeof value.color_scheme !== "string" ||
        typeof value.transparency !== "string" ||
        typeof value.blur_intensity !== "string"
    ) {
        throw new TypeError("Settings returned invalid appearance preferences");
    }
    return {
        theme: value.color_scheme,
        transparency: value.transparency,
        blur: value.blur_intensity,
    };
}

export async function fetchAppearancePreferences(
    fetchImpl: typeof fetch = fetch,
): Promise<AppearancePreferences> {
    const response = await fetchImpl("/admin/settings", {
        method: "GET",
        credentials: "same-origin",
        headers: { accept: "application/json" },
    });
    if (!response.ok) {
        throw new Error(
            `Appearance settings failed with status ${response.status}`,
        );
    }
    return parseAppearancePreferences(await response.json());
}
