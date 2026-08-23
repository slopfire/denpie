import { useCallback, useEffect, useState } from "react";
import {
    ImageIcon,
    KeyRound,
    Plus,
    SaveIcon,
    Trash2,
    UserRound,
} from "lucide-react";
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
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card";
import {
    Field,
    FieldDescription,
    FieldGroup,
    FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import {
    createAuthClient,
    type AuthClient,
    type PasskeyInfo,
} from "@/lib/auth-client";
import type { SessionUser } from "@/lib/auth-types";
import { browserImageDeps, compressFileToDataUrl } from "@/lib/flow-add-images";
import { t, tf } from "@/lib/i18n";
import { useViewRefresh } from "@/islands/use-view-refresh";
import {
    requestPasskeyRegistration,
    type PasskeyCredentialCreator,
} from "@/lib/webauthn-client";

const defaultAuthClient = createAuthClient();

function browserCredentialCreator(): PasskeyCredentialCreator | null {
    if (
        typeof navigator === "undefined" ||
        navigator.credentials === undefined
    ) {
        return null;
    }
    return {
        create: (options) => navigator.credentials.create(options),
    };
}

function passkeyName(passkey: PasskeyInfo): string {
    return passkey.name
        ? tf("account.passkey_name", { name: passkey.name })
        : t("account.passkey_default");
}

function roleLabel(role: string): string {
    switch (role) {
        case "admin":
            return t("account.role_admin");
        case "user":
            return t("account.role_user");
        default:
            return t("account.role_unknown");
    }
}

export function AccountPage({
    user,
    authClient = defaultAuthClient,
    credentialCreator,
    onUserChanged,
    onAccountDeleted,
    active = true,
}: {
    user: SessionUser;
    authClient?: AuthClient;
    credentialCreator?: PasskeyCredentialCreator;
    onUserChanged?: (user: SessionUser) => void;
    onAccountDeleted?: () => void;
    active?: boolean;
}) {
    const displayName = user.display_name ?? user.username;
    const initial = displayName.charAt(0).toUpperCase();
    const [passkeys, setPasskeys] = useState<readonly PasskeyInfo[]>([]);
    const [loading, setLoading] = useState(true);
    const [registering, setRegistering] = useState(false);
    const [deletingId, setDeletingId] = useState<string | null>(null);
    const [pendingDeletion, setPendingDeletion] = useState<PasskeyInfo | null>(
        null,
    );
    const [confirmAccountDeletion, setConfirmAccountDeletion] = useState(false);
    const [draftDisplayName, setDraftDisplayName] = useState(
        user.display_name ?? "",
    );
    const [avatarData, setAvatarData] = useState(user.avatar_data ?? "");
    const [avatarDirty, setAvatarDirty] = useState(false);
    const [password, setPassword] = useState("");
    const [savingProfile, setSavingProfile] = useState(false);
    const [deletingAccount, setDeletingAccount] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [notice, setNotice] = useState<string | null>(null);

    const refreshPasskeys = useCallback(async () => {
        setLoading(true);
        const result = await authClient.listPasskeys();
        if (result.kind === "passkeys") {
            setPasskeys(result.passkeys);
            setError(null);
        } else {
            setError(result.message);
        }
        setLoading(false);
    }, [authClient]);

    useViewRefresh(active, refreshPasskeys);

    useEffect(() => {
        setDraftDisplayName(user.display_name ?? "");
        setAvatarData(user.avatar_data ?? "");
        setAvatarDirty(false);
    }, [user]);

    const selectAvatar = useCallback(async (file: File | undefined) => {
        if (file === undefined) return;
        setError(null);
        try {
            const data = await compressFileToDataUrl(file, browserImageDeps());
            if (data.length > 2 * 1024 * 1024) {
                throw new Error(t("account.avatar_too_large"));
            }
            setAvatarData(data);
            setAvatarDirty(true);
        } catch (cause) {
            setError(
                cause instanceof Error
                    ? cause.message
                    : t("account.avatar_read_failed"),
            );
        }
    }, []);

    const saveProfile = useCallback(
        async (event: React.FormEvent<HTMLFormElement>) => {
            event.preventDefault();
            setSavingProfile(true);
            setError(null);
            setNotice(null);
            const result = await authClient.updateProfile({
                displayName:
                    draftDisplayName.trim() === ""
                        ? undefined
                        : draftDisplayName.trim(),
                avatarData: avatarDirty ? avatarData : undefined,
                password: password === "" ? undefined : password,
            });
            setSavingProfile(false);
            if (result.kind === "error") {
                setError(result.message);
                return;
            }
            setPassword("");
            setAvatarDirty(false);
            setNotice(t("toast.profile_updated"));
            onUserChanged?.(result.user);
        },
        [
            authClient,
            avatarData,
            avatarDirty,
            draftDisplayName,
            onUserChanged,
            password,
        ],
    );

    const deleteAccount = useCallback(async () => {
        setDeletingAccount(true);
        setError(null);
        setNotice(null);
        const result = await authClient.deleteAccount();
        setDeletingAccount(false);
        setConfirmAccountDeletion(false);
        if (result.kind === "error") {
            setError(result.message);
            return;
        }
        onAccountDeleted?.();
    }, [authClient, onAccountDeleted]);

    const registerPasskey = useCallback(async () => {
        if (registering || deletingId !== null) return;

        const creator = credentialCreator ?? browserCredentialCreator();
        if (creator === null) {
            setError(t("account.passkeys_unavailable"));
            return;
        }

        setRegistering(true);
        setError(null);
        setNotice(null);
        const started = await authClient.startPasskeyRegistration();
        if (started.kind === "error") {
            setError(started.message);
            setRegistering(false);
            return;
        }

        const created = await requestPasskeyRegistration({
            credentialCreator: creator,
            request: started.request,
        });
        if (created.kind !== "registration") {
            setError(created.message);
            setRegistering(false);
            return;
        }

        const finished = await authClient.finishPasskeyRegistration(
            created.registration,
        );
        if (finished.kind === "error") {
            setError(finished.message);
            setRegistering(false);
            return;
        }

        setNotice(t("toast.passkey_added"));
        setRegistering(false);
        await refreshPasskeys();
    }, [
        authClient,
        credentialCreator,
        deletingId,
        refreshPasskeys,
        registering,
    ]);

    const deletePasskey = useCallback(async () => {
        if (pendingDeletion === null || deletingId !== null) return;

        setDeletingId(pendingDeletion.id);
        setError(null);
        setNotice(null);
        const result = await authClient.deletePasskey(pendingDeletion.id);
        setDeletingId(null);
        setPendingDeletion(null);
        if (result.kind === "error") {
            setError(result.message);
            return;
        }

        setNotice(t("toast.passkey_deleted"));
        await refreshPasskeys();
    }, [authClient, deletingId, pendingDeletion, refreshPasskeys]);

    return (
        <section
            id="view-account-settings"
            className="space-y-4"
            aria-labelledby="account-title"
        >
            <div>
                <h1
                    id="account-title"
                    className="text-xl font-semibold tracking-tight"
                >
                    {t("account.settings")}
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                    {t("account.subtitle")}
                </p>
            </div>

            {error !== null && (
                <Alert variant="destructive">
                    <AlertTitle>{t("account.action_failed")}</AlertTitle>
                    <AlertDescription>{error}</AlertDescription>
                </Alert>
            )}
            {notice !== null && (
                <Alert>
                    <AlertTitle>{t("account.updated")}</AlertTitle>
                    <AlertDescription>{notice}</AlertDescription>
                </Alert>
            )}

            <div className="grid items-start gap-4 xl:grid-cols-2">
                <Card>
                    <CardHeader>
                        <CardTitle>{t("account.profile")}</CardTitle>
                        <CardDescription>
                            {t("account.profile_description")}
                        </CardDescription>
                    </CardHeader>
                    <CardContent>
                        <form className="space-y-5" onSubmit={saveProfile}>
                            <div className="flex items-center gap-4">
                                <Avatar size="lg">
                                    {avatarData === "" ? null : (
                                        <AvatarImage src={avatarData} alt="" />
                                    )}
                                    <AvatarFallback>{initial}</AvatarFallback>
                                </Avatar>
                                <div className="min-w-0">
                                    <h2 className="truncate text-lg font-semibold">
                                        {user.username}
                                    </h2>
                                    <Badge
                                        variant="secondary"
                                        className="mt-2 capitalize"
                                    >
                                        {roleLabel(user.role)}
                                    </Badge>
                                </div>
                            </div>
                            <FieldGroup>
                                <Field>
                                    <FieldLabel htmlFor="account-display-name">
                                        {t("account.display_name")}
                                    </FieldLabel>
                                    <Input
                                        id="account-display-name"
                                        value={draftDisplayName}
                                        onChange={(event) =>
                                            setDraftDisplayName(
                                                event.target.value,
                                            )
                                        }
                                        placeholder={t(
                                            "account.display_name_placeholder",
                                        )}
                                    />
                                </Field>
                                <Field>
                                    <FieldLabel htmlFor="account-avatar">
                                        {t("account.avatar")}
                                    </FieldLabel>
                                    <div className="flex flex-col gap-2 sm:flex-row">
                                        <Input
                                            id="account-avatar"
                                            type="file"
                                            accept="image/png,image/jpeg,image/webp,image/gif"
                                            onChange={(event) =>
                                                void selectAvatar(
                                                    event.target.files?.[0],
                                                )
                                            }
                                        />
                                        <Button
                                            type="button"
                                            variant="outline"
                                            disabled={avatarData === ""}
                                            onClick={() => {
                                                setAvatarData("");
                                                setAvatarDirty(true);
                                            }}
                                        >
                                            <ImageIcon data-icon="inline-start" />
                                            {t("common.remove")}
                                        </Button>
                                    </div>
                                    <FieldDescription>
                                        {t("account.avatar_description")}
                                    </FieldDescription>
                                </Field>
                                <Field>
                                    <FieldLabel htmlFor="account-password">
                                        {t("account.change_password")}
                                    </FieldLabel>
                                    <Input
                                        id="account-password"
                                        type="password"
                                        autoComplete="new-password"
                                        value={password}
                                        onChange={(event) =>
                                            setPassword(event.target.value)
                                        }
                                        placeholder={t(
                                            "account.password_placeholder",
                                        )}
                                        minLength={
                                            password === "" ? undefined : 8
                                        }
                                    />
                                </Field>
                            </FieldGroup>
                            <div className="flex flex-col gap-2 sm:flex-row sm:justify-between">
                                <Button
                                    type="submit"
                                    disabled={savingProfile || deletingAccount}
                                >
                                    {savingProfile ? (
                                        <Spinner data-icon="inline-start" />
                                    ) : (
                                        <SaveIcon data-icon="inline-start" />
                                    )}
                                    {savingProfile
                                        ? t("account.saving")
                                        : t("common.save_profile")}
                                </Button>
                                <Button
                                    type="button"
                                    variant="destructive"
                                    disabled={savingProfile || deletingAccount}
                                    onClick={() =>
                                        setConfirmAccountDeletion(true)
                                    }
                                >
                                    <Trash2 data-icon="inline-start" />
                                    {t("common.delete_account")}
                                </Button>
                            </div>
                        </form>
                    </CardContent>
                </Card>

                <Card>
                    <CardHeader className="gap-3 sm:flex sm:flex-row sm:items-start sm:justify-between">
                        <div>
                            <CardTitle>{t("account.passkeys")}</CardTitle>
                            <CardDescription>
                                {t("account.passkeys_description")}
                            </CardDescription>
                        </div>
                        <Button
                            type="button"
                            onClick={() => void registerPasskey()}
                            disabled={registering || deletingId !== null}
                        >
                            {registering ? (
                                <Spinner />
                            ) : (
                                <Plus aria-hidden="true" />
                            )}
                            {registering
                                ? t("account.waiting_passkey")
                                : t("account.add_passkey")}
                        </Button>
                    </CardHeader>
                    <CardContent>
                        {loading ? (
                            <div
                                className="flex items-center gap-2 py-2 text-sm text-muted-foreground"
                                role="status"
                            >
                                <Spinner />
                                {t("account.loading_passkeys")}
                            </div>
                        ) : passkeys.length === 0 ? (
                            <p className="py-2 text-sm text-muted-foreground">
                                {t("account.no_passkeys")}
                            </p>
                        ) : (
                            <ul
                                className="space-y-2"
                                aria-label={t("account.registered_passkeys")}
                            >
                                {passkeys.map((passkey) => (
                                    <li
                                        key={passkey.id}
                                        className="flex items-center justify-between gap-3 rounded-md border bg-muted/40 p-2"
                                    >
                                        <div className="flex min-w-0 items-center gap-2">
                                            <KeyRound
                                                className="size-4 shrink-0 text-muted-foreground"
                                                aria-hidden="true"
                                            />
                                            <span className="truncate text-sm font-medium">
                                                {passkeyName(passkey)}
                                            </span>
                                        </div>
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            size="icon-sm"
                                            aria-label={tf(
                                                "common.delete_named",
                                                { name: passkeyName(passkey) },
                                            )}
                                            disabled={
                                                registering ||
                                                deletingId !== null
                                            }
                                            onClick={() =>
                                                setPendingDeletion(passkey)
                                            }
                                        >
                                            <Trash2
                                                className="text-destructive"
                                                aria-hidden="true"
                                            />
                                        </Button>
                                    </li>
                                ))}
                            </ul>
                        )}
                    </CardContent>
                </Card>
            </div>

            <AlertDialog
                open={pendingDeletion !== null}
                onOpenChange={(open) => {
                    if (!open && deletingId === null) setPendingDeletion(null);
                }}
            >
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <UserRound aria-hidden="true" />
                        <AlertDialogTitle>
                            {t("confirm.delete_passkey")}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {pendingDeletion === null
                                ? ""
                                : tf("confirm.delete_passkey_description", {
                                      name: passkeyName(pendingDeletion),
                                  })}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <AlertDialogCancel disabled={deletingId !== null}>
                            {t("common.cancel")}
                        </AlertDialogCancel>
                        <AlertDialogAction
                            variant="destructive"
                            disabled={deletingId !== null}
                            onClick={() => void deletePasskey()}
                        >
                            {deletingId === null
                                ? t("common.delete_passkey")
                                : t("common.deleting")}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>

            <AlertDialog
                open={confirmAccountDeletion}
                onOpenChange={setConfirmAccountDeletion}
            >
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {t("confirm.delete_account")}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {t("confirm.delete_account_description")}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <AlertDialogCancel disabled={deletingAccount}>
                            {t("common.cancel")}
                        </AlertDialogCancel>
                        <AlertDialogAction
                            variant="destructive"
                            disabled={deletingAccount}
                            onClick={() => void deleteAccount()}
                        >
                            {deletingAccount
                                ? t("common.deleting")
                                : t("common.delete_account")}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </section>
    );
}
