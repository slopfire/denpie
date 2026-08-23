import { useCallback, useEffect, useState } from "react";
import { ImagePlusIcon, Trash2Icon } from "lucide-react";
import type { FlowCardInfo, PoolImageInfo } from "@/generated/denpie_pb";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
    appendTipcardImages,
    getTipcard,
    replaceTipcardImages,
} from "@/lib/api-v1/ops";
import { listPoolImages } from "@/lib/api-v1/route-ops";
import { newIdempotencyKey } from "@/lib/api-v1/transport";
import {
    browserImageDeps,
    compressFilesToDataUrls,
    validateImageFiles,
} from "@/lib/flow-add-images";
import { t, tf } from "@/lib/i18n";

type ManagerState =
    { kind: "idle" } | { kind: "saving" } | { kind: "error"; message: string };

function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
}

export function CardImageManager({
    card,
    onChanged,
}: {
    card: FlowCardInfo;
    onChanged: (card: FlowCardInfo) => void;
}) {
    const [open, setOpen] = useState(false);
    const [files, setFiles] = useState<readonly File[]>([]);
    const [url, setUrl] = useState("");
    const [pool, setPool] = useState<readonly PoolImageInfo[]>([]);
    const [selectedPoolIds, setSelectedPoolIds] = useState<readonly string[]>(
        [],
    );
    const [confirmClear, setConfirmClear] = useState(false);
    const [state, setState] = useState<ManagerState>({ kind: "idle" });
    const available = Math.max(0, 4 - card.images.length);

    useEffect(() => {
        if (!open) return;
        void listPoolImages()
            .then((result) => setPool(result.images))
            .catch((error) =>
                setState({ kind: "error", message: errorMessage(error) }),
            );
    }, [open]);

    const refresh = useCallback(async () => {
        const detail = await getTipcard({ cardId: card.id });
        onChanged(detail.card);
    }, [card.id, onChanged]);

    const save = useCallback(async () => {
        setState({ kind: "saving" });
        try {
            if (files.length > 0) {
                validateImageFiles(files, card.images.length);
            }
            const imageData =
                files.length === 0
                    ? []
                    : await compressFilesToDataUrls(files, browserImageDeps());
            await appendTipcardImages({
                cardId: card.id,
                imageData,
                poolImageIds: selectedPoolIds.map((id) => BigInt(id)),
                urls: url.trim() === "" ? [] : [url],
                idempotencyKey: newIdempotencyKey(),
            });
            await refresh();
            setFiles([]);
            setSelectedPoolIds([]);
            setUrl("");
            setState({ kind: "idle" });
            setOpen(false);
        } catch (error) {
            setState({ kind: "error", message: errorMessage(error) });
        }
    }, [card.id, card.images.length, files, refresh, selectedPoolIds, url]);

    const clear = useCallback(async () => {
        setState({ kind: "saving" });
        try {
            await replaceTipcardImages({
                cardId: card.id,
                imageData: [],
                idempotencyKey: newIdempotencyKey(),
            });
            await refresh();
            setState({ kind: "idle" });
            setOpen(false);
        } catch (error) {
            setState({ kind: "error", message: errorMessage(error) });
        }
    }, [card.id, refresh]);

    const selectionCount =
        files.length + selectedPoolIds.length + (url.trim() === "" ? 0 : 1);
    const disabled =
        state.kind === "saving" ||
        selectionCount === 0 ||
        selectionCount > available;

    return (
        <Dialog open={open} onOpenChange={setOpen}>
            <DialogTrigger
                render={<Button variant="outline" size="sm" />}
                data-testid={`manage-images-${card.id}`}
            >
                <ImagePlusIcon data-icon="inline-start" />
                {t("images.label")}
            </DialogTrigger>
            <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-3xl">
                <DialogHeader className="pr-10">
                    <DialogTitle>{t("images.manage_title")}</DialogTitle>
                    <DialogDescription>
                        {tf("images.manage_description", {
                            count: card.images.length,
                            available,
                        })}
                    </DialogDescription>
                </DialogHeader>
                {state.kind === "error" ? (
                    <Alert variant="destructive">
                        <AlertTitle>{t("images.update_error")}</AlertTitle>
                        <AlertDescription role="alert">
                            {state.message}
                        </AlertDescription>
                    </Alert>
                ) : null}
                {available > 0 ? (
                    <Tabs defaultValue="upload">
                        <TabsList>
                            <TabsTrigger value="upload">
                                {t("images.upload")}
                            </TabsTrigger>
                            <TabsTrigger value="pool">
                                {t("images.pool")}
                            </TabsTrigger>
                            <TabsTrigger value="url">
                                {t("images.url")}
                            </TabsTrigger>
                        </TabsList>
                        <TabsContent value="upload">
                            <FieldGroup>
                                <Field>
                                    <FieldLabel
                                        htmlFor={`card-images-${card.id}`}
                                    >
                                        {t("images.files")}
                                    </FieldLabel>
                                    <Input
                                        id={`card-images-${card.id}`}
                                        type="file"
                                        accept="image/png,image/jpeg,image/webp,image/gif"
                                        multiple
                                        disabled={state.kind === "saving"}
                                        onChange={(event) =>
                                            setFiles(
                                                Array.from(
                                                    event.target.files ?? [],
                                                ),
                                            )
                                        }
                                    />
                                </Field>
                            </FieldGroup>
                        </TabsContent>
                        <TabsContent value="pool">
                            {pool.length === 0 ? (
                                <p className="text-sm text-muted-foreground">
                                    {t("images.pool_empty")}
                                </p>
                            ) : (
                                <ToggleGroup
                                    multiple
                                    value={[...selectedPoolIds]}
                                    onValueChange={setSelectedPoolIds}
                                    variant="outline"
                                    className="grid w-full grid-cols-2 gap-2 sm:grid-cols-3"
                                >
                                    {pool.map((image) => (
                                        <ToggleGroupItem
                                            key={image.id.toString()}
                                            value={image.id.toString()}
                                            className="h-auto min-w-0 flex-col p-2"
                                        >
                                            <img
                                                src={`/api/v1/pool-images/${image.id}`}
                                                alt=""
                                                className="aspect-video w-full rounded object-cover"
                                            />
                                            <span className="w-full truncate text-xs">
                                                {image.name}
                                            </span>
                                        </ToggleGroupItem>
                                    ))}
                                </ToggleGroup>
                            )}
                        </TabsContent>
                        <TabsContent value="url">
                            <FieldGroup>
                                <Field>
                                    <FieldLabel
                                        htmlFor={`card-image-url-${card.id}`}
                                    >
                                        {t("images.public_url")}
                                    </FieldLabel>
                                    <Input
                                        id={`card-image-url-${card.id}`}
                                        type="url"
                                        placeholder="https://example.com/image.png"
                                        value={url}
                                        onChange={(event) =>
                                            setUrl(event.target.value)
                                        }
                                    />
                                </Field>
                            </FieldGroup>
                        </TabsContent>
                    </Tabs>
                ) : (
                    <Alert>
                        <AlertTitle>{t("images.limit_reached")}</AlertTitle>
                        <AlertDescription>
                            {t("images.clear_before_attach")}
                        </AlertDescription>
                    </Alert>
                )}
                {selectionCount > available ? (
                    <Alert variant="destructive">
                        <AlertTitle>{t("images.too_many")}</AlertTitle>
                        <AlertDescription>
                            {tf("images.select_at_most", { count: available })}
                        </AlertDescription>
                    </Alert>
                ) : null}
                <DialogFooter className="flex-row justify-between sm:justify-between">
                    <Button
                        type="button"
                        variant="destructive"
                        disabled={
                            state.kind === "saving" || card.images.length === 0
                        }
                        onClick={() => setConfirmClear(true)}
                    >
                        <Trash2Icon data-icon="inline-start" />
                        {t("images.clear")}
                    </Button>
                    <Button
                        type="button"
                        disabled={disabled}
                        onClick={() => void save()}
                    >
                        {state.kind === "saving" ? (
                            <Spinner data-icon="inline-start" />
                        ) : (
                            <ImagePlusIcon data-icon="inline-start" />
                        )}
                        {state.kind === "saving"
                            ? t("common.saving")
                            : t("images.attach")}
                    </Button>
                </DialogFooter>
            </DialogContent>
            <AlertDialog open={confirmClear} onOpenChange={setConfirmClear}>
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {t("confirm.clear_images_title")}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {tf("confirm.clear_images_description", {
                                count: card.images.length,
                            })}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <AlertDialogCancel>
                            {t("common.cancel")}
                        </AlertDialogCancel>
                        <AlertDialogAction
                            variant="destructive"
                            onClick={() => void clear()}
                        >
                            {t("images.clear")}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </Dialog>
    );
}
