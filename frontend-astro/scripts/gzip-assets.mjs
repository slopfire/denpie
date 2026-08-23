// Precompresses build output so the Axum server can serve .gz files directly
// (tower-http ServeDir precompressed_gzip picks up `<name>.gz` siblings
// without on-the-fly compression).
//
// Walks dist/_astro/, and for every file larger than 1 KiB that is not
// already in a compressed container format writes a sibling `<name>.gz`
// (level 9). Skips work when a .gz sibling is newer than its source.
import { gzipSync } from "node:zlib";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// Anchored to this script's location (<root>/scripts/) so the step works
// regardless of the invoking process's working directory.
const DIST_ASTRO = fileURLToPath(new URL("../dist/_astro", import.meta.url));


const MIN_BYTES = 1024;
const GZIP_LEVEL = 9;
// Fonts (woff2) and images (png/jpg/webp) are already compressed containers:
// re-gzipping costs CPU for ~0% gain.
const SKIP_EXT = new Set([".woff2", ".png", ".jpg", ".jpeg", ".webp"]);

function walk(dir, out = []) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) {
            walk(path, out);
        } else if (entry.isFile()) {
            out.push(path);
        }
    }
    return out;
}

let written = 0;
for (const path of walk(DIST_ASTRO)) {
    if (path.endsWith(".gz")) continue;
    if (SKIP_EXT.has(path.slice(path.lastIndexOf(".")))) continue;
    const { size, mtimeMs } = statSync(path);
    if (size <= MIN_BYTES) continue;

    const gzPath = `${path}.gz`;
    try {
        if (statSync(gzPath).mtimeMs >= mtimeMs) continue;
    } catch {
        // No existing .gz: compress.
    }

    writeFileSync(gzPath, gzipSync(readFileSync(path), { level: GZIP_LEVEL }));
    written += 1;
}
console.log(`gzip-assets: wrote ${written} file(s)`);
