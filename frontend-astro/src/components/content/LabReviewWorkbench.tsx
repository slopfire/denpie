import { useEffect, useMemo, useState } from "react";
import { DownloadIcon, EyeIcon, EyeOffIcon, UploadIcon } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { TooltipProvider } from "@/components/ui/tooltip";
import { CardBodies } from "@/components/flow/Flow";
import {
    LAB_REVIEW_DIMENSIONS,
    pairedReviewRows,
    parseLabReviewFile,
    promptArtifactToFlowCard,
    reviewStorageKey,
    type LabReviewDimension,
    type LabReviewFile,
    type LabReviewJudgment,
    type LabReviewPayload,
    type LabReviewRow,
    type LabReviewVerdict,
} from "@/lib/lab-review";
import { t, tf } from "@/lib/i18n";

interface LabReviewWorkbenchProps {
    payload: LabReviewPayload;
}

type JudgmentMap = Readonly<Record<string, LabReviewJudgment>>;

function judgmentMap(file: LabReviewFile): JudgmentMap {
    return Object.fromEntries(
        file.judgments.map((judgment) => [judgment.key, judgment]),
    );
}

function fileFromJudgments(
    payload: LabReviewPayload,
    judgments: JudgmentMap,
): LabReviewFile {
    return {
        version: 1,
        baselineSource: payload.baseline.source,
        candidateSource: payload.candidate.source,
        updatedAt: new Date().toISOString(),
        judgments: Object.values(judgments).sort((left, right) =>
            left.key.localeCompare(right.key),
        ),
    };
}

function shouldSwap(key: string): boolean {
    let hash = 0;
    for (const character of key)
        hash = (hash * 31 + character.charCodeAt(0)) | 0;
    return (hash & 1) === 1;
}

function dimensionLabel(dimension: LabReviewDimension): string {
    return t(`lab.review.dimension.${dimension}`);
}

function metricText(value: string | number | boolean | null): string {
    if (value === null) return "none";
    return String(value);
}

function ReviewArtifact({ row, id }: { row: LabReviewRow | null; id: bigint }) {
    if (row === null) {
        return (
            <p className="p-6 text-sm text-muted-foreground">
                {t("lab.review.no_match")}
            </p>
        );
    }
    if (row.kind === "image") {
        return row.artifact === null ? (
            <p className="p-6 text-sm text-muted-foreground">
                {t("lab.review.image_missing")}
            </p>
        ) : (
            <img
                src={row.artifact}
                alt={tf("lab.review.image_alt", { case: row.caseId })}
                className="aspect-video w-full rounded-md border border-border bg-muted object-contain"
            />
        );
    }
    if (row.artifact === null) {
        return (
            <p className="p-6 text-sm text-muted-foreground">
                {t("lab.review.generated_missing")}
            </p>
        );
    }
    return (
        <Card className="flex flex-col gap-0 overflow-hidden rounded-xl py-0">
            <CardBodies card={promptArtifactToFlowCard(row.artifact, id)} />
        </Card>
    );
}

function Metrics({ row }: { row: LabReviewRow | null }) {
    if (row === null) return null;
    return (
        <details className="mt-3 text-xs text-muted-foreground">
            <summary className="cursor-pointer">
                {t("lab.review.metrics")}
            </summary>
            <dl className="mt-2 grid grid-cols-[minmax(0,1fr)_auto] gap-x-4 gap-y-1">
                {Object.entries(row.metrics).map(([key, value]) => (
                    <div key={key} className="contents">
                        <dt>{key.replaceAll("_", " ")}</dt>
                        <dd className="font-mono text-foreground">
                            {metricText(value)}
                        </dd>
                    </div>
                ))}
            </dl>
        </details>
    );
}

