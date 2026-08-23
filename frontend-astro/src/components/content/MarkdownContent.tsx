import { useState } from "react";
import Prism from "prismjs";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
    CheckIcon,
    ChevronDownIcon,
    ChevronUpIcon,
    CopyIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { t, tf } from "@/lib/i18n";
import {
    isLongCodeFence,
    prismLanguage,
    safeMarkdownUrl,
} from "@/lib/markdown-content";

export interface MarkdownContentProps {
    content: string;
    className?: string;
}

function escapeCode(value: string): string {
    return value
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;");
}

function CodeFence({ code, language }: { code: string; language: string }) {
    const [expanded, setExpanded] = useState(false);
    const [copied, setCopied] = useState(false);
    const collapsible = isLongCodeFence(code);
    const grammar = Prism.languages[language];
    const highlighted =
        grammar === undefined
            ? escapeCode(code)
            : Prism.highlight(code, grammar, language);

    const copy = () => {
        if (navigator.clipboard === undefined) return;
        void navigator.clipboard
            .writeText(code)
            .then(() => {
                setCopied(true);
                window.setTimeout(() => setCopied(false), 1_200);
            })
            .catch(() => setCopied(false));
    };

    return (
        <div
            className="my-3 overflow-hidden rounded-md border border-border bg-muted/50"
            data-line-count={code.split("\n").length}
            data-language={language}
        >
            <div className="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
                <span className="font-mono text-xs font-medium text-muted-foreground">
                    {language}
                </span>
                <Button
                    type="button"
                    variant="ghost"
                    size="xs"
                    onClick={copy}
                    aria-label={tf("content.copy_code_aria", { language })}
                >
                    {copied ? (
                        <CheckIcon data-icon="inline-start" />
                    ) : (
                        <CopyIcon data-icon="inline-start" />
                    )}
                    {copied ? t("common.copied") : t("common.copy")}
                </Button>
            </div>
            <pre
                className={cn(
                    "overflow-x-auto p-3 text-sm leading-6",
                    collapsible &&
                        !expanded &&
                        "max-h-[11rem] overflow-y-hidden",
                )}
            >
                <code
                    className={`language-${language}`}
                    // Prism produces escaped token markup from this source string;
                    // raw HTML from Markdown is never passed to this component.
                    dangerouslySetInnerHTML={{ __html: highlighted }}
                />
            </pre>
            {collapsible ? (
                <div className="border-t border-border px-3 py-2">
                    <Button
                        type="button"
                        variant="ghost"
                        size="xs"
                        onClick={() => setExpanded((current) => !current)}
                        aria-expanded={expanded}
                    >
                        {expanded ? (
                            <ChevronUpIcon data-icon="inline-start" />
                        ) : (
                            <ChevronDownIcon data-icon="inline-start" />
                        )}
                        {expanded
                            ? t("content.collapse_code")
                            : t("content.expand_code")}
                    </Button>
                </div>
            ) : null}
        </div>
    );
}

/**
 * Safe card Markdown: GFM is enabled, while raw HTML is deliberately disabled.
 * Links are normalized at the rendering boundary before they reach the DOM.
 */
export function MarkdownContent({ content, className }: MarkdownContentProps) {
    return (
        <div
            className={cn(
                "min-w-0 break-words text-base leading-7 [&_blockquote]:my-3 [&_blockquote]:border-l-2 [&_blockquote]:border-primary/50 [&_blockquote]:pl-3 [&_blockquote]:text-muted-foreground [&_code:not(pre_code)]:rounded [&_code:not(pre_code)]:bg-muted [&_code:not(pre_code)]:px-1 [&_code:not(pre_code)]:py-0.5 [&_h1]:mt-5 [&_h1]:text-xl [&_h1]:font-bold [&_h2]:mt-4 [&_h2]:text-lg [&_h2]:font-bold [&_h3]:mt-3 [&_h3]:font-semibold [&_li]:ml-5 [&_ol]:my-3 [&_ol]:list-decimal [&_p]:my-3 [&_p:first-child]:mt-0 [&_p:last-child]:mb-0 [&_ul]:my-3 [&_ul]:list-disc",
                className,
            )}
        >
            <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                urlTransform={safeMarkdownUrl}
                components={{
                    a({ href, children, ...props }) {
                        return (
                            <a
                                {...props}
                                href={safeMarkdownUrl(href ?? "")}
                                target="_blank"
                                rel="noopener noreferrer"
                                className="text-primary underline underline-offset-3 hover:text-foreground"
                            >
                                {children}
                            </a>
                        );
                    },
                    code({ className: codeClassName, children, ...props }) {
                        const match = /language-([^\s]+)/.exec(
                            codeClassName ?? "",
                        );
                        if (match === null) {
                            return (
                                <code
                                    {...props}
                                    className={cn(
                                        "rounded bg-muted px-1 py-0.5 font-mono text-[0.9em]",
                                        codeClassName,
                                    )}
                                >
                                    {children}
                                </code>
                            );
                        }
                        const code = String(children).replace(/\n$/, "");
                        return (
                            <CodeFence
                                code={code}
                                language={prismLanguage(match[1])}
                            />
                        );
                    },
                }}
            >
                {content}
            </ReactMarkdown>
        </div>
    );
}
