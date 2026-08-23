import { describe, expect, test } from "bun:test";
import {
    otherRememberedAccounts,
    parseRememberedAccounts,
    recordRememberedAccount,
} from "./remembered-accounts";

describe("remembered accounts", () => {
    test("accepts a JSON string array and drops blanks/duplicates", () => {
        expect(
            parseRememberedAccounts(
                JSON.stringify(["alice", "bob", "alice", "  ", "carol"]),
            ),
        ).toEqual(["alice", "bob", "carol"]);
    });

    test("rejects unknown storage shapes", () => {
        expect(parseRememberedAccounts(null)).toEqual([]);
        expect(parseRememberedAccounts("not-json")).toEqual([]);
        expect(parseRememberedAccounts(JSON.stringify({ name: "alice" }))).toEqual(
            [],
        );
    });

    test("records the latest username first and caps at five", () => {
        const next = recordRememberedAccount(
            ["a", "b", "c", "d", "e"],
            "fresh",
        );
        expect(next).toEqual(["fresh", "a", "b", "c", "d"]);
        expect(recordRememberedAccount(next, "b")).toEqual([
            "b",
            "fresh",
            "a",
            "c",
            "d",
        ]);
    });

    test("hides the signed-in username from the switcher", () => {
        expect(otherRememberedAccounts(["alice", "bob"], "alice")).toEqual([
            "bob",
        ]);
    });
});
