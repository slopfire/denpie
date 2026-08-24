import { create } from "@bufbuild/protobuf";
import { FlowCardInfoSchema, type FlowCardInfo } from "@/generated/denpie_pb";

export const LAB_REVIEW_DIMENSIONS = [
    "overall",
    "correctness",
    "learnability",
    "compression",
    "image_relevance",
    "ui_fit",
] as const;

export type LabReviewDimension = (typeof LAB_REVIEW_DIMENSIONS)[number];
export type LabReviewVerdict = "baseline" | "tie" | "candidate";
export type LabReviewBench = "images" | "prompts";
export type LabReviewScalar = string | number | boolean | null;

export interface LabReviewPromptArtifact {
    readonly title: string;
    readonly fullContent: string;
    readonly compressedContent: string;
    readonly useImage: boolean;
    readonly imageQuery: string;
}

interface LabReviewRowBase {
    readonly key: string;
    readonly caseId: string;
    readonly expected: string;
    readonly metrics: Readonly<Record<string, LabReviewScalar>>;
}

export interface LabReviewPromptRow extends LabReviewRowBase {
    readonly kind: "prompt";
    readonly artifact: LabReviewPromptArtifact | null;
}

export interface LabReviewImageRow extends LabReviewRowBase {
    readonly kind: "image";
    readonly strategy: string;
    readonly artifact: string | null;
}

export type LabReviewRow = LabReviewPromptRow | LabReviewImageRow;

export interface LabReviewRun {
    readonly side: "baseline" | "candidate";
    readonly label: string;
    readonly bench: LabReviewBench;
    readonly source: string;
    readonly manifest: Readonly<Record<string, unknown>> | null;
    readonly rows: readonly LabReviewRow[];
}

export interface LabReviewPayload {
    readonly version: 1;
    readonly baseline: LabReviewRun;
    readonly candidate: LabReviewRun;
}

export interface LabReviewJudgment {
    readonly key: string;
    readonly dimensions: Partial<
        Readonly<Record<LabReviewDimension, LabReviewVerdict>>
    >;
    readonly note: string;
}

export interface LabReviewFile {
    readonly version: 1;
    readonly baselineSource: string;
    readonly candidateSource: string;
    readonly updatedAt: string;
    readonly judgments: readonly LabReviewJudgment[];
}

function isObject(value: unknown): value is Record<string, unknown> {
    return value !== null && typeof value === "object" && !Array.isArray(value);
}

function stringField(value: Record<string, unknown>, key: string): string {
    const field = value[key];
    if (typeof field !== "string") throw new Error(`missing string ${key}`);
    return field;
}

function nullableStringField(
    value: Record<string, unknown>,
    key: string,
): string | null {
    const field = value[key];
    if (field === null) return null;
    if (typeof field !== "string") throw new Error(`invalid ${key}`);
    return field;
}

function parseMetrics(
    value: unknown,
): Readonly<Record<string, LabReviewScalar>> {
    if (!isObject(value)) throw new Error("metrics must be an object");
    const parsed: Record<string, LabReviewScalar> = {};
    for (const [key, metric] of Object.entries(value)) {
        if (
            metric !== null &&
            typeof metric !== "string" &&
            typeof metric !== "number" &&
            typeof metric !== "boolean"
        ) {
            throw new Error(`invalid metric ${key}`);
        }
        parsed[key] = metric;
    }
    return parsed;
}

function parsePromptArtifact(value: unknown): LabReviewPromptArtifact | null {
    if (value === null) return null;
    if (!isObject(value)) throw new Error("prompt artifact must be an object");
    const useImage = value.useImage;
    if (typeof useImage !== "boolean") throw new Error("invalid useImage");
    return {
        title: stringField(value, "title"),
        fullContent: stringField(value, "fullContent"),
        compressedContent: stringField(value, "compressedContent"),
        useImage,
        imageQuery: stringField(value, "imageQuery"),
    };
}

