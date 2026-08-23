import { useEffect, useRef, useState } from "react";
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    MinusIcon,
    PlusIcon,
    RotateCcwIcon,
} from "lucide-react";
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { t, tf } from "@/lib/i18n";
import {
    LIGHTBOX_MAX_ZOOM,
    LIGHTBOX_MIN_ZOOM,
    LIGHTBOX_STAGE_INTERACTIVE,
    LIGHTBOX_ZOOM_STEP,
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
} from "@/lib/image-lightbox";

export interface LightboxImage {
    src: string;
    alt: string;
}

export interface ImageLightboxProps {
    open: boolean;
    images: readonly LightboxImage[];
    initialIndex?: number;
    onOpenChange: (open: boolean) => void;
}

interface PointerOrigin {
    pointerId: number;
    x: number;
    y: number;
    panX: number;
    panY: number;
}

const NO_POINTER: PointerOrigin | null = null;

/** Controlled, keyboard-operable image dialog used by Flow and detail cards. */
export function ImageLightbox({
    open,
    images,
    initialIndex = 0,
    onOpenChange,
}: ImageLightboxProps) {
    const [index, setIndex] = useState(() =>
        clampLightboxIndex(initialIndex, images.length),
    );
    const [zoom, setZoom] = useState(LIGHTBOX_MIN_ZOOM);
    const [pan, setPan] = useState<readonly [number, number]>([0, 0]);
    const pointer = useRef<PointerOrigin | null>(NO_POINTER);
    const stageRef = useRef<HTMLDivElement>(null);
    const zoomRef = useRef(zoom);
    const panRef = useRef(pan);
    const dragged = useRef(false);
    const image = images[index];
    const hasMultiple = images.length > 1;
    zoomRef.current = zoom;
    panRef.current = pan;

    const applyViewport = (
        nextZoom: number,
        nextPan: readonly [number, number],
    ) => {
        zoomRef.current = nextZoom;
        panRef.current = nextPan;
        setZoom(nextZoom);
        setPan(nextPan);
    };

    const resetViewport = () => {
        applyViewport(LIGHTBOX_MIN_ZOOM, [0, 0]);
        pointer.current = NO_POINTER;
        dragged.current = false;
    };

    useEffect(() => {
        if (!open) return;
        setIndex(clampLightboxIndex(initialIndex, images.length));
        resetViewport();
    }, [initialIndex, images.length, open]);

    // Native listener: React's onWheel is passive, so preventDefault would not stick.
    useEffect(() => {
        if (!open) return;
        let pendingY = 0;
        let pendingMode = 0;
        let pendingFocal: readonly [number, number] = [0, 0];
        let frame = 0;

        const flush = () => {
            frame = 0;
            if (pendingY === 0) return;
            const next = nextWheelViewport(
                zoomRef.current,
                panRef.current,
                pendingY,
                pendingMode,
                pendingFocal,
            );
            pendingY = 0;
            applyViewport(next.zoom, next.pan);
        };

        const onWheel = (event: WheelEvent) => {
            const stage = stageRef.current;
            if (stage === null || !stage.contains(event.target as Node)) return;
            event.preventDefault();
            pendingY += scaleWheelDelta(event.deltaY, event.deltaMode);
            pendingMode = 0;
            pendingFocal = cursorOffsetInStage(
                stage.getBoundingClientRect(),
                event.clientX,
                event.clientY,
            );
            if (frame === 0) frame = window.requestAnimationFrame(flush);
        };

        document.addEventListener("wheel", onWheel, {
            passive: false,
            capture: true,
        });
        return () => {
            document.removeEventListener("wheel", onWheel, { capture: true });
            if (frame !== 0) window.cancelAnimationFrame(frame);
        };
    }, [open]);

    useEffect(() => {
        if (!open) return;
        const stage = stageRef.current;
        const img = stage?.querySelector("img");
        if (
            stage === null ||
            !(img instanceof HTMLImageElement) ||
            !img.complete
        )
            return;
        const { width, height } = containedImageSize(
            img.naturalWidth,
            img.naturalHeight,
            stage.clientWidth,
            stage.clientHeight,
        );
        img.style.width = `${width}px`;
        img.style.height = `${height}px`;
    }, [open, index]);

    if (image === undefined) return null;

    const select = (next: number) => {
        setIndex(clampLightboxIndex(next, images.length));
        resetViewport();
    };
    const previous = () => select(previousLightboxIndex(index, images.length));
    const next = () => select(nextLightboxIndex(index, images.length));
    const adjustZoom = (amount: number) => {
        const nextZoom = clampLightboxZoom(zoomRef.current + amount);
        applyViewport(
            nextZoom,
            nextZoom === LIGHTBOX_MIN_ZOOM ? [0, 0] : panRef.current,
        );
    };
    const stageHitInteractive = (target: EventTarget | null) =>
        target instanceof Element &&
        target.closest(LIGHTBOX_STAGE_INTERACTIVE) !== null;
    const fitImageToStage = (img: HTMLImageElement) => {
        const stage = stageRef.current;
        if (stage === null) return;
        const { width, height } = containedImageSize(
            img.naturalWidth,
            img.naturalHeight,
            stage.clientWidth,
            stage.clientHeight,
        );
        img.style.width = `${width}px`;
        img.style.height = `${height}px`;
    };

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                showCloseButton={false}
                data-testid="image-lightbox"
                overlayClassName="bg-background"
                className="top-0 left-0 block h-[100dvh] w-full max-w-none translate-x-0 translate-y-0 overflow-hidden rounded-none border-0 bg-background p-0 text-foreground ring-0 sm:max-w-none"
                aria-describedby="image-lightbox-description"
                onKeyDown={(event) => {
                    if (event.key === "ArrowLeft" && hasMultiple) {
                        event.preventDefault();
                        previous();
                    }
                    if (event.key === "ArrowRight" && hasMultiple) {
                        event.preventDefault();
                        next();
                    }
                }}
            >
                <DialogHeader
                    data-testid="image-lightbox-header"
                    className="pointer-events-none absolute inset-x-0 top-0 z-20 flex-row items-center justify-between gap-3 bg-gradient-to-b from-background/70 to-transparent px-4 py-3"
                >
                    <div className="min-w-0">
                        <DialogTitle className="truncate">
                            {tf("images.lightbox.position", {
                                current: index + 1,
                                total: images.length,
                            })}
                        </DialogTitle>
                        <DialogDescription
                            id="image-lightbox-description"
                            className="truncate"
                        >
                            {image.alt}
                        </DialogDescription>
                    </div>
                    <div
                        data-lightbox-ui
                        className="pointer-events-auto flex items-center gap-1"
                    >
                        <Button
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            onClick={() => adjustZoom(-LIGHTBOX_ZOOM_STEP)}
                            disabled={zoom <= LIGHTBOX_MIN_ZOOM}
                            aria-label={t("images.lightbox.zoom_out")}
                        >
                            <MinusIcon />
                        </Button>
                        <span
                            data-testid="image-lightbox-zoom"
                            className="w-12 text-center text-xs font-medium text-muted-foreground"
                        >
                            {Math.round(zoom * 100)}%
                        </span>
                        <Button
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            onClick={() => adjustZoom(LIGHTBOX_ZOOM_STEP)}
                            disabled={zoom >= LIGHTBOX_MAX_ZOOM}
                            aria-label={t("images.lightbox.zoom_in")}
                        >
                            <PlusIcon />
                        </Button>
                        <Button
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            onClick={resetViewport}
                            disabled={
                                zoom === LIGHTBOX_MIN_ZOOM &&
                                pan[0] === 0 &&
                                pan[1] === 0
                            }
                            aria-label={t("images.lightbox.reset_view")}
                        >
                            <RotateCcwIcon />
                        </Button>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => onOpenChange(false)}
                        >
                            {t("common.close")}
                        </Button>
                    </div>
                </DialogHeader>

                <div
                    ref={stageRef}
                    data-testid="image-lightbox-stage"
                    className="absolute inset-0 z-0 flex touch-none items-center justify-center overflow-hidden"
                    onPointerDown={(event) => {
                        dragged.current = false;
                        if (zoomRef.current <= LIGHTBOX_MIN_ZOOM) return;
                        event.currentTarget.setPointerCapture(event.pointerId);
                        pointer.current = {
                            pointerId: event.pointerId,
                            x: event.clientX,
                            y: event.clientY,
                            panX: panRef.current[0],
                            panY: panRef.current[1],
                        };
                    }}
                    onPointerMove={(event) => {
                        const origin = pointer.current;
                        if (
                            origin === null ||
                            origin.pointerId !== event.pointerId
                        )
                            return;
                        if (
                            event.clientX !== origin.x ||
                            event.clientY !== origin.y
                        ) {
                            dragged.current = true;
                        }
                        const nextPan = panFromPointer(
                            origin.panX,
                            origin.panY,
                            origin.x,
                            origin.y,
                            event.clientX,
                            event.clientY,
                        );
                        panRef.current = nextPan;
                        setPan(nextPan);
                    }}
                    onPointerUp={(event) => {
                        if (pointer.current?.pointerId !== event.pointerId)
                            return;
                        pointer.current = NO_POINTER;
                        event.currentTarget.releasePointerCapture(
                            event.pointerId,
                        );
                    }}
                    onPointerCancel={() => {
                        pointer.current = NO_POINTER;
                    }}
                    onClick={(event) => {
                        if (
                            !shouldCloseLightboxOnStageClick(
                                dragged.current,
                                stageHitInteractive(event.target),
                            )
                        ) {
                            return;
                        }
                        onOpenChange(false);
                    }}
                >
                    {hasMultiple ? (
                        <Button
                            type="button"
                            variant="ghost"
                            size="icon-lg"
                            className="absolute top-1/2 left-3 z-10 -translate-y-1/2 bg-background/80"
                            onClick={previous}
                            aria-label={t("images.lightbox.previous")}
                        >
                            <ChevronLeftIcon />
                        </Button>
                    ) : null}
                    <img
                        key={image.src}
                        src={image.src}
                        alt={image.alt}
                        draggable={false}
                        onLoad={(event) => fitImageToStage(event.currentTarget)}
                        onDoubleClick={() => {
                            const nextZoom = doubleClickZoom(zoomRef.current);
                            applyViewport(
                                nextZoom,
                                nextZoom === LIGHTBOX_MIN_ZOOM
                                    ? [0, 0]
                                    : panRef.current,
                            );
                        }}
                        className={cn(
                            "max-h-full max-w-full select-none object-contain",
                            zoom > LIGHTBOX_MIN_ZOOM
                                ? "cursor-grab active:cursor-grabbing"
                                : "cursor-zoom-in",
                        )}
                        style={{
                            transform: `translate3d(${pan[0]}px, ${pan[1]}px, 0) scale(${zoom})`,
                        }}
                    />
                    {hasMultiple ? (
                        <Button
                            type="button"
                            variant="ghost"
                            size="icon-lg"
                            className="absolute top-1/2 right-3 z-10 -translate-y-1/2 bg-background/80"
                            onClick={next}
                            aria-label={t("images.lightbox.next")}
                        >
                            <ChevronRightIcon />
                        </Button>
                    ) : null}
                </div>

                {hasMultiple ? (
                    <div className="absolute inset-x-0 bottom-0 z-20 flex gap-2 overflow-x-auto bg-gradient-to-t from-background/70 to-transparent px-4 py-3">
                        {images.map((candidate, candidateIndex) => (
                            <Button
                                key={candidate.src}
                                type="button"
                                variant="ghost"
                                onClick={() => select(candidateIndex)}
                                aria-label={tf("images.lightbox.view_image", {
                                    index: candidateIndex + 1,
                                })}
                                aria-current={
                                    candidateIndex === index
                                        ? "true"
                                        : undefined
                                }
                                className={cn(
                                    "size-16 h-auto shrink-0 overflow-hidden rounded-md border-2 border-border p-0 opacity-70 transition hover:opacity-100",
                                    candidateIndex === index &&
                                        "-translate-y-0.5 border-primary opacity-100",
                                )}
                            >
                                <img
                                    src={candidate.src}
                                    alt=""
                                    className="size-full object-cover"
                                />
                            </Button>
                        ))}
                    </div>
                ) : null}
            </DialogContent>
        </Dialog>
    );
}
