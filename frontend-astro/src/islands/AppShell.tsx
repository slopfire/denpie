import {
    useCallback,
    useEffect,
    useRef,
    useState,
    type MouseEvent,
    type ReactNode,
} from "react";
import { flushSync } from "react-dom";
import {
    Antenna,
    Archive,
    ChevronsUpDown,
    CircuitBoard,
    GitCommitHorizontal,
    KeyRound,
    LogOut,
    RefreshCw,
    Settings,
    Shield,
    UserRound,
    Zap,
    type LucideIcon,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Flow } from "@/components/flow/Flow";
import { AccountPage } from "@/components/pages/AccountPage";
import { AdminPage } from "@/components/pages/AdminPage";
import { ApiKeysPage } from "@/components/pages/ApiKeysPage";
import { ArchivePage } from "@/components/pages/ArchivePage";
import { GroundingPage } from "@/components/pages/GroundingPage";
import { SettingsPage } from "@/components/pages/SettingsPage";
import { ToastProvider, useToast } from "@/islands/toast-context";
import { Button } from "@/components/ui/button";
import {
    Card,
    CardContent,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card";
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
    applyLogout,
    createAuthClient,
    toSessionState,
    type LogoutResult,
} from "@/lib/auth-client";
import type { SessionState, SessionUser } from "@/lib/auth-types";
import {
    requestPasskeyAssertion,
    type PasskeyCredentialGetter,
} from "@/lib/webauthn-client";
import { t, tf, type MessageKey } from "@/lib/i18n";
import {
    consumePrefillUsername,
    loadRememberedAccounts,
    otherRememberedAccounts,
    recordRememberedAccount,
    saveRememberedAccounts,
    storePrefillUsername,
} from "@/lib/remembered-accounts";
import { runViewTransition } from "@/lib/view-transition";
import { cn } from "@/lib/utils";

const authClient = createAuthClient();

export type AppView =
    "flow" | "grounding" | "settings" | "keys" | "archive" | "account";

interface NavigationItem {
    readonly labelKey: MessageKey;
    readonly href: string;
    readonly view: Exclude<AppView, "account">;
    readonly icon: LucideIcon;
}

const desktopNavigation = [
    { labelKey: "nav.flow", href: "/", view: "flow", icon: Antenna },
    {
        labelKey: "nav.grounding",
        href: "/grounding",
        view: "grounding",
        icon: CircuitBoard,
    },
    {
        labelKey: "nav.settings",
        href: "/settings",
        view: "settings",
        icon: Settings,
    },
    { labelKey: "nav.api_keys", href: "/keys", view: "keys", icon: KeyRound },
    {
        labelKey: "nav.archive",
        href: "/archive",
        view: "archive",
        icon: Archive,
    },
] as const satisfies readonly NavigationItem[];

const mobileNavigation = [
    desktopNavigation[1],
    desktopNavigation[0],
    desktopNavigation[4],
    desktopNavigation[2],
    desktopNavigation[3],
] as const;

function pathnameForView(view: AppView): string {
    switch (view) {
        case "flow":
            return "/";
        case "grounding":
            return "/grounding";
        case "settings":
            return "/settings";
        case "keys":
            return "/keys";
        case "archive":
            return "/archive";
        case "account":
            return "/account";
    }
}

function titleKeyForView(view: AppView): MessageKey {
    switch (view) {
        case "flow":
            return "nav.flow";
        case "grounding":
            return "nav.grounding";
        case "settings":
            return "nav.settings";
        case "keys":
            return "nav.api_keys";
        case "archive":
            return "nav.archive";
        case "account":
            return "account.menu_title";
    }
}

/** The static Astro routes that can be changed without a document navigation. */
export function viewForPathname(pathname: string): AppView | null {
    const normalized = pathname.replace(/\/+$/, "") || "/";
    switch (normalized) {
        case "/":
        case "/flow":
            return "flow";
        case "/grounding":
            return "grounding";
        case "/settings":
            return "settings";
        case "/keys":
            return "keys";
        case "/archive":
            return "archive";
        case "/account":
            return "account";
        default:
            return null;
    }
}

