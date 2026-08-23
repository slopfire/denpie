/** localStorage key: a JSON array of usernames, newest first. */
export const REMEMBERED_ACCOUNTS_KEY = "denpie.remembered_accounts";
export const PREFILL_USERNAME_KEY = "denpie.prefill_username";
export const REMEMBERED_MAX = 5;

function isUsernameList(value: unknown): value is string[] {
    return (
        Array.isArray(value) &&
        value.every((entry) => typeof entry === "string")
    );
}

/** Parse the remembered-account list; unknown JSON becomes empty. */
export function parseRememberedAccounts(raw: string | null): string[] {
    if (raw === null || raw.trim() === "") return [];
    try {
        const parsed: unknown = JSON.parse(raw);
        if (!isUsernameList(parsed)) return [];
        const seen = new Set<string>();
        const names: string[] = [];
        for (const entry of parsed) {
            const name = entry.trim();
            if (name === "" || seen.has(name)) continue;
            seen.add(name);
            names.push(name);
            if (names.length === REMEMBERED_MAX) break;
        }
        return names;
    } catch {
        return [];
    }
}

/** Move `name` to the front, drop blanks/duplicates, cap at REMEMBERED_MAX. */
export function recordRememberedAccount(
    accounts: readonly string[],
    name: string,
): string[] {
    const username = name.trim();
    if (username === "") return [...accounts];
    return [
        username,
        ...accounts.filter((entry) => entry !== username),
    ].slice(0, REMEMBERED_MAX);
}

export function otherRememberedAccounts(
    accounts: readonly string[],
    currentUsername: string,
): string[] {
    return accounts.filter((name) => name !== currentUsername);
}

export function loadRememberedAccounts(
    storage: Pick<Storage, "getItem"> = window.localStorage,
): string[] {
    return parseRememberedAccounts(storage.getItem(REMEMBERED_ACCOUNTS_KEY));
}

export function saveRememberedAccounts(
    accounts: readonly string[],
    storage: Pick<Storage, "setItem"> = window.localStorage,
): void {
    storage.setItem(REMEMBERED_ACCOUNTS_KEY, JSON.stringify(accounts));
}

export function consumePrefillUsername(
    storage: Pick<Storage, "getItem" | "removeItem"> = window.localStorage,
): string {
    const value = storage.getItem(PREFILL_USERNAME_KEY) ?? "";
    storage.removeItem(PREFILL_USERNAME_KEY);
    return value;
}

export function storePrefillUsername(
    name: string,
    storage: Pick<Storage, "setItem"> = window.localStorage,
): void {
    storage.setItem(PREFILL_USERNAME_KEY, name);
}
