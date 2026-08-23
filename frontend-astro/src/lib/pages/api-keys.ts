import type { ApiKeyInfo } from "@/generated/denpie_pb";

/** Keep API-key inventory deterministic when the server returns a new order. */
export function sortApiKeys(keys: readonly ApiKeyInfo[]): ApiKeyInfo[] {
    return [...keys].sort((left, right) => {
        const byCreated = right.createdAt.localeCompare(left.createdAt);
        return byCreated !== 0
            ? byCreated
            : right.id < left.id
              ? -1
              : right.id > left.id
                ? 1
                : 0;
    });
}

export function apiKeyDomId(id: bigint): string {
    return `api-key-${id.toString()}`;
}

export function formatApiKeyDate(value: string): string {
    if (value.trim() === "") return "Never";
    const date = new Date(value);
    return Number.isNaN(date.getTime())
        ? value
        : new Intl.DateTimeFormat(undefined, {
              dateStyle: "medium",
              timeStyle: "short",
          }).format(date);
}