function parseRow(value: unknown): LabReviewRow {
    if (!isObject(value)) throw new Error("review row must be an object");
    const common = {
        key: stringField(value, "key"),
        caseId: stringField(value, "caseId"),
        expected: stringField(value, "expected"),
        metrics: parseMetrics(value.metrics),
    };
    if (value.kind === "prompt") {
        return {
            kind: "prompt",
            ...common,
            artifact: parsePromptArtifact(value.artifact),
        };
    }
    if (value.kind === "image") {
        return {
            kind: "image",
            ...common,
            strategy: stringField(value, "strategy"),
            artifact: nullableStringField(value, "artifact"),
        };
    }
    throw new Error("review row has an unknown kind");
}

function parseRun(value: unknown): LabReviewRun {
    if (!isObject(value) || !Array.isArray(value.rows)) {
        throw new Error("review run must contain rows");
    }
    if (value.side !== "baseline" && value.side !== "candidate") {
        throw new Error("invalid review side");
    }
    if (value.bench !== "images" && value.bench !== "prompts") {
        throw new Error("invalid review bench");
    }
    if (value.manifest !== null && !isObject(value.manifest)) {
        throw new Error("invalid review manifest");
    }
    return {
        side: value.side,
        bench: value.bench,
        label: stringField(value, "label"),
        source: stringField(value, "source"),
        manifest: value.manifest,
        rows: value.rows.map(parseRow),
    };
}

export function parseLabReviewPayload(value: unknown): LabReviewPayload {
    if (!isObject(value) || value.version !== 1) {
        throw new Error("unsupported lab review payload");
    }
    const baseline = parseRun(value.baseline);
    const candidate = parseRun(value.candidate);
    if (baseline.side !== "baseline" || candidate.side !== "candidate") {
        throw new Error("lab review sides are reversed");
    }
    if (baseline.bench !== candidate.bench) {
        throw new Error("cannot review different bench types");
    }
    return { version: 1, baseline, candidate };
}

export function pairedReviewRows(payload: LabReviewPayload): Array<{
    readonly key: string;
    readonly baseline: LabReviewRow | null;
    readonly candidate: LabReviewRow | null;
}> {
    const baseline = new Map(
        payload.baseline.rows.map((row) => [row.key, row]),
    );
    const candidate = new Map(
        payload.candidate.rows.map((row) => [row.key, row]),
    );
    const keys = [...new Set([...baseline.keys(), ...candidate.keys()])].sort();
    return keys.map((key) => ({
        key,
        baseline: baseline.get(key) ?? null,
        candidate: candidate.get(key) ?? null,
    }));
}

export function promptArtifactToFlowCard(
    artifact: LabReviewPromptArtifact,
    id: bigint,
): FlowCardInfo {
    return create(FlowCardInfoSchema, {
        id,
        topicName: "Lab candidate",
        topicIcon: "radix-icons:bookmark",
        title: artifact.title,
        fullContent: artifact.fullContent,
        compressedContent: artifact.compressedContent,
        tipcardType: "casual_tip",
        status: "active",
        pinned: false,
        pendingCount: 0n,
        images: [],
        sources: [],
    });
}

export function reviewStorageKey(payload: LabReviewPayload): string {
    return `denpie-lab-review:${payload.baseline.source}:${payload.candidate.source}`;
}

export function parseLabReviewFile(value: unknown): LabReviewFile {
    if (
        !isObject(value) ||
        value.version !== 1 ||
        !Array.isArray(value.judgments)
    ) {
        throw new Error("unsupported review file");
    }
    const judgments = value.judgments.map((entry) => {
        if (!isObject(entry) || !isObject(entry.dimensions)) {
            throw new Error("invalid review judgment");
        }
        const dimensions: Partial<
            Record<LabReviewDimension, LabReviewVerdict>
        > = {};
        for (const dimension of LAB_REVIEW_DIMENSIONS) {
            const verdict = entry.dimensions[dimension];
            if (
                verdict === "baseline" ||
                verdict === "tie" ||
                verdict === "candidate"
            ) {
                dimensions[dimension] = verdict;
            }
        }
        return {
            key: stringField(entry, "key"),
            note: stringField(entry, "note"),
            dimensions,
        };
    });
    return {
        version: 1,
        baselineSource: stringField(value, "baselineSource"),
        candidateSource: stringField(value, "candidateSource"),
        updatedAt: stringField(value, "updatedAt"),
        judgments,
    };
}