function isUnmodifiedPrimaryClick(
    event: MouseEvent<HTMLAnchorElement>,
): boolean {
    return (
        event.button === 0 &&
        !event.defaultPrevented &&
        !event.metaKey &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.shiftKey
    );
}

interface NavigationHistoryState {
    readonly kind: "denpie-navigation";
    readonly scrollY: number;
}

function navigationHistoryState(scrollY: number): NavigationHistoryState {
    return { kind: "denpie-navigation", scrollY };
}

function scrollYFromHistoryState(state: unknown): number {
    if (
        typeof state === "object" &&
        state !== null &&
        "kind" in state &&
        "scrollY" in state &&
        state.kind === "denpie-navigation" &&
        typeof state.scrollY === "number" &&
        Number.isFinite(state.scrollY)
    ) {
        return state.scrollY;
    }
    return 0;
}

function roleLabel(role: string): string {
    if (role === "admin") return t("role.admin");
    if (role === "user") return t("role.user");
    return role;
}

function commitUrl(sha: string): string {
    return `https://github.com/slopfire/dailytipdraft/commit/${sha}`;
}

function shortSha(sha: string): string {
    return sha.slice(0, 7);
}

function DesktopSidebar({
    user,
    view,
    otherAccounts,
    onNavigate,
    onRefreshSession,
    onSwitchAccount,
    onAdminMode,
    onLogout,
}: {
    user: SessionUser;
    view: AppView;
    otherAccounts: readonly string[];
    onNavigate: (view: AppView) => void;
    onRefreshSession: () => void;
    onSwitchAccount: (username: string) => void;
    onAdminMode: () => void;
    onLogout: (result: LogoutResult) => void;
}) {
    const displayName = user.display_name ?? user.username;
    const initial = displayName.charAt(0).toUpperCase();
    const showCommit =
        user.build_sha !== "" && user.build_sha !== "unknown";

    return (
        <nav
            className="fixed inset-y-0 left-0 z-50 hidden w-56 flex-col border-r bg-background p-4 lg:flex"
            aria-label={t("common.primary_navigation")}
            data-testid="desktop-sidebar"
        >
            <a
                href="/"
                className="mb-4 flex items-center gap-2 px-2 py-2 text-lg font-semibold tracking-tight"
                onClick={(event) => {
                    if (!isUnmodifiedPrimaryClick(event)) return;
                    event.preventDefault();
                    onNavigate("flow");
                }}
            >
                <Zap className="size-5 text-primary" aria-hidden="true" />
                {t("app.name")}
            </a>
            <div className="flex flex-1 flex-col gap-1">
                {desktopNavigation.map((item) => {
                    const Icon = item.icon;
                    const active = item.view === view;
                    return (
                        <a
                            key={item.view}
                            href={item.href}
                            aria-current={active ? "page" : undefined}
                            onClick={(event) => {
                                if (!isUnmodifiedPrimaryClick(event)) return;
                                event.preventDefault();
                                onNavigate(item.view);
                            }}
                            className={cn(
                                "grid w-full grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-3 rounded-md px-3 py-2 text-left text-sm font-semibold transition-colors",
                                active
                                    ? "bg-muted text-foreground"
                                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                            )}
                        >
                            <Icon
                                className="size-4 justify-self-center"
                                aria-hidden="true"
                            />
                            <span>{t(item.labelKey)}</span>
                        </a>
                    );
                })}
            </div>
            <div className="mt-4 border-t pt-4">
                <DropdownMenu>
                    <DropdownMenuTrigger
                        render={
                            <Button
                                variant="outline"
                                className="h-auto w-full justify-start px-2 py-2"
                            />
                        }
                        data-testid="account-menu-btn"
                    >
                        <Avatar>
                            {user.avatar_data === null ||
                            user.avatar_data.length === 0 ? null : (
                                <AvatarImage src={user.avatar_data} alt="" />
                            )}
                            <AvatarFallback>{initial}</AvatarFallback>
                        </Avatar>
                        <span className="min-w-0 flex-1 text-left">
                            <span className="block truncate text-sm font-semibold">
                                {displayName}
                            </span>
                            <span className="block truncate text-xs text-muted-foreground">
                                {roleLabel(user.role)}
                            </span>
                        </span>
                        <ChevronsUpDown
                            className="ml-auto text-muted-foreground"
                            aria-hidden="true"
                        />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent
                        side="top"
                        align="start"
                        className="w-(--anchor-width) min-w-48"
                    >
                        <DropdownMenuGroup>
                            <DropdownMenuLabel>
                                {t("account.menu_title")}
                            </DropdownMenuLabel>
                            <DropdownMenuItem
                                onClick={() => onNavigate("account")}
                            >
                                <UserRound aria-hidden="true" />
                                {t("account.menu_settings")}
                            </DropdownMenuItem>
                            <DropdownMenuItem onClick={onRefreshSession}>
                                <RefreshCw aria-hidden="true" />
                                {t("account.refresh_session")}
                            </DropdownMenuItem>
                        </DropdownMenuGroup>
                        {otherAccounts.length === 0 ? null : (
                            <>
                                <DropdownMenuSeparator />
                                <DropdownMenuGroup>
                                    {otherAccounts.map((name) => (
                                        <DropdownMenuItem
                                            key={name}
                                            onClick={() =>
                                                onSwitchAccount(name)
                                            }
                                        >
                                            <UserRound aria-hidden="true" />
                                            {tf("account.switch_to", { name })}
                                        </DropdownMenuItem>
                                    ))}
                                </DropdownMenuGroup>
                            </>
                        )}
                        {user.role === "admin" ? (
                            <>
                                <DropdownMenuSeparator />
                                <DropdownMenuGroup>
                                    <DropdownMenuItem onClick={onAdminMode}>
                                        <Shield aria-hidden="true" />
                                        {t("admin.switch_to_admin")}
                                    </DropdownMenuItem>
                                </DropdownMenuGroup>
                            </>
                        ) : null}
                        <DropdownMenuSeparator />
                        <DropdownMenuGroup>
                            <LogoutMenuItem onResult={onLogout} />
                        </DropdownMenuGroup>
                    </DropdownMenuContent>
                </DropdownMenu>
                {showCommit ? (
                    <a
                        href={commitUrl(user.build_sha)}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="mt-3 inline-flex items-center justify-center gap-1.5 rounded-full border px-2.5 py-1 font-mono text-[10px] text-muted-foreground hover:text-foreground"
                        title={t("account.view_commit")}
                    >
                        <GitCommitHorizontal
                            className="size-3"
                            aria-hidden="true"
                        />
                        {shortSha(user.build_sha)}
                    </a>
                ) : null}
            </div>
        </nav>
    );
}

