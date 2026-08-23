import { useCallback, useState } from "react";
import { ArrowLeft, LogOut, Pencil, Trash2 } from "lucide-react";
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
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select";
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table";
import { createAuthClient, type LogoutResult } from "@/lib/auth-client";
import type { SessionUser } from "@/lib/auth-types";
import {
    createAdminUser,
    deleteAdminUser,
    listAdminUsers,
    updateAdminUser,
    type AdminUser,
} from "@/lib/dashboard-session";
import { t } from "@/lib/i18n";
import { useToast } from "@/islands/toast-context";
import { useViewRefresh } from "@/islands/use-view-refresh";

const authClient = createAuthClient();

function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : t("toast.request_failed");
}

function roleLabel(role: string): string {
    if (role === "admin") return t("role.admin");
    if (role === "user") return t("role.user");
    return role;
}

export function AdminPage({
    user,
    onLeave,
    onLogout,
}: {
    user: SessionUser;
    onLeave: () => void;
    onLogout: (result: LogoutResult) => void;
}) {
    const toast = useToast();
    const [users, setUsers] = useState<AdminUser[]>([]);
    const [loading, setLoading] = useState(true);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [username, setUsername] = useState("");
    const [password, setPassword] = useState("");
    const [displayName, setDisplayName] = useState("");
    const [role, setRole] = useState("user");
    const [editing, setEditing] = useState<AdminUser | null>(null);
    const [editRole, setEditRole] = useState("user");
    const [editPassword, setEditPassword] = useState("");
    const [deleteTarget, setDeleteTarget] = useState<AdminUser | null>(null);

    const refresh = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            setUsers(await listAdminUsers());
        } catch (cause) {
            setError(errorMessage(cause));
        } finally {
            setLoading(false);
        }
    }, []);

    useViewRefresh(true, refresh);

    const create = async (event: React.FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        if (username.trim() === "" || password.length < 8) {
            toast.show(t("admin.validation_username_password"), "error");
            return;
        }
        setBusy(true);
        try {
            await createAdminUser({
                username: username.trim(),
                password,
                role,
                displayName,
            });
            setUsername("");
            setPassword("");
            setDisplayName("");
            setRole("user");
            toast.show(t("admin.user_created"), "success");
            await refresh();
        } catch (cause) {
            toast.show(errorMessage(cause), "error");
        } finally {
            setBusy(false);
        }
    };

    const saveEdit = async () => {
        if (editing === null) return;
        setBusy(true);
        try {
            await updateAdminUser({
                id: editing.id,
                role: editRole,
                password: editPassword,
            });
            setEditing(null);
            setEditPassword("");
            toast.show(t("admin.user_updated"), "success");
            await refresh();
        } catch (cause) {
            toast.show(errorMessage(cause), "error");
        } finally {
            setBusy(false);
        }
    };

    const remove = async () => {
        if (deleteTarget === null) return;
        if (deleteTarget.id === user.id) {
            toast.show(t("admin.cannot_delete_self"), "error");
            return;
        }
        setBusy(true);
        try {
            await deleteAdminUser({ id: deleteTarget.id });
            setDeleteTarget(null);
            toast.show(t("admin.user_deleted"), "success");
            await refresh();
        } catch (cause) {
            toast.show(errorMessage(cause), "error");
        } finally {
            setBusy(false);
        }
    };

    return (
        <div
            className="mx-auto flex w-full max-w-5xl flex-col gap-6"
            data-testid="admin-page"
        >
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                    <h1 className="text-2xl font-semibold tracking-tight">
                        {t("admin.user_management")}
                    </h1>
                    <p className="mt-2 text-sm text-muted-foreground">
                        {t("admin.subtitle")}
                    </p>
                </div>
                <div className="flex gap-2">
                    <Button variant="outline" onClick={onLeave}>
                        <ArrowLeft data-icon="inline-start" />
                        {t("admin.switch_to_app")}
                    </Button>
                    <Button
                        variant="destructive"
                        onClick={async () => {
                            onLogout(await authClient.logout());
                        }}
                    >
                        <LogOut data-icon="inline-start" />
                        {t("nav.logout")}
                    </Button>
                </div>
            </div>

            {error === null ? null : (
                <Alert variant="destructive">
                    <AlertTitle>{t("admin.loading")}</AlertTitle>
                    <AlertDescription>{error}</AlertDescription>
                </Alert>
            )}

            <Card>
                <CardHeader>
                    <CardTitle>{t("admin.create_user")}</CardTitle>
                    <CardDescription>
                        {t("admin.validation_username_password")}
                    </CardDescription>
                </CardHeader>
                <form onSubmit={(event) => void create(event)}>
                    <CardContent>
                        <FieldGroup className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                            <Field>
                                <FieldLabel htmlFor="admin-username">
                                    {t("auth.username")}
                                </FieldLabel>
                                <Input
                                    id="admin-username"
                                    value={username}
                                    autoComplete="off"
                                    onChange={(event) =>
                                        setUsername(event.target.value)
                                    }
                                />
                            </Field>
                            <Field>
                                <FieldLabel htmlFor="admin-password">
                                    {t("admin.password")}
                                </FieldLabel>
                                <Input
                                    id="admin-password"
                                    type="password"
                                    value={password}
                                    autoComplete="new-password"
                                    onChange={(event) =>
                                        setPassword(event.target.value)
                                    }
                                />
                            </Field>
                            <Field>
                                <FieldLabel htmlFor="admin-display-name">
                                    {t("admin.display_name_optional")}
                                </FieldLabel>
                                <Input
                                    id="admin-display-name"
                                    value={displayName}
                                    onChange={(event) =>
                                        setDisplayName(event.target.value)
                                    }
                                />
                            </Field>
                            <Field>
                                <FieldLabel>{t("admin.role")}</FieldLabel>
                                <Select
                                    value={role}
                                    onValueChange={(value) =>
                                        value !== null && setRole(value)
                                    }
                                >
                                    <SelectTrigger className="w-full">
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value="user">
                                            {t("role.user")}
                                        </SelectItem>
                                        <SelectItem value="admin">
                                            {t("role.admin")}
                                        </SelectItem>
                                    </SelectContent>
                                </Select>
                            </Field>
                        </FieldGroup>
                    </CardContent>
                    <div className="px-6 pb-6">
                        <Button type="submit" disabled={busy}>
                            {busy
                                ? t("admin.creating")
                                : t("admin.create_user")}
                        </Button>
                    </div>
                </form>
            </Card>

            <Card>
                <CardHeader>
                    <CardTitle>{t("admin.users")}</CardTitle>
                </CardHeader>
                <CardContent>
                    {loading ? (
                        <p className="text-sm text-muted-foreground" role="status">
                            {t("admin.loading")}
                        </p>
                    ) : users.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            {t("admin.no_users")}
                        </p>
                    ) : (
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>{t("auth.username")}</TableHead>
                                    <TableHead>{t("admin.role")}</TableHead>
                                    <TableHead>
                                        {t("admin.created_at")}
                                    </TableHead>
                                    <TableHead className="text-right">
                                        {t("admin.actions")}
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {users.map((entry) => (
                                    <TableRow key={entry.id}>
                                        <TableCell>
                                            <div className="flex items-center gap-2">
                                                <span>
                                                    {entry.displayName ||
                                                        entry.username}
                                                </span>
                                                {entry.id === user.id ? (
                                                    <Badge variant="secondary">
                                                        {t("admin.you")}
                                                    </Badge>
                                                ) : null}
                                            </div>
                                            {entry.displayName === null ? null : (
                                                <p className="text-xs text-muted-foreground">
                                                    {entry.username}
                                                </p>
                                            )}
                                        </TableCell>
                                        <TableCell>
                                            {roleLabel(entry.role)}
                                        </TableCell>
                                        <TableCell className="font-mono text-xs">
                                            {entry.createdAt}
                                        </TableCell>
                                        <TableCell className="text-right">
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                size="icon-sm"
                                                aria-label={t("admin.edit")}
                                                disabled={busy}
                                                onClick={() => {
                                                    setEditing(entry);
                                                    setEditRole(entry.role);
                                                    setEditPassword("");
                                                }}
                                            >
                                                <Pencil />
                                            </Button>
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                size="icon-sm"
                                                aria-label={t(
                                                    "admin.confirm_delete",
                                                )}
                                                disabled={
                                                    busy || entry.id === user.id
                                                }
                                                onClick={() =>
                                                    setDeleteTarget(entry)
                                                }
                                            >
                                                <Trash2 />
                                            </Button>
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    )}
                </CardContent>
            </Card>

            <AlertDialog
                open={editing !== null}
                onOpenChange={(open) => {
                    if (!open && !busy) setEditing(null);
                }}
            >
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {t("admin.edit")}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {editing?.username}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <FieldGroup>
                        <Field>
                            <FieldLabel>{t("admin.role")}</FieldLabel>
                            <Select
                                value={editRole}
                                onValueChange={(value) =>
                                    value !== null && setEditRole(value)
                                }
                            >
                                <SelectTrigger className="w-full">
                                    <SelectValue />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="user">
                                        {t("role.user")}
                                    </SelectItem>
                                    <SelectItem value="admin">
                                        {t("role.admin")}
                                    </SelectItem>
                                </SelectContent>
                            </Select>
                        </Field>
                        <Field>
                            <FieldLabel htmlFor="admin-edit-password">
                                {t("admin.new_password_optional")}
                            </FieldLabel>
                            <Input
                                id="admin-edit-password"
                                type="password"
                                value={editPassword}
                                onChange={(event) =>
                                    setEditPassword(event.target.value)
                                }
                            />
                        </Field>
                    </FieldGroup>
                    <AlertDialogFooter>
                        <AlertDialogCancel disabled={busy}>
                            {t("common.cancel")}
                        </AlertDialogCancel>
                        <AlertDialogAction
                            disabled={busy}
                            onClick={() => void saveEdit()}
                        >
                            {busy ? t("admin.saving") : t("common.save")}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>

            <AlertDialog
                open={deleteTarget !== null}
                onOpenChange={(open) => {
                    if (!open && !busy) setDeleteTarget(null);
                }}
            >
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {t("admin.confirm_delete")}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {deleteTarget?.username}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <AlertDialogCancel disabled={busy}>
                            {t("common.cancel")}
                        </AlertDialogCancel>
                        <AlertDialogAction
                            variant="destructive"
                            disabled={busy}
                            onClick={() => void remove()}
                        >
                            {t("common.delete")}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </div>
    );
}
