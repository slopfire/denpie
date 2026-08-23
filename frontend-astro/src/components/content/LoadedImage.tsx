import { useEffect, useState, type ReactNode } from "react";
import { listImageObjectUrl, revokeBlobUrl } from "@/lib/image-thumbnail";

/**
 * Hide a broken image instead of showing the browser's alt-text glyph.
 * `render` wraps a successful image (button, link). `fallback` replaces a
 * failed load; omit it to collapse the slot.
 *
 * `maxDecodeEdge` downsamples through `createImageBitmap` so list thumbnails
 * do not keep 2048px decoded bitmaps. Lightbox and detail should omit it.
 */
export function LoadedImage({
    src,
    alt,
    className,
    fallback = null,
    render,
    maxDecodeEdge,
}: {
    src: string;
    alt: string;
    className?: string;
    fallback?: ReactNode;
    render?: (image: ReactNode) => ReactNode;
    maxDecodeEdge?: number;
}) {
    const [failed, setFailed] = useState(false);
    const [displaySrc, setDisplaySrc] = useState(
        maxDecodeEdge === undefined ? src : "",
    );

    useEffect(() => {
        setFailed(false);
        if (src === "" || maxDecodeEdge === undefined) {
            setDisplaySrc(src);
            return;
        }
        let cancelled = false;
        let created: string | undefined;
        setDisplaySrc("");
        void listImageObjectUrl(src, maxDecodeEdge).then(
            (url) => {
                if (cancelled) {
                    revokeBlobUrl(url);
                    return;
                }
                created = url;
                setDisplaySrc(url);
            },
            () => {
                // Downsample is best-effort: show the original file if
                // createImageBitmap/canvas is missing or rejects (SVG, etc).
                if (!cancelled) setDisplaySrc(src);
            },
        );
        return () => {
            cancelled = true;
            if (created !== undefined) revokeBlobUrl(created);
        };
    }, [src, maxDecodeEdge]);

    if (src === "" || failed || displaySrc === "") return fallback;
    const image = (
        <img
            src={displaySrc}
            alt={alt}
            loading="lazy"
            decoding="async"
            className={className}
            onError={() => setFailed(true)}
        />
    );
    return render === undefined ? image : render(image);
}
