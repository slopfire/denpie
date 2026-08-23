import { describe, expect, test } from "bun:test";
import { thumbnailSize } from "./image-thumbnail";

describe("thumbnailSize", () => {
    test("leaves already-small images alone", () => {
        expect(thumbnailSize(320, 180, 640)).toEqual({
            width: 320,
            height: 180,
        });
    });

    test("scales a 2048px card image down to the list edge", () => {
        expect(thumbnailSize(2048, 1024, 640)).toEqual({
            width: 640,
            height: 320,
        });
        expect(thumbnailSize(1024, 2048, 640)).toEqual({
            width: 320,
            height: 640,
        });
    });

    test("rejects non-positive sizes", () => {
        expect(thumbnailSize(0, 10, 640)).toEqual({ width: 0, height: 0 });
        expect(thumbnailSize(10, 10, 0)).toEqual({ width: 0, height: 0 });
    });
});
