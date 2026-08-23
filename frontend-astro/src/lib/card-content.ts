export type CardContentKind = "normal" | "llm_error" | "api_key_missing";

/** Detect LLM/API-key failure copy in card text. */
export function detectCardContentKind(text: string): CardContentKind {
    const trimmed = text.trim();
    if (trimmed === "") return "normal";
    const lower = trimmed.toLocaleLowerCase();
    if (lower.includes("api key missing")) return "api_key_missing";
    if (
        lower.startsWith("llm error:") ||
        lower.includes("\nllm error:") ||
        lower.includes("llm error: http")
    ) {
        return "llm_error";
    }
    return "normal";
}

export function cardErrorDetail(
    kind: CardContentKind,
    fullContent: string,
    compressedContent: string,
): string {
    if (kind === "normal") return "";
    const primary = fullContent.trim();
    return primary === "" ? compressedContent.trim() : primary;
}

export function filterTopicsByName<T extends { name: string }>(
    topics: readonly T[],
    query: string,
): T[] {
    const needle = query.trim().toLocaleLowerCase();
    if (needle === "") return [...topics];
    return topics.filter((topic) =>
        topic.name.toLocaleLowerCase().includes(needle),
    );
}