function VerdictButtons({
    dimension,
    value,
    swapped,
    onChange,
}: {
    dimension: LabReviewDimension;
    value: LabReviewVerdict | undefined;
    swapped: boolean;
    onChange: (verdict: LabReviewVerdict) => void;
}) {
    const left: LabReviewVerdict = swapped ? "candidate" : "baseline";
    const right: LabReviewVerdict = swapped ? "baseline" : "candidate";
    return (
        <div className="grid grid-cols-[minmax(7rem,1fr)_auto_auto_auto] items-center gap-2">
            <span className="text-sm capitalize">
                {dimensionLabel(dimension)}
            </span>
            <Button
                type="button"
                size="xs"
                variant={value === left ? "default" : "outline"}
                aria-pressed={value === left}
                data-testid={`lab-verdict-${dimension}-a`}
                onClick={() => onChange(left)}
            >
                A
            </Button>
            <Button
                type="button"
                size="xs"
                variant={value === "tie" ? "default" : "outline"}
                aria-pressed={value === "tie"}
                data-testid={`lab-verdict-${dimension}-tie`}
                onClick={() => onChange("tie")}
            >
                {t("lab.review.tie")}
            </Button>
            <Button
                type="button"
                size="xs"
                variant={value === right ? "default" : "outline"}
                aria-pressed={value === right}
                data-testid={`lab-verdict-${dimension}-b`}
                onClick={() => onChange(right)}
            >
                B
            </Button>
        </div>
    );
}

