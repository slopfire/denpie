export type ToastKind = "error" | "success" | "info";

export interface ToastParts {
    readonly summary: string;
    readonly detail: string | undefined;
    readonly kind: ToastKind;
}

export interface ShownToast extends ToastParts {
    readonly id: number;
}

const ERROR_MARKERS = [
    "fail",
    "error",
    "invalid",
    "unable",
    "denied",
    "unauthorized",
    "forbidden",
    "not found",
    "timeout",
    "panic",
    "could not",
    "cannot",
    "unavailable",
    "missing",
    "expired",
    "refused",
    "rate limit",
    "too many requests",
] as const;

/** Error-like copy never auto-hides, even if the caller said success. */
export function looksLikeError(message: string): boolean {
    const lower = message.toLocaleLowerCase();
    return (
        lower.startsWith("llm error") ||
        lower.includes("api key missing") ||
        ERROR_MARKERS.some((marker) => lower.includes(marker))
    );
}

const HEAD_LIMIT = 120;

export function splitToastParts(message: string): {
    summary: string;
    detail: string | undefined;
} {
    const trimmed = message.trim();
    if (trimmed === "") return { summary: "", detail: undefined };
    const newline = trimmed.indexOf("\n");
    if (newline !== -1) {
        const head = trimmed.slice(0, newline).trim();
        const rest = trimmed.slice(newline + 1).trim();
        if (rest !== "") return { summary: head, detail: rest };
    }
    if ([...trimmed].length <= HEAD_LIMIT) {
        return { summary: trimmed, detail: undefined };
    }
    let head = [...trimmed].slice(0, HEAD_LIMIT).join("");
    while (/[\s,:{]$/.test(head)) head = head.slice(0, -1);
    return { summary: `${head}…`, detail: trimmed };
}

export function toastTimeoutMs(kind: ToastKind): number | null {
    switch (kind) {
        case "error":
            return null;
        case "success":
            return 2400;
        case "info":
            return 2800;
    }
}

export function classifyToast(
    message: string,
    kind: ToastKind = "info",
): ToastParts {
    const parts = splitToastParts(message);
    return {
        summary: parts.summary,
        detail: parts.detail,
        kind: looksLikeError(parts.summary) ? "error" : kind,
    };
}
