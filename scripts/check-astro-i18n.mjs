#!/usr/bin/env node

import { readdir, readFile } from "node:fs/promises";
import { dirname, extname, join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = dirname(scriptDirectory);
const sourceRoots = [
    join(repositoryRoot, "frontend-astro/src/islands"),
    join(repositoryRoot, "frontend-astro/src/components/content"),
    join(repositoryRoot, "frontend-astro/src/components/flow"),
    join(repositoryRoot, "frontend-astro/src/components/pages"),
];
const catalogPath = join(repositoryRoot, "frontend-astro/src/i18n/en.json");
const typescriptPath = join(
    repositoryRoot,
    "frontend-astro/node_modules/typescript/lib/typescript.js",
);
const ts = (await import(pathToFileURL(typescriptPath).href)).default;

const translatedAttributes = new Set([
    "alt",
    "aria-description",
    "aria-label",
    "placeholder",
    "title",
]);
const translatedProperties = new Set([
    "description",
    "label",
    "message",
    "title",
]);

function looksLikeCopy(value) {
    return /[A-Z][A-Za-z]|\s+[A-Za-z]/.test(value);
}

function literalValue(node) {
    if (
        ts.isStringLiteral(node) ||
        ts.isNoSubstitutionTemplateLiteral(node) ||
        ts.isTemplateExpression(node)
    ) {
        return node.getText();
    }
    return null;
}

function propertyName(node) {
    if (ts.isIdentifier(node) || ts.isStringLiteral(node)) return node.text;
    return null;
}

async function sourceFiles(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    const paths = [];
    for (const entry of entries) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) paths.push(...(await sourceFiles(path)));
        else if (extname(entry.name) === ".tsx") paths.push(path);
    }
    return paths;
}

function audit(sourceFile, catalog) {
    const findings = [];

    function reportFinding(node, value) {
        const { line, character } = sourceFile.getLineAndCharacterOfPosition(
            node.getStart(sourceFile),
        );
        findings.push({
            line: line + 1,
            column: character + 1,
            value: value.replace(/\s+/g, " ").slice(0, 100),
        });
    }

    function report(node, value) {
        if (looksLikeCopy(value)) reportFinding(node, value);
    }

    function visit(node) {
        if (ts.isJsxText(node)) {
            report(node, node.text.trim());
        } else if (ts.isJsxAttribute(node)) {
            const name = node.name.getText(sourceFile);
            if (
                translatedAttributes.has(name) &&
                node.initializer !== undefined
            ) {
                if (ts.isStringLiteral(node.initializer)) {
                    report(node.initializer, node.initializer.text);
                } else if (
                    ts.isJsxExpression(node.initializer) &&
                    node.initializer.expression !== undefined
                ) {
                    const value = literalValue(node.initializer.expression);
                    if (value !== null)
                        report(node.initializer.expression, value);
                }
            }
        } else if (ts.isPropertyAssignment(node)) {
            const name = propertyName(node.name);
            if (name !== null && translatedProperties.has(name)) {
                const value = literalValue(node.initializer);
                if (value !== null) report(node.initializer, value);
            }
        } else if (ts.isCallExpression(node)) {
            const callee = node.expression.getText(sourceFile);
            if (
                callee === "tf" &&
                ts.isStringLiteral(node.arguments[0]) &&
                ts.isObjectLiteralExpression(node.arguments[1])
            ) {
                const key = node.arguments[0].text;
                const message = catalog[key];
                if (typeof message === "string") {
                    const expected = [
                        ...new Set(
                            [...message.matchAll(/\{([^{}]+)\}/g)].map(
                                (match) => match[1],
                            ),
                        ),
                    ].sort();
                    const actual = node.arguments[1].properties
                        .map((property) =>
                            ts.isPropertyAssignment(property) ||
                            ts.isShorthandPropertyAssignment(property)
                                ? propertyName(property.name)
                                : null,
                        )
                        .filter((name) => name !== null)
                        .sort();
                    if (expected.join("\0") !== actual.join("\0")) {
                        reportFinding(
                            node,
                            `tf(${JSON.stringify(key)}) expects {${expected.join(", ")}} but receives {${actual.join(", ")}}`,
                        );
                    }
                }
            }
            if (
                (callee === "setError" || callee.endsWith(".confirm")) &&
                node.arguments[0] !== undefined
            ) {
                const value = literalValue(node.arguments[0]);
                if (value !== null) report(node.arguments[0], value);
            }
        }
        ts.forEachChild(node, visit);
    }

    visit(sourceFile);
    return findings;
}

const catalog = JSON.parse(await readFile(catalogPath, "utf8"));
const paths = (await Promise.all(sourceRoots.map(sourceFiles))).flat().sort();
let count = 0;
for (const path of paths) {
    const source = await readFile(path, "utf8");
    const sourceFile = ts.createSourceFile(
        path,
        source,
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.TSX,
    );
    for (const finding of audit(sourceFile, catalog)) {
        count += 1;
        console.error(
            `${relative(repositoryRoot, path)}:${finding.line}:${finding.column}: untranslated UI copy ${finding.value}`,
        );
    }
}

if (count > 0) {
    console.error(`Astro i18n check found ${count} untranslated UI literals.`);
    process.exitCode = 1;
} else {
    console.log(`Astro i18n check passed (${paths.length} TSX files).`);
}