export function LabReviewWorkbench({ payload }: LabReviewWorkbenchProps) {
    const pairs = useMemo(() => pairedReviewRows(payload), [payload]);
    const [selectedKey, setSelectedKey] = useState(pairs[0]?.key ?? "");
    const [blind, setBlind] = useState(true);
    const [judgments, setJudgments] = useState<JudgmentMap>({});
    const selected = pairs.find((pair) => pair.key === selectedKey) ?? pairs[0];
    const storageKey = reviewStorageKey(payload);

    useEffect(() => {
        const stored = window.localStorage.getItem(storageKey);
        if (stored === null) return;
        try {
            setJudgments(judgmentMap(parseLabReviewFile(JSON.parse(stored))));
        } catch {
            window.localStorage.removeItem(storageKey);
        }
    }, [storageKey]);

    useEffect(() => {
        const file = fileFromJudgments(payload, judgments);
        window.localStorage.setItem(storageKey, JSON.stringify(file));
    }, [judgments, payload, storageKey]);

    const swapped =
        selected === undefined ? false : blind && shouldSwap(selected.key);
    const left =
        selected === undefined
            ? null
            : swapped
              ? selected.candidate
              : selected.baseline;
    const right =
        selected === undefined
            ? null
            : swapped
              ? selected.baseline
              : selected.candidate;
    const judgment =
        selected === undefined ? undefined : judgments[selected.key];

    function updateJudgment(
        key: string,
        update: (current: LabReviewJudgment) => LabReviewJudgment,
    ) {
        setJudgments((current) => {
            const existing = current[key] ?? { key, dimensions: {}, note: "" };
            return { ...current, [key]: update(existing) };
        });
    }

    function setVerdict(
        dimension: LabReviewDimension,
        verdict: LabReviewVerdict,
    ) {
        if (selected === undefined) return;
        updateJudgment(selected.key, (current) => ({
            ...current,
            dimensions: { ...current.dimensions, [dimension]: verdict },
        }));
    }

    useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            if (
                event.target instanceof HTMLInputElement ||
                event.target instanceof HTMLTextAreaElement
            ) {
                return;
            }
            const visible =
                event.key === "1"
                    ? "left"
                    : event.key === "2"
                      ? "tie"
                      : event.key === "3"
                        ? "right"
                        : null;
            if (visible === null || selected === undefined) return;
            event.preventDefault();
            const verdict =
                visible === "tie"
                    ? "tie"
                    : visible === "left"
                      ? swapped
                          ? "candidate"
                          : "baseline"
                      : swapped
                        ? "baseline"
                        : "candidate";
            setVerdict("overall", verdict);
        };
        window.addEventListener("keydown", onKeyDown);
        return () => window.removeEventListener("keydown", onKeyDown);
    });

    function exportReview() {
        const file = fileFromJudgments(payload, judgments);
        const url = URL.createObjectURL(
            new Blob([JSON.stringify(file, null, 2)], {
                type: "application/json",
            }),
        );
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = "review.json";
        anchor.click();
        URL.revokeObjectURL(url);
    }

    async function importReview(file: File) {
        const parsed = parseLabReviewFile(JSON.parse(await file.text()));
        if (
            parsed.baselineSource !== payload.baseline.source ||
            parsed.candidateSource !== payload.candidate.source
        ) {
            throw new Error("review file belongs to different runs");
        }
        setJudgments(judgmentMap(parsed));
    }

    const reviewed = Object.values(judgments).filter(
        (item) => item.dimensions.overall !== undefined,
    ).length;

    return (
        <TooltipProvider>
            <div className="mx-auto flex w-full max-w-[1600px] flex-col gap-5 px-4 py-6">
                <div className="flex flex-wrap items-center gap-3 rounded-xl border border-border bg-card p-3">
                    <label className="flex min-w-64 flex-1 flex-col gap-1 text-sm">
                        <span className="text-xs text-muted-foreground">
                            {t("lab.review.case")}
                        </span>
                        <select
                            value={selected?.key ?? ""}
                            onChange={(event) =>
                                setSelectedKey(event.currentTarget.value)
                            }
                            className="h-9 rounded-md border border-input bg-background px-3"
                        >
                            {pairs.map((pair) => (
                                <option key={pair.key} value={pair.key}>
                                    {pair.key}
                                </option>
                            ))}
                        </select>
                    </label>
                    <span className="text-sm text-muted-foreground">
                        {tf("lab.review.judged", {
                            reviewed,
                            total: pairs.length,
                        })}
                    </span>
                    <Button
                        type="button"
                        variant="outline"
                        onClick={() => setBlind((value) => !value)}
                    >
                        {blind ? <EyeOffIcon /> : <EyeIcon />}
                        {blind ? t("lab.review.reveal") : t("lab.review.blind")}
                    </Button>
                    <Button
                        type="button"
                        variant="outline"
                        onClick={exportReview}
                    >
                        <DownloadIcon /> {t("lab.review.export")}
                    </Button>
                    <Button type="button" variant="outline" render={<label />}>
                        <UploadIcon /> {t("lab.review.import")}
                        <input
                            type="file"
                            accept="application/json"
                            className="sr-only"
                            onChange={(event) => {
                                const file = event.currentTarget.files?.[0];
                                if (file !== undefined) void importReview(file);
                            }}
                        />
                    </Button>
                </div>

                {selected === undefined ? (
                    <p className="rounded-xl border border-border p-8 text-center text-muted-foreground">
                        {t("lab.review.empty")}
                    </p>
                ) : (
                    <>
                        <div className="rounded-xl border border-border bg-muted/30 p-4 text-sm">
                            <span className="font-medium">
                                {t("lab.review.rubric")}
                            </span>{" "}
                            {selected.baseline?.expected ||
                                selected.candidate?.expected ||
                                t("lab.review.no_expectation")}
                        </div>
                        <div className="grid gap-5 lg:grid-cols-2">
                            {[
                                {
                                    label: blind
                                        ? "A"
                                        : swapped
                                          ? payload.candidate.label
                                          : payload.baseline.label,
                                    row: left,
                                    id: 9000001n,
                                },
                                {
                                    label: blind
                                        ? "B"
                                        : swapped
                                          ? payload.baseline.label
                                          : payload.candidate.label,
                                    row: right,
                                    id: 9000002n,
                                },
                            ].map((side) => (
                                <section
                                    key={side.label}
                                    className="min-w-0 rounded-xl border border-border bg-card p-4"
                                >
                                    <h2 className="mb-3 text-lg font-semibold">
                                        {side.label}
                                    </h2>
                                    <ReviewArtifact
                                        row={side.row}
                                        id={side.id}
                                    />
                                    <Metrics row={side.row} />
                                </section>
                            ))}
                        </div>
                        <Card>
                            <CardHeader>
                                <CardTitle>
                                    {t("lab.review.judgment")}
                                </CardTitle>
                            </CardHeader>
                            <CardContent className="grid gap-4 md:grid-cols-2">
                                <div className="space-y-2">
                                    {LAB_REVIEW_DIMENSIONS.map((dimension) => (
                                        <VerdictButtons
                                            key={dimension}
                                            dimension={dimension}
                                            value={
                                                judgment?.dimensions[dimension]
                                            }
                                            swapped={swapped}
                                            onChange={(verdict) =>
                                                setVerdict(dimension, verdict)
                                            }
                                        />
                                    ))}
                                    <p className="text-xs text-muted-foreground">
                                        {t("lab.review.keyboard")}
                                    </p>
                                </div>
                                <Textarea
                                    value={judgment?.note ?? ""}
                                    placeholder={t(
                                        "lab.review.note_placeholder",
                                    )}
                                    className="min-h-40"
                                    data-testid="lab-review-note"
                                    onChange={(event) => {
                                        const note = event.currentTarget.value;
                                        updateJudgment(
                                            selected.key,
                                            (current) => ({ ...current, note }),
                                        );
                                    }}
                                />
                            </CardContent>
                        </Card>
                    </>
                )}
            </div>
        </TooltipProvider>
    );
}
