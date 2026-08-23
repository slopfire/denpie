import { describe, expect, test } from "bun:test";
import {
    isLongCodeFence,
    prismLanguage,
    safeMarkdownUrl,
} from "./markdown-content";

describe("safeMarkdownUrl", () => {
    test("keeps the supported card-link schemes and local paths", () => {
        expect(safeMarkdownUrl("https://denpie.dev/docs")).toBe(
            "https://denpie.dev/docs",
        );
        expect(safeMarkdownUrl("mailto:hello@example.com")).toBe(
            "mailto:hello@example.com",
        );
        expect(safeMarkdownUrl("/guide")).toBe("/guide");
        expect(safeMarkdownUrl("#code")).toBe("#code");
    });

    test("neutralizes unsupported schemes", () => {
        expect(safeMarkdownUrl("javascript:alert(1)")).toBe("#");
        expect(safeMarkdownUrl("data:text/html,nope")).toBe("#");
    });
});

describe("prismLanguage", () => {
    test("normalizes familiar fenced-code aliases", () => {
        expect(prismLanguage("rs")).toBe("rust");
        expect(prismLanguage("language-tsx")).toBe("typescript");
        expect(prismLanguage("{yaml title=fixture}")).toBe("yaml");
        expect(prismLanguage(undefined)).toBe("plaintext");
    });
});

describe("isLongCodeFence", () => {
    test("uses the five-line compact-card threshold", () => {
        expect(isLongCodeFence("1\n2\n3\n4\n5")).toBe(false);
        expect(isLongCodeFence("1\n2\n3\n4\n5\n6")).toBe(true);
    });
});
