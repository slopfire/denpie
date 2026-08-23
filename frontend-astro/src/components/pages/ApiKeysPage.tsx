import { useCallback, useState } from "react";
import { useViewRefresh } from "@/islands/use-view-refresh";
import { CheckIcon, CopyIcon, KeyRoundIcon, Trash2Icon } from "lucide-react";
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
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
    Card,
    CardContent,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import type { ApiKeyInfo } from "@/generated/denpie_pb";
import {
    createApiKey,
    deleteApiKey,
    listApiKeys,
} from "@/lib/api-v1/route-ops";
import { newIdempotencyKey } from "@/lib/api-v1/transport";
import { t, tf } from "@/lib/i18n";
import {
    apiKeyDomId,
    formatApiKeyDate,
    sortApiKeys,
} from "@/lib/pages/api-keys";

function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
}

function formatKeyDate(value: string): string {
    return value.trim() === "" ? t("format.never") : formatApiKeyDate(value);
}

export function ApiKeysPage({ active = true }: { active?: boolean }) {
    const [keys, setKeys] = useState<ApiKeyInfo[]>([]);
    const [loading, setLoading] = useState(true);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [clientName, setClientName] = useState("");
    const [secret, setSecret] = useState<string | null>(null);
    const [copied, setCopied] = useState(false);
    const [deleteTarget, setDeleteTarget] = useState<ApiKeyInfo | null>(null);

    const refresh = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            const result = await listApiKeys();
            setKeys(sortApiKeys(result.keys));
        } catch (cause) {
            setError(errorMessage(cause));
        } finally {
            setLoading(false);
        }
    }, []);

    useViewRefresh(active, refresh);

    const handleCreate = async (event: React.FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        setBusy(true);
        setError(null);
        setSecret(null);
        setCopied(false);
        try {
            const result = await createApiKey({
                clientName: clientName.trim(),
                idempotencyKey: newIdempotencyKey(),
            });
            setClientName("");
            setSecret(result.apiKey);
            await refresh();
        } catch (cause) {
            setError(errorMessage(cause));
        } finally {
            setBusy(false);
        }
    };

    const handleDelete = async () => {
        if (deleteTarget === null) return;
        setBusy(true);
        setError(null);
        try {
            await deleteApiKey({
                id: deleteTarget.id,
                idempotencyKey: newIdempotencyKey(),
            });
            setDeleteTarget(null);
            await refresh();
        } catch (cause) {
            setError(errorMessage(cause));
        } finally {
            setBusy(false);
        }
    };

    const copySecret = () => {
        if (secret === null || navigator.clipboard === undefined) return;
        void navigator.clipboard
            .writeText(secret)
            .then(() => setCopied(true))
            .catch(() => setCopied(false));
    };

    return (
        <section
            className="mx-auto flex w-full max-w-5xl flex-col gap-6"
            data-testid="api-keys-page"
        >
            <header>
                <p className="text-sm font-medium text-muted-foreground">
                    {t("api_keys.access")}
                </p>
                <h1 className="text-2xl font-semibold tracking-tight">
                    {t("api_keys.title")}
                </h1>
                <p className="mt-2 max-w-2xl text-sm text-muted-foreground">
                    {t("api_keys.description")}
                </p>
            </header>

            {error === null ? null : (
                <Alert variant="destructive">
                    <AlertTitle>{t("api_keys.update_failed")}</AlertTitle>
                    <AlertDescription>{error}</AlertDescription>
                </Alert>
            )}

            {secret === null ? null : (
                <Alert data-testid="api-key-secret">
                    <KeyRoundIcon aria-hidden="true" />
                    <AlertTitle>{t("api_keys.copy_secret_title")}</AlertTitle>
                    <AlertDescription className="flex flex-col gap-3">
                        <code className="overflow-x-auto rounded-md bg-muted px-3 py-2 text-xs break-all">
                            {secret}
                        </code>
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="self-start"
                            onClick={copySecret}
                        >
                            {copied ? (
                                <CheckIcon data-icon="inline-start" />
                            ) : (
                                <CopyIcon data-icon="inline-start" />
                            )}
                            {copied
                                ? t("api_keys.copied")
                                : t("api_keys.copy_secret")}
                        </Button>
                    </AlertDescription>
                </Alert>
            )}

            <Card>
                <CardHeader>
                    <CardTitle>{t("api_keys.create")}</CardTitle>
                    <CardDescription>
                        {t("api_keys.create_description")}
                    </CardDescription>
                </CardHeader>
                <form onSubmit={handleCreate}>
                    <CardContent className="flex flex-col gap-2 sm:flex-row">
                        <label
                            className="sr-only"
                            htmlFor="api-key-client-name"
                        >
                            {t("api_keys.client_name_label")}
                        </label>
                        <Input
                            id="api-key-client-name"
                            name="clientName"
                            value={clientName}
                            onChange={(event) =>
                                setClientName(event.target.value)
                            }
                            placeholder={t("api_keys.client_name_placeholder")}
                            autoComplete="off"
                        />
                        <Button type="submit" disabled={busy}>
                            {busy
                                ? t("api_keys.creating")
                                : t("api_keys.create_key")}
                        </Button>
                    </CardContent>
                </form>
            </Card>

            <Card>
                <CardHeader>
                    <CardTitle>{t("api_keys.active")}</CardTitle>
                    <CardDescription>
                        {tf(
                            keys.length === 1
                                ? "format.api_key_count_one"
                                : "format.api_key_count_other",
                            { count: keys.length },
                        )}{" "}
                        {t("api_keys.currently_in_account")}
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    {loading ? (
                        <p
                            className="py-8 text-center text-sm text-muted-foreground"
                            role="status"
                        >
                            {t("api_keys.loading")}
                        </p>
                    ) : keys.length === 0 ? (
                        <div className="rounded-lg border border-dashed p-8 text-center">
                            <KeyRoundIcon
                                className="mx-auto mb-3 size-8 text-muted-foreground"
                                aria-hidden="true"
                            />
                            <p className="font-medium">{t("api_keys.empty")}</p>
                            <p className="mt-1 text-sm text-muted-foreground">
                                {t("api_keys.empty_description")}
                            </p>
                        </div>
                    ) : (
                        <div className="grid gap-3 md:grid-cols-2">
                            {keys.map((key) => (
                                <article
                                    key={key.id.toString()}
                                    id={apiKeyDomId(key.id)}
                                    className="rounded-lg border p-4"
                                >
                                    <div className="flex items-start justify-between gap-3">
                                        <div className="min-w-0">
                                            <h3 className="truncate font-medium">
                                                {key.clientName ||
                                                    t(
                                                        "api_keys.unnamed_client",
                                                    )}
                                            </h3>
                                            <p className="mt-1 font-mono text-xs text-muted-foreground">
                                                {tf("format.id", {
                                                    id: key.id.toString(),
                                                })}
                                            </p>
                                        </div>
                                        <Button
                                            type="button"
                                            variant="destructive"
                                            size="icon-sm"
                                            aria-label={tf(
                                                "common.delete_named",
                                                {
                                                    name:
                                                        key.clientName ||
                                                        t("api_keys.api_key"),
                                                },
                                            )}
                                            disabled={busy}
                                            onClick={() => setDeleteTarget(key)}
                                        >
                                            <Trash2Icon />
                                        </Button>
                                    </div>
                                    <dl className="mt-4 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs">
                                        <dt className="text-muted-foreground">
                                            {t("api_keys.created")}
                                        </dt>
                                        <dd>{formatKeyDate(key.createdAt)}</dd>
                                        <dt className="text-muted-foreground">
                                            {t("api_keys.last_used")}
                                        </dt>
                                        <dd>{formatKeyDate(key.lastUsedAt)}</dd>
                                        <dt className="text-muted-foreground">
                                            {t("api_keys.expires")}
                                        </dt>
                                        <dd>{formatKeyDate(key.expiresAt)}</dd>
                                    </dl>
                                    {key.scopes.length === 0 ? null : (
                                        <div className="mt-3 flex flex-wrap gap-1">
                                            {key.scopes.map((scope) => (
                                                <Badge
                                                    key={scope}
                                                    variant="secondary"
                                                >
                                                    {scope}
                                                </Badge>
                                            ))}
                                        </div>
                                    )}
                                </article>
                            ))}
                        </div>
                    )}
                </CardContent>
                <CardFooter className="justify-end">
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => void refresh()}
                        disabled={loading || busy}
                    >
                        {t("common.refresh")}
                    </Button>
                </CardFooter>
            </Card>

            <AlertDialog
                open={deleteTarget !== null}
                onOpenChange={(open) => {
                    if (!open && !busy) setDeleteTarget(null);
                }}
            >
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {t("confirm.delete_api_key")}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {tf("confirm.delete_api_key_description", {
                                name:
                                    deleteTarget?.clientName ||
                                    t("api_keys.this_key"),
                            })}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <AlertDialogCancel disabled={busy}>
                            {t("common.cancel")}
                        </AlertDialogCancel>
                        <AlertDialogAction
                            variant="destructive"
                            disabled={busy}
                            onClick={() => void handleDelete()}
                        >
                            {busy
                                ? t("common.deleting")
                                : t("api_keys.delete_key")}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </section>
    );
}
