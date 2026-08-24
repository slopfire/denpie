export type LabGalleryLayout = "grid" | "list";
export type LabGalleryColumns = 1 | 2 | 3 | 4;
export type LabGalleryViewport = "fluid" | "mobile" | "tablet";
export type LabGalleryTheme = "light" | "dark";

export interface LabGallerySettings {
    readonly filter: string;
    readonly layout: LabGalleryLayout;
    readonly columns: LabGalleryColumns;
    readonly viewport: LabGalleryViewport;
    readonly theme: LabGalleryTheme;
}

export const DEFAULT_LAB_GALLERY_SETTINGS: LabGallerySettings = {
    filter: "",
    layout: "grid",
    columns: 2,
    viewport: "fluid",
    theme: "dark",
};

function columns(value: string | null): LabGalleryColumns {
    if (value === "1") return 1;
    if (value === "3") return 3;
    if (value === "4") return 4;
    return 2;
}

export function parseLabGallerySettings(search: string): LabGallerySettings {
    const query = new URLSearchParams(search);
    const layout = query.get("layout");
    const viewport = query.get("viewport");
    const theme = query.get("theme");
    return {
        filter: query.get("fixture")?.trim() ?? "",
        layout: layout === "list" ? "list" : "grid",
        columns: columns(query.get("columns")),
        viewport:
            viewport === "mobile" || viewport === "tablet" ? viewport : "fluid",
        theme: theme === "light" ? "light" : "dark",
    };
}

export function labGallerySearch(settings: LabGallerySettings): string {
    const query = new URLSearchParams();
    if (settings.filter !== "") query.set("fixture", settings.filter);
    if (settings.layout !== DEFAULT_LAB_GALLERY_SETTINGS.layout) {
        query.set("layout", settings.layout);
    }
    if (settings.columns !== DEFAULT_LAB_GALLERY_SETTINGS.columns) {
        query.set("columns", String(settings.columns));
    }
    if (settings.viewport !== DEFAULT_LAB_GALLERY_SETTINGS.viewport) {
        query.set("viewport", settings.viewport);
    }
    if (settings.theme !== DEFAULT_LAB_GALLERY_SETTINGS.theme) {
        query.set("theme", settings.theme);
    }
    const serialized = query.toString();
    return serialized === "" ? "" : `?${serialized}`;
}

export function matchesLabFixture(
    filter: string,
    values: readonly string[],
): boolean {
    const normalize = (value: string) =>
        value.trim().toLocaleLowerCase().replaceAll(/[-_]+/g, " ");
    const needle = normalize(filter);
    return (
        needle === "" ||
        values.some((value) => normalize(value).includes(needle))
    );
}
