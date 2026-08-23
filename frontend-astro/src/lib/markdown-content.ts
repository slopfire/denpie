/** Markdown URL policy shared by rendered card content. */
export function safeMarkdownUrl(value: string): string {
    const url = value.trim();
    if (
        url.startsWith("https:") ||
        url.startsWith("http:") ||
        url.startsWith("mailto:") ||
        url.startsWith("/") ||
        url.startsWith("#") ||
        url.startsWith("?")
    ) {
        return url;
    }
    return "#";
}

/** Normalize fenced-code aliases to Prism's conventional language names. */
export function prismLanguage(value: string | undefined): string {
    const raw = (value ?? "")
        .trim()
        .split(/\s+/, 1)[0]
        ?.replace(/^language-/, "")
        .replace(/[{}]/g, "")
        .toLowerCase();

    switch (raw) {
        case "rs":
        case "rust":
            return "rust";
        case "sh":
        case "shell":
        case "zsh":
        case "fish":
        case "console":
        case "terminal":
        case "bash":
            return "bash";
        case "js":
        case "jsx":
        case "javascript":
            return "javascript";
        case "ts":
        case "tsx":
        case "typescript":
            return "typescript";
        case "py":
        case "python":
            return "python";
        case "yml":
        case "yaml":
            return "yaml";
        case "md":
        case "markdown":
            return "markdown";
        case "c++":
        case "cc":
        case "cpp":
        case "cxx":
            return "cpp";
        case "cs":
        case "csharp":
        case "c#":
            return "csharp";
        case "go":
        case "golang":
            return "go";
        case "html":
        case "htm":
        case "css":
        case "json":
        case "toml":
        case "sql":
        case "java":
        case "kotlin":
        case "lua":
        case "php":
        case "ruby":
        case "swift":
        case "diff":
        case "ini":
        case "dockerfile":
            return raw;
        default:
            return "plaintext";
    }
}

/** Fences become collapsible once they exceed the old card's five-line threshold. */
export function isLongCodeFence(value: string): boolean {
    return value.split("\n").length > 5;
}
