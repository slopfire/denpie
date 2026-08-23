import {
    createContext,
    useCallback,
    useContext,
    useEffect,
    useRef,
    useState,
    type ReactNode,
} from "react";
import { CircleAlertIcon, InfoIcon, XIcon } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { t } from "@/lib/i18n";
import {
    classifyToast,
    toastTimeoutMs,
    type ShownToast,
    type ToastKind,
} from "@/lib/toast";

interface ToastApi {
    show: (message: string, kind?: ToastKind) => void;
}

const ToastContext = createContext<ToastApi>({
    show: () => undefined,
});

export function useToast(): ToastApi {
    return useContext(ToastContext);
}

export function ToastProvider({ children }: { children: ReactNode }) {
    const [toast, setToast] = useState<ShownToast | null>(null);
    const [expanded, setExpanded] = useState(false);
    const nextId = useRef(1);

    const show = useCallback((message: string, kind: ToastKind = "info") => {
        const parts = classifyToast(message, kind);
        setExpanded(false);
        setToast({ ...parts, id: nextId.current++ });
    }, []);

    useEffect(() => {
        if (toast === null) return;
        const timeout = toastTimeoutMs(toast.kind);
        if (timeout === null) return;
        const timer = window.setTimeout(() => setToast(null), timeout);
        return () => window.clearTimeout(timer);
    }, [toast]);

    return (
        <ToastContext.Provider value={{ show }}>
            {children}
            {toast === null ? null : (
                <div
                    id="toast"
                    className="pointer-events-none fixed inset-x-0 bottom-20 z-80 flex justify-center px-4 lg:bottom-6"
                    data-testid="app-toast"
                >
                    <Alert
                        variant={
                            toast.kind === "error" ? "destructive" : "default"
                        }
                        className="pointer-events-auto w-full max-w-md shadow-lg"
                        role={toast.kind === "error" ? "alert" : "status"}
                        aria-live={
                            toast.kind === "error" ? "assertive" : "polite"
                        }
                    >
                        {toast.kind === "error" ? (
                            <CircleAlertIcon />
                        ) : (
                            <InfoIcon />
                        )}
                        <AlertTitle className="flex items-start justify-between gap-2">
                            <span>{toast.summary}</span>
                            <Button
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                aria-label={t("toast.dismiss")}
                                onClick={() => setToast(null)}
                            >
                                <XIcon />
                            </Button>
                        </AlertTitle>
                        {toast.detail === undefined ? null : (
                            <AlertDescription className="flex flex-col gap-2">
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className="self-start"
                                    onClick={() =>
                                        setExpanded((current) => !current)
                                    }
                                >
                                    {expanded
                                        ? t("toast.hide_detail")
                                        : t("toast.show_detail")}
                                </Button>
                                {expanded ? (
                                    <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-words font-mono text-xs">
                                        {toast.detail}
                                    </pre>
                                ) : null}
                            </AlertDescription>
                        )}
                    </Alert>
                </div>
            )}
        </ToastContext.Provider>
    );
}
