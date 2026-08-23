import { useState, type ReactNode } from "react";

/**
 * Hide a broken image instead of showing the browser's alt-text glyph.
 * `render` wraps a successful image (button, link). `fallback` replaces a
 * failed load; omit it to collapse the slot.
 */
export function LoadedImage({
    src,
    alt,
    className,
    fallback = null,
    render,
}: {
    src: string;
    alt: string;
    className?: string;
    fallback?: ReactNode;
    render?: (image: ReactNode) => ReactNode;
}) {
    const [failed, setFailed] = useState(false);
    if (src === "" || failed) return fallback;
    const image = (
        <img
            src={src}
            alt={alt}
            loading="lazy"
            className={className}
            onError={() => setFailed(true)}
        />
    );
    return render === undefined ? image : render(image);
}