function MobileNavigation({
    view,
    onNavigate,
}: {
    view: AppView;
    onNavigate: (view: AppView) => void;
}) {
    return (
        <nav
            className="grid w-full shrink-0 grid-cols-5 border-t bg-background lg:hidden"
            aria-label={t("common.primary_navigation")}
            data-testid="mobile-navigation"
        >
            {mobileNavigation.map((item) => {
                const Icon = item.icon;
                const active = item.view === view;
                return (
                    <a
                        key={item.view}
                        href={item.href}
                        aria-current={active ? "page" : undefined}
                        onClick={(event) => {
                            if (!isUnmodifiedPrimaryClick(event)) return;
                            event.preventDefault();
                            onNavigate(item.view);
                        }}
                        className={cn(
                            "flex min-h-14 items-center justify-center rounded-md px-2 py-2",
                            active
                                ? "bg-muted text-foreground"
                                : "text-muted-foreground hover:text-foreground",
                        )}
                    >
                        <Icon className="size-5" aria-hidden="true" />
                        <span className="sr-only">{t(item.labelKey)}</span>
                    </a>
                );
            })}
        </nav>
    );
}

function browserCredentialGetter(): PasskeyCredentialGetter | null {
    if (
        typeof navigator === "undefined" ||
        navigator.credentials === undefined
    ) {
        return null;
    }
    return { get: (options) => navigator.credentials.get(options) };
}

