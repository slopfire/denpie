// Static output only: Astro owns the HTML document shell. There is NO SSR
// adapter, NO Astro Actions, and NO Node backend — the Axum server in `src/`
// remains the sole server and API surface (`POST /auth/login`, `GET /auth/me`,
// `POST /auth/logout`, ...). `astro dev` (used by `just dev`) is a local Vite
// HMR process only; it proxies API/session routes to Axum and is not part of
// `astro build` or production.
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// Injects `<link rel="modulepreload">` for every island entry and its
// statically reachable chunks into each emitted page (`astro:build:done`).
// Astro hydrates via the astro-island custom element, which imports its
// chunk lazily at runtime — without these links the browser discovers the
// island graph only after HTML parse and hydration begin, serializing JS
// startup behind a waterfall. dist is regenerated each build, so plain
// insertion after <head> is idempotent. Regex over build text on purpose:
// no new dependencies, and Vite's chunk layout is a stable target.
const ASTRO_DIR = "_astro";

function collectStaticChunks(distDir, entryName) {
    const queue = [entryName];
    const seen = new Set(queue);
    const order = [];
    while (queue.length > 0) {
        const name = queue.shift();
        order.push(name);
        let source = "";
        try {
            source = readFileSync(join(distDir, ASTRO_DIR, name), "utf8");
        } catch {
            // Missing/unreadable chunk: preload what we have so far.
        }
        for (const match of source.matchAll(/(?:from|import)\s*"(\.[^"]+)"/g)) {
            // Static .js/.mjs imports only: CSS is handled by Vite's own
            // link injection and dynamic import() must stay on-demand.
            if (!/\.(?:js|mjs)$/.test(match[1])) continue;
            const target = match[1].slice(2);
            if (!seen.has(target)) {
                seen.add(target);
                queue.push(target);
            }
        }
    }
    return order;
}

function injectModulePreloads(dir, pages) {
    const distDir = typeof dir === "string" ? dir : fileURLToPath(dir);
    let injected = 0;
    for (const page of pages) {
        let htmlPath = join(distDir, page.pathname);
        // Root page arrives as "" and directory-format pages as "foo/";
        // resolve either to the emitted index.html inside.
        try {
            if (statSync(htmlPath).isDirectory())
                htmlPath = join(htmlPath, "index.html");
        } catch {
            continue;
        }
        let original;
        try {
            original = readFileSync(htmlPath, "utf8");
        } catch {
            continue;
        }
        const entries = [
            ...new Set(
                [
                    ...original.matchAll(
                        /component-url="(\/_astro\/[^"]+\.js)"/g,
                    ),
                ].map((match) => match[1]),
            ),
        ];
        if (entries.length === 0) continue;
        const urls = new Set();
        for (const entry of entries) {
            for (const chunk of collectStaticChunks(
                distDir,
                entry.slice(ASTRO_DIR.length + 2),
            )) {
                urls.add(`/${ASTRO_DIR}/${chunk}`);
            }
        }
        if (urls.size === 0) continue;
        const tags = [...urls]
            .map((href) => `<link rel="modulepreload" href="${href}">`)
            .join("");
        const headIndex = original.indexOf("<head>");
        if (headIndex === -1) continue;
        const insertAt = headIndex + "<head>".length;
        writeFileSync(
            htmlPath,
            original.slice(0, insertAt) + tags + original.slice(insertAt),
        );
        injected += 1;
    }
    return injected;
}

function modulePreloadIntegration() {
    return {
        name: "denpie-module-preload",
        hooks: {
            "astro:build:done": ({ dir, pages, logger }) => {
                const count = injectModulePreloads(dir, pages);
                logger.info(
                    `modulepreload: injected island graph into ${count} page(s)`,
                );
            },
        },
    };
}

import { defineConfig } from "astro/config";
import react from "@astrojs/react";
import tailwindcss from "@tailwindcss/vite";
import { loadLabReviewPayload } from "./scripts/lab-review-data.mjs";

// Checked-in lab card fixtures, inlined at build time via Vite `define` so
// the lab page renders the exact pack under `lab/cases/cards/` without a
// network call or a copy drifting from the source of truth.
const labCardFixtures = readFileSync(
    new URL("../lab/cases/cards/repeatable-states.json", import.meta.url),
    "utf8",
);

const labReviewPayload = loadLabReviewPayload({
    baseline: process.env.DENPIE_LAB_REVIEW_BASELINE,
    candidate: process.env.DENPIE_LAB_REVIEW_CANDIDATE,
});

const axumOrigin = (() => {
    const bind = process.env.DENPIE_BIND_ADDR || "127.0.0.1:3017";
    if (/^https?:\/\//.test(bind)) {
        return bind;
    }
    return `http://${bind}`;
})();

const axumProxy = {
    "/api": axumOrigin,
    "/auth": axumOrigin,
    "/app": axumOrigin,
    "/admin": axumOrigin,
    "/static": axumOrigin,
};

export default defineConfig({
    output: "static",
    site: "http://localhost:3027",
    integrations: [react(), modulePreloadIntegration()],
    server: {
        port: 4321,
    },
    vite: {
        plugins: [tailwindcss()],
        define: {
            __LAB_CARD_FIXTURES__: JSON.stringify(labCardFixtures),
            __LAB_REVIEW_PAYLOAD__: JSON.stringify(
                JSON.stringify(labReviewPayload),
            ),
        },
        server: {
            strictPort: true,
            proxy: axumProxy,
        },
    },
});
