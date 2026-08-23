// Browser image processing behind a testable boundary: files at or below
// 200 KiB stay untouched data URLs; larger non-GIF images are downscaled to
// a 2048 px longest edge and encoded WebP quality 0.82 with a JPEG fallback;
// any canvas failure falls back to the original data URL. Object URLs are
// never produced or sent.

import {
    MAX_MANUAL_IMAGES,
    MAX_IMAGE_BYTES,
    MAX_EDGE_PX,
    OUTPUT_QUALITY,
    SKIP_IF_SMALLER_BYTES,
    fitWithin,
    isAllowedImageType,
} from "./flow-add-form";

/**
 * Browser seams. The real implementation uses `FileReader`, an
 * `HTMLImageElement`, and a canvas; tests inject deterministic doubles.
 */
export interface ImageDeps {
    /** Read the file's bytes as a base64 data URL of its own type. */
    readAsDataUrl(file: File): Promise<string>;
    /** Decode a data URL, resolving natural width/height. */
    loadImage(src: string): Promise<{ width: number; height: number }>;
    /** Draw at the given size and export the requested MIME/quality. */
    drawAndEncode(
        src: string,
        width: number,
        height: number,
        mimeType: "image/webp" | "image/jpeg",
        quality: number,
    ): Promise<string>;
}

/** Validate the browser file metadata accepted by the backend image path. */
export function validateImageFile(file: File): void {
    if (!isAllowedImageType(file.type)) {
        throw new TypeError(
            "Unsupported image type. Use PNG, JPEG, WebP, or GIF files only.",
        );
    }
    if (
        !Number.isFinite(file.size) ||
        file.size < 0 ||
        file.size > MAX_IMAGE_BYTES
    ) {
        throw new TypeError("Each image must be 10 MiB or smaller.");
    }
}

/** Validate a batch before any FileReader or canvas work starts. */
export function validateImageFiles(
    files: readonly File[],
    currentCount = 0,
): void {
    if (files.length === 0) throw new TypeError("No files selected.");
    for (const file of files) validateImageFile(file);
    if (currentCount < 0 || currentCount + files.length > MAX_MANUAL_IMAGES) {
        throw new TypeError(`At most ${MAX_MANUAL_IMAGES} images per card.`);
    }
}

/** GIFs skip canvas work entirely (animation would be lost). */
export async function compressFileToDataUrl(
    file: File,
    deps: ImageDeps,
): Promise<string> {
    validateImageFile(file);
    if (file.size <= SKIP_IF_SMALLER_BYTES || file.type === "image/gif") {
        return deps.readAsDataUrl(file);
    }
    const original = await deps.readAsDataUrl(file);
    try {
        const image = await deps.loadImage(original);
        const { width, height } = fitWithin(
            image.width,
            image.height,
            MAX_EDGE_PX,
        );
        try {
            const webp = await deps.drawAndEncode(
                original,
                width,
                height,
                "image/webp",
                OUTPUT_QUALITY,
            );
            if (!webp.startsWith("data:image/webp")) {
                throw new Error("WebP encoding unsupported");
            }
            return webp;
        } catch {
            const jpeg = await deps.drawAndEncode(
                original,
                width,
                height,
                "image/jpeg",
                OUTPUT_QUALITY,
            );
            if (!jpeg.startsWith("data:image/jpeg")) {
                throw new Error("JPEG encoding unsupported");
            }
            return jpeg;
        }
    } catch {
        // Original-data fallback when decoding/canvas fails entirely. The source
        // was read once, so fallback cannot accidentally produce a second read or
        // a different data URL.
        return original;
    }
}

export async function compressFilesToDataUrls(
    files: readonly File[],
    deps: ImageDeps,
): Promise<string[]> {
    validateImageFiles(files);
    const urls: string[] = [];
    for (const file of files) {
        urls.push(await compressFileToDataUrl(file, deps));
    }
    return urls;
}

function decodeToElement(src: string): Promise<HTMLImageElement> {
    const { promise, resolve, reject } =
        Promise.withResolvers<HTMLImageElement>();
    const element = new Image();
    element.addEventListener("load", () => resolve(element));
    element.addEventListener("error", () => reject(new Error("decode failed")));
    element.src = src;
    return promise;
}

/** Real browser implementation of {@link ImageDeps}. */
export function browserImageDeps(): ImageDeps {
    return {
        readAsDataUrl: (file) => {
            const { promise, resolve, reject } =
                Promise.withResolvers<string>();
            const reader = new FileReader();
            reader.addEventListener("load", () => {
                if (typeof reader.result === "string") resolve(reader.result);
                else reject(new TypeError("FileReader returned no data URL"));
            });
            reader.addEventListener("error", () => reject(reader.error));
            reader.readAsDataURL(file);
            return promise;
        },
        loadImage: async (src) => {
            const image = await decodeToElement(src);
            return { width: image.naturalWidth, height: image.naturalHeight };
        },
        drawAndEncode: async (src, width, height, mimeType, quality) => {
            const canvas = document.createElement("canvas");
            canvas.width = width;
            canvas.height = height;
            const context = canvas.getContext("2d");
            if (context === null)
                throw new Error("canvas 2d context unavailable");
            context.drawImage(await decodeToElement(src), 0, 0, width, height);
            const encoded = canvas.toDataURL(mimeType, quality);
            if (!encoded.startsWith(`data:${mimeType}`)) {
                throw new Error(`${mimeType} encoding unsupported`);
            }
            return encoded;
        },
    };
}