type LoginAttempt = "idle" | "password" | "setup" | "passkey";

function LoginForm({
    onAuthenticated,
}: {
    onAuthenticated: (user: SessionUser) => void;
}) {
    const toast = useToast();
    const [username, setUsername] = useState("");
    const [password, setPassword] = useState("");
    const [setupToken, setSetupToken] = useState("");
    const [attempt, setAttempt] = useState<LoginAttempt>("idle");
    const [error, setError] = useState<string | null>(null);
    const busy = attempt !== "idle";
    const isSetup = setupToken.trim() !== "";

    useEffect(() => {
        const prefill = consumePrefillUsername();
        if (prefill !== "") setUsername(prefill);
    }, []);

    async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
        event.preventDefault();
        setAttempt(isSetup ? "setup" : "password");
        setError(null);
        const result = isSetup
            ? await authClient.setup({
                  username,
                  password,
                  setupToken: setupToken.trim(),
              })
            : await authClient.login(username, password);
        setAttempt("idle");
        if (!result.ok) {
            setError(result.message);
            toast.show(result.message, "error");
            return;
        }
        rememberAndEnter(result.user, t("toast.logged_in"));
    }

    function rememberAndEnter(user: SessionUser, message: string) {
        saveRememberedAccounts(
            recordRememberedAccount(loadRememberedAccounts(), user.username),
        );
        toast.show(message, "success");
        onAuthenticated(user);
    }

    async function handlePasskeyLogin() {
        const credentialGetter = browserCredentialGetter();
        if (credentialGetter === null) {
            setError(t("auth.passkey_unavailable"));
            return;
        }
        setAttempt("passkey");
        setError(null);
        const started = await authClient.startPasskeyLogin();
        if (started.kind === "error") {
            setAttempt("idle");
            setError(started.message);
            return;
        }
        const credential = await requestPasskeyAssertion({
            credentialGetter,
            request: started.request,
        });
        if (credential.kind !== "assertion") {
            setAttempt("idle");
            setError(credential.message);
            return;
        }
        const result = await authClient.finishPasskeyLogin(
            credential.assertion,
        );
        setAttempt("idle");
        if (!result.ok) {
            setError(result.message);
            toast.show(result.message, "error");
            return;
        }
        rememberAndEnter(result.user, t("toast.logged_in_passkey"));
    }

    return (
        <Card className="mx-auto w-full max-w-md">
            <CardHeader>
                <CardTitle>{t("auth.sign_in_title")}</CardTitle>
                <CardDescription>{t("auth.description")}</CardDescription>
            </CardHeader>
            <form onSubmit={handleSubmit}>
                <CardContent className="flex flex-col gap-3">
                    {error === null ? null : (
                        <Alert variant="destructive">
                            <AlertTitle>{t("auth.sign_in_failed")}</AlertTitle>
                            <AlertDescription
                                id="auth-error"
                                role="alert"
                                aria-live="assertive"
                            >
                                {error}
                            </AlertDescription>
                        </Alert>
                    )}
                    <Field>
                        <FieldLabel htmlFor="login-username">
                            {t("auth.username")}
                        </FieldLabel>
                        <Input
                            id="login-username"
                            name="username"
                            autoComplete="username"
                            placeholder={t("auth.username")}
                            required
                            value={username}
                            onChange={(event) =>
                                setUsername(event.target.value)
                            }
                        />
                    </Field>
                    <Field>
                        <FieldLabel htmlFor="login-password">
                            {t("auth.password")}
                        </FieldLabel>
                        <Input
                            id="login-password"
                            name="password"
                            type="password"
                            autoComplete={
                                isSetup ? "new-password" : "current-password"
                            }
                            placeholder={t("auth.password")}
                            required
                            value={password}
                            onChange={(event) =>
                                setPassword(event.target.value)
                            }
                        />
                    </Field>
                    <Field>
                        <FieldLabel htmlFor="login-setup-token">
                            {t("auth.setup_token")}
                        </FieldLabel>
                        <Input
                            id="login-setup-token"
                            name="setup-token"
                            type="password"
                            autoComplete="off"
                            placeholder={t("auth.setup_token_placeholder")}
                            value={setupToken}
                            onChange={(event) =>
                                setSetupToken(event.target.value)
                            }
                        />
                        <FieldDescription>
                            {t("auth.setup_token_description")}
                        </FieldDescription>
                    </Field>
                </CardContent>
                <CardFooter className="mt-4 flex-col gap-2 sm:flex-row">
                    <Button
                        type="submit"
                        disabled={busy || !username || !password}
                        data-testid="login-submit"
                        className="w-full sm:w-auto"
                    >
                        {attempt === "password"
                            ? t("auth.signing_in")
                            : attempt === "setup"
                              ? t("auth.setting_up")
                              : isSetup
                                ? t("auth.setup")
                                : t("auth.sign_in")}
                    </Button>
                    <Button
                        type="button"
                        variant="outline"
                        disabled={busy}
                        onClick={() => void handlePasskeyLogin()}
                        className="w-full sm:w-auto"
                        data-testid="passkey-login"
                    >
                        {attempt === "passkey" ? (
                            <Spinner data-icon="inline-start" />
                        ) : (
                            <KeyRound data-icon="inline-start" />
                        )}
                        {attempt === "passkey"
                            ? t("auth.waiting_passkey")
                            : t("auth.use_passkey")}
                    </Button>
                </CardFooter>
            </form>
        </Card>
    );
}

