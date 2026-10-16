import { describe, expect, test } from "bun:test";
import {
    clearGroundingSearchOverride,
    customizeGroundingSearch,
    legacyGroundingPolicy,
    legacyStrategyForPolicy,
    hasGroundingSearchOverride,
    normalizeBehavior,
    parseDomainList,
    resolveGroundingPolicy,
    sourceUsesDocuments,
    sourceUsesWeb,
} from "./grounding-policy";

describe("grounding policy", () => {
    test("maps every legacy strategy onto independent source and behavior", () => {
        expect(legacyGroundingPolicy("factual")).toEqual({
            sourceMode: "model",
            behavior: "standard",
        });
        expect(legacyGroundingPolicy("agentic")).toEqual({
            sourceMode: "web",
            behavior: "agentic",
        });
        expect(legacyGroundingPolicy("create_and_ground")).toEqual({
            sourceMode: "web",
            behavior: "fact_check",
        });
        expect(legacyGroundingPolicy("rag")).toEqual({
            sourceMode: "documents",
            behavior: "standard",
        });
    });

    test("models web and document capabilities without strategy-name checks", () => {
        expect(sourceUsesWeb("web_and_documents")).toBe(true);
        expect(sourceUsesDocuments("web_and_documents")).toBe(true);
        expect(sourceUsesWeb("documents")).toBe(false);
        expect(sourceUsesDocuments("web")).toBe(false);
    });

    test("model knowledge cannot claim agentic research", () => {
        expect(normalizeBehavior("model", "agentic")).toBe("standard");
        expect(
            resolveGroundingPolicy({
                sourceMode: "model",
                behavior: "agentic",
                searchDepth: "",
                freshness: "",
                searchGuidance: "",
                includeDomains: "",
                failurePolicy: "",
            }).behavior,
        ).toBe("standard");
    });

    test("normalizes a human domain list and removes duplicates", () => {
        expect(
            parseDomainList("doc.rust-lang.org, GitHub.com\ndoc.rust-lang.org"),
        ).toEqual(["doc.rust-lang.org", "github.com"]);
    });

    test("keeps a legacy projection for older API clients", () => {
        expect(
            legacyStrategyForPolicy({
                sourceMode: "web",
                behavior: "fact_check",
            }),
        ).toBe("create_and_ground");
        expect(
            legacyStrategyForPolicy({
                sourceMode: "web_and_documents",
                behavior: "agentic",
            }),
        ).toBe("agentic");
    });

    test("makes a topic search profile explicitly inherited or customized", () => {
        const inherited = {
            sourceMode: "web" as const,
            behavior: "standard" as const,
            searchDepth: "deep" as const,
            freshness: "month" as const,
            searchGuidance: "Prefer standards",
            includeDomains: ["example.com"],
            failurePolicy: "stop" as const,
        };
        const blank = clearGroundingSearchOverride({
            sourceMode: "web",
            behavior: "standard",
            searchDepth: "quick",
            freshness: "week",
            searchGuidance: "Old",
            includeDomains: "old.example",
            failurePolicy: "mark_unverified",
        });
        expect(hasGroundingSearchOverride(blank)).toBe(false);

        const customized = customizeGroundingSearch(blank, inherited);
        expect(hasGroundingSearchOverride(customized)).toBe(true);
        expect(customized).toMatchObject({
            searchDepth: "deep",
            freshness: "month",
            searchGuidance: "Prefer standards",
            includeDomains: "example.com",
            failurePolicy: "stop",
        });
    });
});
