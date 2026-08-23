export interface AdminUser {
    readonly id: string;
    readonly username: string;
    readonly role: string;
    readonly displayName: string | null;
    readonly createdAt: string;
}

export interface TokenSpend {
    readonly daily: number;
    readonly monthly: number;
    readonly total: number;
}

export interface AutoupdateStatus {
    readonly phase: string;
    readonly message: string;
    readonly targetSha: string;
    readonly updatedAt: string;
}

export interface AutoupdateTrigger {
    readonly message: string;
    readonly restarting: boolean;
    readonly updating: boolean;
    readonly targetSha: string | null;
    readonly buildSha: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function asString(value: unknown, field: string): string {
    if (typeof value !== "string") {
        throw new TypeError(`${field} is not a string`);
    }
    return value;
}

function asStringOrNull(value: unknown, field: string): string | null {
    if (value === null) return null;
    return asString(value, field);
}

function asNumber(value: unknown, field: string): number {
    if (typeof value !== "number" || !Number.isFinite(value)) {
        throw new TypeError(`${field} is not a number`);
    }
    return value;
}

function asBoolean(value: unknown, field: string): boolean {
    if (typeof value !== "boolean") {
        throw new TypeError(`${field} is not a boolean`);
    }
    return value;
}

export function parseAdminUser(json: unknown): AdminUser {
    if (!isRecord(json)) throw new TypeError("user is not an object");
    return {
        id: asString(json.id, "id"),
        username: asString(json.username, "username"),
        role: asString(json.role, "role"),
        displayName: asStringOrNull(json.display_name, "display_name"),
        createdAt: asString(json.created_at, "created_at"),
    };
}

export function parseAdminUsers(json: unknown): AdminUser[] {
    if (!Array.isArray(json)) throw new TypeError("users is not an array");
    return json.map(parseAdminUser);
}

export function parseTokenSpend(json: unknown): TokenSpend {
    if (!isRecord(json)) throw new TypeError("token spend is not an object");
    return {
        daily: asNumber(json.daily, "daily"),
        monthly: asNumber(json.monthly, "monthly"),
        total: asNumber(json.total, "total"),
    };
}

export function parseAutoupdateStatus(json: unknown): AutoupdateStatus {
    if (!isRecord(json)) throw new TypeError("autoupdate status is not an object");
    return {
        phase: asString(json.phase, "phase"),
        message: asString(json.message, "message"),
        targetSha: asString(json.target_sha, "target_sha"),
        updatedAt: asString(json.updated_at, "updated_at"),
    };
}

export function isAutoupdateActive(status: AutoupdateStatus): boolean {
    return [
        "starting",
        "queued",
        "checking",
        "preparing",
        "pulling",
        "cloning",
        "compiling",
        "installing",
        "restarting",
        "running",
    ].includes(status.phase);
}

export function isEmptyAutoupdateStatus(status: AutoupdateStatus): boolean {
    return (
        status.phase === "unknown" &&
        status.message === "" &&
        status.updatedAt === ""
    );
}

export function parseAutoupdateTrigger(json: unknown): AutoupdateTrigger {
    if (!isRecord(json)) throw new TypeError("autoupdate result is not an object");
    const target = json.target_sha;
    return {
        message: asString(json.message, "message"),
        restarting: asBoolean(json.restarting, "restarting"),
        updating: asBoolean(json.updating, "updating"),
        targetSha:
            target === null || target === undefined
                ? null
                : asString(target, "target_sha"),
        buildSha: asString(json.build_sha, "build_sha"),
    };
}

async function readJson(
    response: Response,
    fallback: string,
): Promise<unknown> {
    const text = await response.text();
    if (!response.ok) {
        throw new Error(text.trim() === "" ? fallback : text);
    }
    if (text.trim() === "") return null;
    try {
        return JSON.parse(text) as unknown;
    } catch {
        throw new Error(fallback);
    }
}

interface SessionDeps {
    fetchImpl?: typeof fetch;
}

function sessionFetch(
    url: string,
    init: RequestInit,
    fetchImpl: typeof fetch,
): Promise<Response> {
    return fetchImpl(url, {
        ...init,
        credentials: "same-origin",
        headers: { accept: "application/json", ...init.headers },
    });
}

export async function listAdminUsers({
    fetchImpl = fetch,
}: SessionDeps = {}): Promise<AdminUser[]> {
    const response = await sessionFetch("/admin/users", { method: "GET" }, fetchImpl);
    return parseAdminUsers(
        await readJson(response, `List users failed with status ${response.status}`),
    );
}

export async function createAdminUser({
    username,
    password,
    role,
    displayName,
    fetchImpl = fetch,
}: {
    username: string;
    password: string;
    role: string;
    displayName: string;
    fetchImpl?: typeof fetch;
}): Promise<AdminUser> {
    const response = await sessionFetch(
        "/admin/users",
        {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
                username,
                password,
                role,
                display_name: displayName.trim() === "" ? null : displayName.trim(),
            }),
        },
        fetchImpl,
    );
    return parseAdminUser(
        await readJson(response, `Create user failed with status ${response.status}`),
    );
}

export async function updateAdminUser({
    id,
    role,
    password,
    fetchImpl = fetch,
}: {
    id: string;
    role?: string;
    password?: string;
    fetchImpl?: typeof fetch;
}): Promise<AdminUser> {
    const response = await sessionFetch(
        `/admin/users/${encodeURIComponent(id)}`,
        {
            method: "PATCH",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
                role: role ?? null,
                password: password === undefined || password === "" ? null : password,
            }),
        },
        fetchImpl,
    );
    return parseAdminUser(
        await readJson(response, `Update user failed with status ${response.status}`),
    );
}

export async function deleteAdminUser({
    id,
    fetchImpl = fetch,
}: {
    id: string;
    fetchImpl?: typeof fetch;
}): Promise<void> {
    const response = await sessionFetch(
        `/admin/users/${encodeURIComponent(id)}`,
        { method: "DELETE" },
        fetchImpl,
    );
    await readJson(response, `Delete user failed with status ${response.status}`);
}

export async function fetchTokenSpend({
    fetchImpl = fetch,
}: SessionDeps = {}): Promise<TokenSpend> {
    const response = await sessionFetch(
        "/admin/token-spend",
        { method: "GET" },
        fetchImpl,
    );
    return parseTokenSpend(
        await readJson(
            response,
            `Token spend failed with status ${response.status}`,
        ),
    );
}

export async function fetchAutoupdateStatus({
    fetchImpl = fetch,
}: SessionDeps = {}): Promise<AutoupdateStatus | null> {
    const response = await sessionFetch(
        "/admin/autoupdate/status",
        { method: "GET" },
        fetchImpl,
    );
    const status = parseAutoupdateStatus(
        await readJson(
            response,
            `Autoupdate status failed with status ${response.status}`,
        ),
    );
    return isEmptyAutoupdateStatus(status) ? null : status;
}

export async function triggerAutoupdate({
    fetchImpl = fetch,
}: SessionDeps = {}): Promise<AutoupdateTrigger> {
    const response = await sessionFetch(
        "/admin/autoupdate",
        { method: "POST" },
        fetchImpl,
    );
    return parseAutoupdateTrigger(
        await readJson(
            response,
            `Autoupdate check failed with status ${response.status}`,
        ),
    );
}
