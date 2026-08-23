export const LIGHTBOX_MIN_ZOOM = 1;
export const LIGHTBOX_MAX_ZOOM = 4;
export const LIGHTBOX_ZOOM_STEP = 0.25;
export const LIGHTBOX_ZOOM_WHEEL_EXP_FACTOR = 0.0014;
export const LIGHTBOX_STAGE_INTERACTIVE = "img, button, a, [data-lightbox-ui]";

export function clampLightboxIndex(index: number, count: number): number {
    if (count <= 0) return 0;
    return Math.min(Math.max(0, Math.trunc(index)), count - 1);
}

export function nextLightboxIndex(index: number, count: number): number {
    if (count <= 0) return 0;
    return (clampLightboxIndex(index, count) + 1) % count;
}

export function previousLightboxIndex(index: number, count: number): number {
    if (count <= 0) return 0;
    return (clampLightboxIndex(index, count) + count - 1) % count;
}

export function clampLightboxZoom(value: number): number {
    return Math.min(LIGHTBOX_MAX_ZOOM, Math.max(LIGHTBOX_MIN_ZOOM, value));
}

export function doubleClickZoom(value: number): number {
    return value > LIGHTBOX_MIN_ZOOM ? LIGHTBOX_MIN_ZOOM : 2;
}

export function panFromPointer(
    originX: number,
    originY: number,
    startX: number,
    startY: number,
    currentX: number,
    currentY: number,
): readonly [number, number] {
    return [originX + currentX - startX, originY + currentY - startY];
}

/** Pixel-equivalent wheel delta. mode 0 = pixels, 1 = lines, 2 = pages. */
export function scaleWheelDelta(delta: number, mode: number): number {
    if (mode === 1) return delta * 18;
    if (mode === 2) return delta * 120;
    return delta;
}

export function cursorOffsetInStage(
    rect: { left: number; top: number; width: number; height: number },
    clientX: number,
    clientY: number,
): readonly [number, number] {
    return [
        clientX - (rect.left + rect.width / 2),
        clientY - (rect.top + rect.height / 2),
    ];
}

export function panForZoomAtFocal(
    oldZoom: number,
    newZoom: number,
    panX: number,
    panY: number,
    focalX: number,
    focalY: number,
): readonly [number, number] {
    const old = Math.max(oldZoom, LIGHTBOX_MIN_ZOOM);
    const ratio = newZoom / old;
    return [focalX - (focalX - panX) * ratio, focalY - (focalY - panY) * ratio];
}

export function nextWheelViewport(
    zoom: number,
    pan: readonly [number, number],
    deltaY: number,
    deltaMode: number,
    focal: readonly [number, number],
): { zoom: number; pan: readonly [number, number] } {
    const nextZoom = clampLightboxZoom(
        zoom *
            Math.exp(
                -scaleWheelDelta(deltaY, deltaMode) *
                    LIGHTBOX_ZOOM_WHEEL_EXP_FACTOR,
            ),
    );
    if (nextZoom <= LIGHTBOX_MIN_ZOOM) {
        return { zoom: LIGHTBOX_MIN_ZOOM, pan: [0, 0] };
    }
    return {
        zoom: nextZoom,
        pan: panForZoomAtFocal(
            zoom,
            nextZoom,
            pan[0],
            pan[1],
            focal[0],
            focal[1],
        ),
    };
}

/** Close when the click missed the image and controls, and was not a pan. */
export function shouldCloseLightboxOnStageClick(
    dragged: boolean,
    hitInteractive: boolean,
): boolean {
    return !dragged && !hitInteractive;
}

/** Largest size that fits the stage, scaling the image up or down. */
export function containedImageSize(
    naturalWidth: number,
    naturalHeight: number,
    stageWidth: number,
    stageHeight: number,
): { width: number; height: number } {
    if (
        naturalWidth <= 0 ||
        naturalHeight <= 0 ||
        stageWidth <= 0 ||
        stageHeight <= 0
    ) {
        return { width: 0, height: 0 };
    }
    const scale = Math.min(
        stageWidth / naturalWidth,
        stageHeight / naturalHeight,
    );
    return {
        width: naturalWidth * scale,
        height: naturalHeight * scale,
    };
}