function KeepAliveView({
    mounted,
    active,
    children,
}: {
    mounted: boolean;
    active: boolean;
    children: ReactNode;
}) {
    if (!mounted) return null;
    return (
        <div
            className={active ? undefined : "hidden"}
            hidden={!active}
            aria-hidden={!active}
        >
            {children}
        </div>
    );
}

function AuthenticatedView({
    view,
    user,
    onUserChanged,
    onAccountDeleted,
}: {
    view: AppView;
    user: SessionUser;
    onUserChanged: (user: SessionUser) => void;
    onAccountDeleted: () => void;
}) {
    // First visit must mount in the same synchronous commit as activation:
    // the view-transition update callback snapshots the DOM when it returns,
    // before passive effects run. Adjusting state during render keeps the
    // mount inside the flushSync'd update instead of a follow-up effect.
    const [mounted, setMounted] = useState(() => new Set<AppView>([view]));
    const [lastView, setLastView] = useState(view);
    if (view !== lastView) {
        setLastView(view);
        if (!mounted.has(view)) {
            const next = new Set(mounted);
            next.add(view);
            setMounted(next);
        }
    }
    return (
        <>
            <KeepAliveView mounted={mounted.has("flow")} active={view === "flow"}>
                <div id="flow-view">
                    <Flow active={view === "flow"} />
                </div>
            </KeepAliveView>
            <KeepAliveView
                mounted={mounted.has("grounding")}
                active={view === "grounding"}
            >
                <GroundingPage active={view === "grounding"} />
            </KeepAliveView>
            <KeepAliveView
                mounted={mounted.has("settings")}
                active={view === "settings"}
            >
                <SettingsPage
                    active={view === "settings"}
                    isAdmin={user.role === "admin"}
                />
            </KeepAliveView>
            <KeepAliveView mounted={mounted.has("keys")} active={view === "keys"}>
                <ApiKeysPage active={view === "keys"} />
            </KeepAliveView>
            <KeepAliveView
                mounted={mounted.has("archive")}
                active={view === "archive"}
            >
                <ArchivePage active={view === "archive"} />
            </KeepAliveView>
            <KeepAliveView
                mounted={mounted.has("account")}
                active={view === "account"}
            >
                <AccountPage
                    active={view === "account"}
                    user={user}
                    onUserChanged={onUserChanged}
                    onAccountDeleted={onAccountDeleted}
                />
            </KeepAliveView>
        </>
    );
}

/** Shared authentication, sidebar, and mobile dock for every static Astro route. */
export function AppShell({ view = "flow" }: { view?: AppView }) {
    return (
        <ToastProvider>
            <AppShellInner view={view} />
        </ToastProvider>
    );
}

