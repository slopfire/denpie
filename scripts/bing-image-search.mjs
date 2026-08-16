import { chromium } from "playwright";

const NAVIGATION_TIMEOUT_MS = 25_000;
const DOM_SETTLE_WAIT_MS = 1_500;
const MAX_CANDIDATES = 32;

function searchUrl(query) {
  const url = new URL("https://www.bing.com/images/search");
  url.searchParams.set("q", query.trim());
  url.searchParams.set("form", "HDRSC2");
  return url.toString();
}

function murlsFromHtml(html) {
  const results = [];
  const seen = new Set();
  // Bing embeds result metadata as JSON in the page. Decode JSON string
  // escapes rather than trying to interpret the URL in JavaScript directly.
  const murlPatterns = [
    /["']murl["']\s*:\s*"(https?:[^"]*)"/g,
    /murl(?:&quot;|")\s*:\s*(?:&quot;|")(https?:.*?)&quot;/g,
  ];
  for (const [patternIndex, murlPattern] of murlPatterns.entries()) {
    for (const match of html.matchAll(murlPattern)) {
      let candidate;
      try {
        candidate = patternIndex === 0
          ? JSON.parse(`"${match[1]}"`)
          : match[1]
              .replaceAll("&amp;", "&")
              .replaceAll("&#x2F;", "/")
              .replaceAll("&#47;", "/");
      } catch {
        continue;
      }
      try {
        const parsed = new URL(candidate);
        if (!/^https?:$/.test(parsed.protocol) || seen.has(parsed.href)) continue;
        seen.add(parsed.href);
        results.push(parsed.href);
        if (results.length >= MAX_CANDIDATES) break;
      } catch {
        // Ignore malformed metadata; another result may still be usable.
      }
    }
    if (results.length >= MAX_CANDIDATES) break;
  }
  return results;
}

async function discover(query) {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({
      userAgent:
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 " +
        "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    });

    // This sidecar discovers source URLs only. It must never download the
    // candidate image bytes; the Rust downloader performs that separate pass.
    await page.route("**/*", (route) => {
      const resourceType = route.request().resourceType();
      if (["image", "media", "font"].includes(resourceType)) {
        return route.abort();
      }
      return route.continue();
    });

    await page.goto(searchUrl(query), {
      waitUntil: "domcontentloaded",
      timeout: NAVIGATION_TIMEOUT_MS,
    });
    await page.waitForTimeout(DOM_SETTLE_WAIT_MS);
    return murlsFromHtml(await page.content());
  } finally {
    await browser.close();
  }
}

const query = process.argv[2]?.trim();
if (!query) {
  console.error("Bing image search requires a non-empty query");
  process.exitCode = 2;
} else {
  try {
    process.stdout.write(`${JSON.stringify(await discover(query))}\n`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
