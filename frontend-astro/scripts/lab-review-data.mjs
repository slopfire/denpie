import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

function object(value, context) {
    if (value === null || typeof value !== "object" || Array.isArray(value)) {
        throw new Error(`${context} must be a JSON object`);
    }
    return value;
}

function rowsFromScorecard(value, path) {
    if (Array.isArray(value)) return value;
    const envelope = object(value, `scorecard ${path}`);
    if (!Array.isArray(envelope.rows)) {
        throw new Error(
            `scorecard ${path} must be an array or contain a rows array`,
        );
    }
    return envelope.rows;
}

function optionalObject(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value)
        ? value
        : null;
}

function string(value, key, fallback = "") {
    const candidate = optionalObject(value)?.[key];
    return typeof candidate === "string" ? candidate : fallback;
}

function scalarMetrics(row, excluded) {
    const metrics = {};
    for (const [key, value] of Object.entries(row)) {
        if (excluded.has(key)) continue;
        if (
            value === null ||
            typeof value === "string" ||
            typeof value === "number" ||
            typeof value === "boolean"
        ) {
            metrics[key] = value;
        }
    }
    return metrics;
}

function directInputPath(input, root) {
    const absolute = isAbsolute(input) ? input : resolve(root, input);
    return existsSync(absolute) ? absolute : null;
}

function runDirectories(root) {
    const runsDir = join(root, "lab", "runs");
    if (!existsSync(runsDir)) return [];
    return readdirSync(runsDir, { withFileTypes: true })
        .filter((entry) => entry.isDirectory())
        .map((entry) => join(runsDir, entry.name))
        .filter((path) => existsSync(join(path, "manifest.json")))
        .sort()
        .reverse();
}

function manifestLabel(path) {
    try {
        const manifest = object(
            JSON.parse(readFileSync(join(path, "manifest.json"), "utf8")),
            `manifest ${path}`,
        );
        return string(manifest, "label");
    } catch {
        return "";
    }
}

export function resolveLabReviewInput(input, root = repoRoot) {
    const direct = directInputPath(input, root);
    if (direct !== null) return direct;

    const baselinesPath = join(root, "lab", "runs", "baselines.json");
    if (existsSync(baselinesPath)) {
        const baselines = object(
            JSON.parse(readFileSync(baselinesPath, "utf8")),
            `baselines ${baselinesPath}`,
        );
        const selected = baselines[input];
        if (typeof selected === "string") {
            const baseline = directInputPath(selected, root);
            if (baseline !== null) return baseline;
        }
    }

    const runs = runDirectories(root);
    const selected =
        input === "latest"
            ? runs[0]
            : runs.find(
                  (path) =>
                      path.split("/").at(-1) === input ||
                      manifestLabel(path) === input,
              );
    if (selected !== undefined) return selected;
    throw new Error(`no lab run matches ${input}`);
}

function resolveRunPath(input) {
    const absolute = resolveLabReviewInput(input);
    const stat = statSync(absolute);
    return stat.isDirectory()
        ? { runDir: absolute, scorecardPath: join(absolute, "scorecard.json") }
        : { runDir: dirname(absolute), scorecardPath: absolute };
}

function readManifest(runDir) {
    const path = join(runDir, "manifest.json");
    if (!existsSync(path)) return null;
    return object(JSON.parse(readFileSync(path, "utf8")), `manifest ${path}`);
}

function repeatIndex(row) {
    return typeof row.repeat_index === "number" ? row.repeat_index : null;
}

function readPromptCard(runDir, caseId, row) {
    const repeat = repeatIndex(row);
    const repeatedPath =
        repeat === null
            ? null
            : join(runDir, "cases", `${caseId}-${repeat}.card.json`);
    const path =
        repeatedPath !== null && existsSync(repeatedPath)
            ? repeatedPath
            : join(runDir, "cases", `${caseId}.card.json`);
    if (!existsSync(path)) return null;
    const card = object(JSON.parse(readFileSync(path, "utf8")), `card ${path}`);
    return {
        title: string(card, "title"),
        fullContent: string(card, "full_content"),
        compressedContent: string(card, "compressed_content"),
        useImage: card.use_image === true,
        imageQuery: string(card, "image_query"),
    };
}

function imageDataUrl(runDir, row) {
    const extension = string(row, "extension");
    const strategy = string(row, "strategy");
    const caseId = row.case_id;
    if (extension === "" || strategy === "" || typeof caseId !== "number")
        return null;
    const repeat = repeatIndex(row);
    const repeatedPath =
        repeat === null
            ? null
            : join(
                  runDir,
                  "cases",
                  String(caseId),
                  `${strategy}-${repeat}.${extension}`,
              );
    const path =
        repeatedPath !== null && existsSync(repeatedPath)
            ? repeatedPath
            : join(runDir, "cases", String(caseId), `${strategy}.${extension}`);
    if (!existsSync(path)) return null;
    const declared = string(row, "mime_type");
    const inferred =
        extname(path) === ".png"
            ? "image/png"
            : extname(path) === ".webp"
              ? "image/webp"
              : "image/jpeg";
    return `data:${declared || inferred};base64,${readFileSync(path).toString("base64")}`;
}

function normalizeRun(input, side) {
    const { runDir, scorecardPath } = resolveRunPath(input);
    const rows = rowsFromScorecard(
        JSON.parse(readFileSync(scorecardPath, "utf8")),
        scorecardPath,
    );
    const manifest = readManifest(runDir);
    const manifestBench = string(manifest, "bench");
    const first = optionalObject(rows[0]);
    const bench =
        manifestBench === "images" || manifestBench === "prompts"
            ? manifestBench
            : first !== null && typeof first.strategy === "string"
              ? "images"
              : "prompts";
    const label =
        string(manifest, "label") ||
        runDir.split("/").filter(Boolean).at(-1) ||
        side;

    const normalizedRows = rows.map((raw, index) => {
        const row = object(raw, `${scorecardPath} row ${index}`);
        if (bench === "images") {
            if (typeof row.case_id !== "number") {
                throw new Error(
                    `${scorecardPath} row ${index} has no numeric case_id`,
                );
            }
            const strategy = string(row, "strategy");
            const repeat = repeatIndex(row);
            return {
                kind: "image",
                key:
                    repeat === null
                        ? `${row.case_id}/${strategy}`
                        : `${row.case_id}/${strategy}/${repeat}`,
                caseId: String(row.case_id),
                strategy,
                expected: string(row, "expected"),
                artifact: imageDataUrl(runDir, row),
                metrics: scalarMetrics(
                    row,
                    new Set(["case_id", "strategy", "expected", "visual"]),
                ),
            };
        }
        const caseId = string(row, "case_id");
        if (caseId === "") {
            throw new Error(
                `${scorecardPath} row ${index} has no string case_id`,
            );
        }
        const repeat = repeatIndex(row);
        return {
            kind: "prompt",
            key: repeat === null ? caseId : `${caseId}/${repeat}`,
            caseId,
            expected: string(row, "expected"),
            artifact: readPromptCard(runDir, caseId, row),
            metrics: scalarMetrics(
                row,
                new Set(["case_id", "expected", "visual", "error"]),
            ),
        };
    });

    return {
        side,
        label,
        bench,
        source: input,
        manifest,
        rows: normalizedRows,
    };
}

export function loadLabReviewPayload({ baseline, candidate }) {
    if (!baseline || !candidate) return null;
    return {
        version: 1,
        baseline: normalizeRun(baseline, "baseline"),
        candidate: normalizeRun(candidate, "candidate"),
    };
}