function AppShellInner({ view = "flow" }: { view?: AppView }) {
    const toast = useToast();
    const [state, setState] = useState<SessionState>({ status: "checking" });
    const [activeView, setActiveView] = useState(view);
    const [adminMode, setAdminMode] = useState(false);
    const mainRef = useRef<HTMLElement>(null);

    const refresh = useCallback(async (showChecking = true) => {
        if (showChecking) setState({ status: "checking" });
        setState(toSessionState(await authClient.fetchMe()));
    }, []);

    useEffect(() => {
        void refresh();
    }, [refresh]);

    useEffect(() => {
        setActiveView(view);
    }, [view]);

    useEffect(() => {
        document.title = `${t(titleKeyForView(activeView))} · ${t("app.name")}`;
    }, [activeView]);

    const navigate = useCallback((nextView: AppView) => {
        const nextPathname = pathnameForView(nextView);
        if (viewForPathname(window.location.pathname) === nextView) return;
        window.history.replaceState(
            navigationHistoryState(
                Math.max(mainRef.current?.scrollTop ?? 0, window.scrollY),
            ),
            "",
            window.location.href,
        );
        window.history.pushState(navigationHistoryState(0), "", nextPathname);
        runViewTransition(() => {
            flushSync(() => setActiveView(nextView));
            mainRef.current?.scrollTo(0, 0);
            window.scrollTo(0, 0);
        });
    }, []);

    useEffect(() => {
        const onPopState = (event: PopStateEvent) => {
            const nextView = viewForPathname(window.location.pathname);
            if (nextView === null) return;
            const scrollY = scrollYFromHistoryState(event.state);
            // Scroll restoration belongs inside the transition callback: the
            // browser snapshots the new state once the callback resolves, so
            // an async rAF restore would be captured mid-animation.
            runViewTransition(() => {
                flushSync(() => setActiveView(nextView));
                mainRef.current?.scrollTo(0, scrollY);
                window.scrollTo(0, scrollY);
            });
        };
        window.addEventListener("popstate", onPopState);
        return () => window.removeEventListener("popstate", onPopState);
    }, []);

    const authenticatedUserId =
        state.status === "authenticated" ? state.user.id : null;
    useEffect(() => {
        if (authenticatedUserId === null) return;
        let active = true;
        void import("@/lib/appearance-client")
            .then(({ fetchAppearancePreferences }) =>
                fetchAppearancePreferences(),
            )
            .then((appearance) => {
                if (!active) return;
                const root = document.documentElement;
                root.dataset.theme = appearance.theme;
                root.dataset.transparency = appearance.transparency;
                root.dataset.blurIntensity = appearance.blur;
                window.localStorage.setItem(
                    "denpie-appearance",
                    JSON.stringify(appearance),
                );
            })
            .catch(() => undefined);
        return () => {
            active = false;
        };
    }, [authenticatedUserId]);

    const handleLogout = useCallback((result: LogoutResult) => {
        setAdminMode(false);
        if (result.ok) toast.show(t("toast.logged_out"), "success");
        setState((previous) => applyLogout(previous, result));
    }, [toast]);

    const handleSwitchAccount = useCallback(
        async (username: string) => {
            storePrefillUsername(username);
            const result = await authClient.logout();
            setAdminMode(false);
            if (result.ok) toast.show(t("toast.switched_account"), "info");
            setState((previous) => applyLogout(previous, result));
        },
        [toast],
    );

    const handleRefreshSession = useCallback(async () => {
        const result = await authClient.fetchMe();
        const next = toSessionState(result);
        setState(next);
        if (next.status === "authenticated") {
            toast.show(t("toast.profile_refreshed"), "success");
        } else {
            toast.show(t("toast.profile_refresh_failed"), "error");
        }
    }, [toast]);

    const authenticated = state.status === "authenticated";
    const mainClassName = authenticated
        ? "app-main min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-4 py-5 pb-20 sm:px-6 lg:ml-56 lg:overflow-visible lg:px-6"
        : "app-main min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-4 py-5 pb-20 sm:px-6 lg:px-6";

    return (
        <div className="flex h-dvh min-h-0 flex-col overflow-hidden lg:h-auto lg:min-h-screen lg:overflow-visible">
            {state.status === "authenticated" && !adminMode ? (
                <DesktopSidebar
                    user={state.user}
                    view={activeView}
                    otherAccounts={
                        typeof window === "undefined"
                            ? []
                            : otherRememberedAccounts(
                                  loadRememberedAccounts(),
                                  state.user.username,
                              )
                    }
                    onNavigate={navigate}
                    onRefreshSession={() => void handleRefreshSession()}
                    onSwitchAccount={(name) => void handleSwitchAccount(name)}
                    onAdminMode={() => setAdminMode(true)}
                    onLogout={handleLogout}
                />
            ) : null}
            <main
                ref={mainRef}
                className={mainClassName}
                data-testid="app-main"
            >
                <TooltipProvider>
                    {state.status === "checking" ? (
                        <p
                            className="text-sm text-muted-foreground"
                            role="status"
                        >
                            {t("auth.checking_session_ellipsis")}
                        </p>
                    ) : null}
                    {state.status === "guest" ? (
                        <LoginForm
                            onAuthenticated={(user) =>
                                setState({ status: "authenticated", user })
                            }
                        />
                    ) : null}
                    {state.status === "authenticated" ? (
                        <div id="auth-session">
                            {state.notice ? (
                                <Alert
                                    variant="destructive"
                                    className="mb-4 max-w-md"
                                >
                                    <AlertTitle>
                                        {t("auth.sign_out_failed")}
                                    </AlertTitle>
                                    <AlertDescription id="logout-notice">
                                        {tf("auth.sign_out_notice", {
                                            message: state.notice,
                                        })}
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            {adminMode ? (
                                <AdminPage
                                    user={state.user}
                                    onLeave={() => setAdminMode(false)}
                                    onLogout={handleLogout}
                                />
                            ) : (
                                <AuthenticatedView
                                    view={activeView}
                                    user={state.user}
                                    onUserChanged={(user) =>
                                        setState({
                                            status: "authenticated",
                                            user,
                                        })
                                    }
                                    onAccountDeleted={() =>
                                        setState({ status: "guest" })
                                    }
                                />
                            )}
                        </div>
                    ) : null}
                    {state.status === "error" ? (
                        <Card className="mx-auto w-full max-w-md border-destructive">
                            <CardHeader>
                                <CardTitle>
                                    {t("auth.session_check_failed")}
                                </CardTitle>
                            </CardHeader>
                            <CardContent>
                                <p
                                    id="auth-error"
                                    role="alert"
                                    aria-live="assertive"
                                    className="text-sm text-destructive"
                                >
                                    {state.message}
                                </p>
                            </CardContent>
                            <CardFooter className="gap-2">
                                <Button onClick={() => void refresh()}>
                                    {t("common.retry")}
                                </Button>
                                <Button
                                    variant="ghost"
                                    onClick={() =>
                                        setState({ status: "guest" })
                                    }
                                >
                                    {t("auth.back_to_sign_in")}
                                </Button>
                            </CardFooter>
                        </Card>
                    ) : null}
                </TooltipProvider>
            </main>
            {state.status === "authenticated" && !adminMode ? (
                <MobileNavigation view={activeView} onNavigate={navigate} />
            ) : null}
        </div>
    );
}

function LogoutMenuItem({
    onResult,
}: {
    onResult: (result: LogoutResult) => void;
}) {
    const [busy, setBusy] = useState(false);
    return (
        <DropdownMenuItem
            variant="destructive"
            data-testid="logout-btn"
            disabled={busy}
            onClick={async () => {
                setBusy(true);
                const result = await authClient.logout();
                setBusy(false);
                onResult(result);
            }}
        >
            <LogOut aria-hidden="true" />
            {busy ? t("auth.signing_out") : t("auth.sign_out")}
        </DropdownMenuItem>
    );
}
