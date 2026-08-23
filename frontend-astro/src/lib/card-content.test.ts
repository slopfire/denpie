import { describe, expect, test } from "bun:test";
import {
    cardErrorDetail,
    detectCardContentKind,
    filterTopicsByName,
} from "./card-content";

describe("card content kind", () => {
    test("classifies LLM and missing-key failures", () => {
        expect(detectCardContentKind("LLM Error: HTTP 429")).toBe("llm_error");
        expect(detectCardContentKind("prefix\nLLM Error: boom")).toBe(
            "llm_error",
        );
        expect(detectCardContentKind("API key missing for OpenRouter")).toBe(
            "api_key_missing",
        );
        expect(detectCardContentKind("A normal tip about SQL")).toBe("normal");
    });

    test("prefers full content for the expandable error detail", () => {
        expect(
            cardErrorDetail("llm_error", "LLM Error: HTTP 500", "compressed"),
        ).toBe("LLM Error: HTTP 500");
        expect(cardErrorDetail("api_key_missing", "", "API key missing")).toBe(
            "API key missing",
        );
        expect(cardErrorDetail("normal", "body", "compact")).toBe("");
    });
});

describe("topic search", () => {
    test("filters by name case-insensitively", () => {
        const topics = [{ name: "Rust" }, { name: "Python" }, { name: "Go" }];
        expect(filterTopicsByName(topics, "ru")).toEqual([{ name: "Rust" }]);
        expect(filterTopicsByName(topics, "  ")).toEqual(topics);
    });
});
