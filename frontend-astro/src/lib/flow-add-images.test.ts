import { describe, expect, test } from "bun:test";
import {
    MAX_IMAGE_BYTES,
    SKIP_IF_SMALLER_BYTES,
    MAX_EDGE_PX,
    OUTPUT_QUALITY,
} from "./flow-add-form";
import {
    compressFileToDataUrl,
    compressFilesToDataUrls,
    validateImageFile,
    validateImageFiles,
} from "./flow-add-images";
import type { ImageDeps } from "./flow-add-images";

function file(type: string, size: number): File {
    return new File([new Uint8Array(size)], "selected-image", { type });
}

function deps(overrides: Partial<ImageDeps> = {}): ImageDeps & {
    reads: number;
    loads: number;
    encodes: Array<{
        width: number;
        height: number;
        mimeType: "image/webp" | "image/jpeg";
        quality: number;
    }>;
} {
    let reads = 0;
    let loads = 0;
    const encodes: Array<{
        width: number;
        height: number;
        mimeType: "image/webp" | "image/jpeg";
        quality: number;
    }> = [];
    return {
        get reads() {
            return reads;
        },
        get loads() {
            return loads;
        },
        encodes,
        readAsDataUrl: async () => {
            reads += 1;
            return "data:image/png;base64,ORIGINAL";
        },
        loadImage: async () => {
            loads += 1;
            return { width: 4096, height: 2048 };
        },
        drawAndEncode: async (_src, width, height, mimeType, quality) => {
            encodes.push({ width, height, mimeType, quality });
            return `data:${mimeType};base64,COMPRESSED`;
        },
        ...overrides,
    };
}

describe("flow add image processing", () => {
    test("validates MIME and byte limits before browser work", () => {
        expect(() => validateImageFile(file("image/svg+xml", 10))).toThrow(
            /Unsupported image type/,
        );
        expect(() =>
            validateImageFile(file("image/png", MAX_IMAGE_BYTES + 1)),
        ).toThrow(/10 MiB/);
        expect(() =>
            validateImageFile(file("image/gif", MAX_IMAGE_BYTES)),
        ).not.toThrow();
        expect(() => validateImageFiles([], 0)).toThrow(/No files/);
        expect(() =>
            validateImageFiles(
                [file("image/png", 10), file("image/jpeg", 10)],
                3,
            ),
        ).toThrow(/At most 4/);
    });

    test("leaves small images as original data URLs", async () => {
        const imageDeps = deps();
        const result = await compressFileToDataUrl(
            file("image/png", SKIP_IF_SMALLER_BYTES),
            imageDeps,
        );
        expect(result).toBe("data:image/png;base64,ORIGINAL");
        expect(imageDeps.reads).toBe(1);
        expect(imageDeps.loads).toBe(0);
        expect(imageDeps.encodes).toEqual([]);
    });

    test("skips canvas for GIFs, including large GIFs", async () => {
        const imageDeps = deps();
        const result = await compressFileToDataUrl(
            file("image/gif", SKIP_IF_SMALLER_BYTES + 1),
            imageDeps,
        );
        expect(result).toBe("data:image/png;base64,ORIGINAL");
        expect(imageDeps.loads).toBe(0);
        expect(imageDeps.encodes).toEqual([]);
    });

    test("downscales large images to 2048 and prefers WebP quality .82", async () => {
        const imageDeps = deps();
        const result = await compressFileToDataUrl(
            file("image/png", SKIP_IF_SMALLER_BYTES + 1),
            imageDeps,
        );
        expect(result).toMatch(/^data:image\/webp/);
        expect(imageDeps.encodes).toEqual([
            {
                width: MAX_EDGE_PX,
                height: 1024,
                mimeType: "image/webp",
                quality: OUTPUT_QUALITY,
            },
        ]);
    });

    test("falls back from WebP to JPEG, then to the original once", async () => {
        let attempt = 0;
        const jpegEncodes: Array<"image/webp" | "image/jpeg"> = [];
        const jpegDeps = deps({
            drawAndEncode: async (_src, width, height, mimeType, quality) => {
                attempt += 1;
                jpegEncodes.push(mimeType);
                if (attempt === 1) throw new Error("WebP unavailable");
                return `data:${mimeType};${width}x${height};q=${quality}`;
            },
        });
        const jpeg = await compressFileToDataUrl(
            file("image/jpeg", SKIP_IF_SMALLER_BYTES + 1),
            jpegDeps,
        );
        expect(jpeg).toMatch(/^data:image\/jpeg/);
        expect(jpegEncodes).toEqual(["image/webp", "image/jpeg"]);

        const fallbackEncodes: Array<"image/webp" | "image/jpeg"> = [];
        const fallbackDeps = deps({
            drawAndEncode: async (_src, _width, _height, mimeType) => {
                fallbackEncodes.push(mimeType);
                throw new Error("canvas unavailable");
            },
        });
        const original = await compressFileToDataUrl(
            file("image/webp", SKIP_IF_SMALLER_BYTES + 1),
            fallbackDeps,
        );
        expect(original).toBe("data:image/png;base64,ORIGINAL");
        expect(fallbackDeps.reads).toBe(1);
        expect(fallbackEncodes).toEqual(["image/webp", "image/jpeg"]);
    });

    test("compresses a validated batch in order and rejects batches over four", async () => {
        const imageDeps = deps();
        const files = [
            file("image/png", 1),
            file("image/jpeg", 2),
            file("image/webp", 3),
            file("image/gif", 4),
        ];
        const urls = await compressFilesToDataUrls(files, imageDeps);
        expect(urls).toHaveLength(4);
        await expect(
            compressFilesToDataUrls(
                [...files, file("image/png", 5)],
                imageDeps,
            ),
        ).rejects.toThrow(/At most 4/);
    });
});
