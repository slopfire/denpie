import { expect, test } from "bun:test";
import { topicArchiveHref } from "./GroundingPage";

test("topic archive links preserve the requested status and encode the topic", () => {
    expect(topicArchiveHref("pending", "Rust & Python")).toBe(
        "/archive?status=pending&topic=Rust%20%26%20Python",
    );
    expect(topicArchiveHref("scheduled", "System Design")).toBe(
        "/archive?status=scheduled&topic=System%20Design",
    );
});
