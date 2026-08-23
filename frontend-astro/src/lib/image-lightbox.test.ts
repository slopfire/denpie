import { describe, expect, test } from "bun:test";
import {
    LIGHTBOX_MAX_ZOOM,
    LIGHTBOX_MIN_ZOOM,
    clampLightboxIndex,
    clampLightboxZoom,
    containedImageSize,
    cursorOffsetInStage,
    doubleClickZoom,
    nextLightboxIndex,
    nextWheelViewport,
    panFromPointer,
    previousLightboxIndex,
    scaleWheelDelta,
    shouldCloseLightboxOnStageClick,
} from "./image-lightbox";

describe("image lightbox navigation", () => {
    test("clamps an initial index and wraps through the image set", () => {
        expect(clampLightboxIndex(9, 3)).toBe(2);
        expect(clampLightboxIndex(-1, 3)).toBe(0);
        expect(nextLightboxIndex(2, 3)).toBe(0);
        expect(previousLightboxIndex(0, 3)).toBe(2);
        expect(nextLightboxIndex(0, 0)).toBe(0);
    });
});

describe("image lightbox viewport", () => {
    test("bounds zoom, toggles double-click zoom, and derives pointer pan", () => {
        expect(clampLightboxZoom(0)).toBe(LIGHTBOX_MIN_ZOOM);
        expect(clampLightboxZoom(8)).toBe(LIGHTBOX_MAX_ZOOM);
        expect(doubleClickZoom(1)).toBe(2);
        expect(doubleClickZoom(2)).toBe(1);
        expect(panFromPointer(4, 8, 10, 20, 31, 15)).toEqual([25, 3]);
        expect(
            cursorOffsetInStage(
                { left: 10, top: 20, width: 200, height: 100 },
                60,
                90,
            ),
        ).toEqual([-50, 20]);
    });

    test("wheel zoom stays in range, pans toward the cursor, and resets at 1x", () => {
        expect(scaleWheelDelta(2, 0)).toBe(2);
        expect(scaleWheelDelta(2, 1)).toBe(36);
        expect(scaleWheelDelta(2, 2)).toBe(240);

        const zoomed = nextWheelViewport(1, [0, 0], -100, 0, [40, -20]);
        expect(zoomed.zoom).toBeCloseTo(Math.exp(0.14), 8);
        expect(zoomed.zoom).toBeGreaterThan(LIGHTBOX_MIN_ZOOM);
        expect(zoomed.zoom).toBeLessThanOrEqual(LIGHTBOX_MAX_ZOOM);
        expect(zoomed.pan[0]).toBeLessThan(0);
        expect(zoomed.pan[1]).toBeGreaterThan(0);

        const clamped = nextWheelViewport(
            LIGHTBOX_MAX_ZOOM,
            [8, 8],
            -400,
            0,
            [0, 0],
        );
        expect(clamped.zoom).toBe(LIGHTBOX_MAX_ZOOM);

        const reset = nextWheelViewport(1.5, [10, 10], 8000, 0, [40, -20]);
        expect(reset.zoom).toBe(LIGHTBOX_MIN_ZOOM);
        expect(reset.pan).toEqual([0, 0]);
    });

    test("fits the image to the stage by scaling up or down", () => {
        expect(containedImageSize(8, 8, 400, 200)).toEqual({
            width: 200,
            height: 200,
        });
        expect(containedImageSize(800, 400, 400, 200)).toEqual({
            width: 400,
            height: 200,
        });
        expect(containedImageSize(0, 10, 400, 200)).toEqual({
            width: 0,
            height: 0,
        });
    });

    test("empty stage clicks close unless the pointer dragged or hit chrome", () => {
        expect(shouldCloseLightboxOnStageClick(false, false)).toBe(true);
        expect(shouldCloseLightboxOnStageClick(false, true)).toBe(false);
        expect(shouldCloseLightboxOnStageClick(true, false)).toBe(false);
        expect(shouldCloseLightboxOnStageClick(true, true)).toBe(false);
    });
});
