/** Longest edge for list/grid card images. Full-size files are up to 2048px. */
export const LIST_IMAGE_MAX_EDGE_PX = 640;

/** Scale `width`×`height` so the longer edge is at most `maxEdge`. */
export function thumbnailSize(
    width: number,
    height: number,
    maxEdge: number,
): { width: number; height: number } {
    if (width <= 0 || height <= 0 || maxEdge <= 0) {
        return { width: 0, height: 0 };
    }
    const edge = Math.max(width, height);
    if (edge <= maxEdge) return { width, height };
    const scale = maxEdge / edge;
    return {
        width: Math.max(1, Math.round(width * scale)),
        height: Math.max(1, Math.round(height * scale)),
    };
}

export function revokeBlobUrl(url: string): void {
    if (url.startsWith("blob:")) URL.revokeObjectURL(url);
}

function blobFromBitmap(bitmap: ImageBitmap): Promise<Blob> {
    if (typeof OffscreenCanvas === "function") {
        const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
        const context = canvas.getContext("2d");
        if (context === null) {
            bitmap.close();
            return Promise.reject(new TypeError("OffscreenCanvas 2d"));
        }
        context.drawImage(bitmap, 0, 0);
        bitmap.close();
        return canvas.convertToBlob({ type: "image/jpeg", quality: 0.82 });
    }
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext("2d");
    if (context === null) {
        bitmap.close();
        return Promise.reject(new TypeError("canvas 2d"));
    }
    context.drawImage(bitmap, 0, 0);
    bitmap.close();
    return new Promise((resolve, reject) => {
        canvas.toBlob(
            (blob) => {
                if (blob === null) {
                    reject(new TypeError("canvas toBlob"));
                    return;
                }
                resolve(blob);
            },
            "image/jpeg",
            0.82,
        );
    });
}

/**
 * Decode `src` no wider than `maxEdge` and return a blob URL.
 *
 * Falls back to `src` when `createImageBitmap` is missing. Callers must
 * {@link revokeBlobUrl} the result on unmount when it is a blob URL.
 */
export async function listImageObjectUrl(
    src: string,
    maxEdge: number,
): Promise<string> {
    if (typeof createImageBitmap !== "function") return src;
    const response = await fetch(src, { credentials: "same-origin" });
    if (!response.ok) {
        throw new TypeError(`image fetch ${response.status}`);
    }
    const blob = await response.blob();
    const bitmap = await createImageBitmap(blob, {
        resizeWidth: maxEdge,
        resizeQuality: "medium",
    });
    const sized = thumbnailSize(bitmap.width, bitmap.height, maxEdge);
    if (sized.width !== bitmap.width || sized.height !== bitmap.height) {
        bitmap.close();
        const resized = await createImageBitmap(blob, {
            resizeWidth: sized.width,
            resizeHeight: sized.height,
            resizeQuality: "medium",
        });
        return URL.createObjectURL(await blobFromBitmap(resized));
    }
    return URL.createObjectURL(await blobFromBitmap(bitmap));
}
