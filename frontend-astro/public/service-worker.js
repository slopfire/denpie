const CACHE_PREFIX = "denpie";
const APP_CACHE = `${CACHE_PREFIX}-app-v1`;
const ASSET_CACHE = `${CACHE_PREFIX}-assets-v1`;
// Hashed /_astro/ entries accumulate across deploys: each deploy emits new
// content-hashed URLs, so old entries are never overwritten, only abandoned.
// Correctness never depends on this cache — HTML is network-first and every
// deploy references fresh hashes — so once the entry count exceeds this bound
// we delete the whole cache and let it repopulate naturally from the network
// rather than grow without limit on long-lived clients.
const ASSET_CACHE_MAX_ENTRIES = 250;

self.addEventListener("install", (event) => {
    event.waitUntil(
        caches.open(APP_CACHE).then((cache) => cache.add("/").catch(() => undefined)),
    );
    self.skipWaiting();
});

self.addEventListener("activate", (event) => {
    event.waitUntil(
        caches
            .keys()
            .then((keys) =>
                Promise.all(
                    keys
                        .filter((key) => key.startsWith(CACHE_PREFIX) && key !== APP_CACHE && key !== ASSET_CACHE)
                        .map((key) => caches.delete(key)),
                ),
            )
            .then(boundAssetCache)
    );
});

self.addEventListener("fetch", (event) => {
    const request = event.request;
    if (request.method !== "GET") {
        return;
    }

    const url = new URL(request.url);
    if (request.mode === "navigate") {
        event.respondWith(networkFirst(request, APP_CACHE));
        return;
    }

    if (isImmutableAsset(url)) {
        event.respondWith(cacheFirst(request, ASSET_CACHE));
        return;
    }

    if (isRevalidatedAsset(request, url)) {
        event.respondWith(staleWhileRevalidate(request, ASSET_CACHE));
    }
});

function isImmutableAsset(url) {
    return url.origin === self.location.origin && url.pathname.startsWith("/_astro/");
}

function isRevalidatedAsset(_request, url) {
    return url.origin === self.location.origin && url.pathname.startsWith("/static/");
}

async function cacheFirst(request, cacheName) {
    const cache = await caches.open(cacheName);
    const cached = await cache.match(request);
    if (cached) {
        return cached;
    }

    const response = await fetch(request);
    if (response.ok || response.type === "opaque") {
        cache.put(request, response.clone()).catch(() => undefined);
    }

    await boundAssetCache();
    return response;
}

async function staleWhileRevalidate(request, cacheName) {
    const cache = await caches.open(cacheName);
    const cached = await cache.match(request);
    const refresh = fetch(request)
        .then((response) => {
            if (response.ok || response.type === "opaque") {
                cache.put(request, response.clone()).catch(() => undefined);
            }
            return response;
        })
        .catch(() => undefined);

    return cached || refresh || fetch(request);
}

// Keeps ASSET_CACHE from growing without bound: hashed /_astro/ URLs are
// immutable, so entries are never rewritten — deploys just abandon them.
// When the count exceeds the bound, delete the cache wholesale; it refills
// from the network on subsequent requests. Best-effort only. Opens the
// cache itself so it can run from the activate chain or after a put.
async function boundAssetCache() {
    try {
        const cache = await caches.open(ASSET_CACHE);
        if ((await cache.keys()).length > ASSET_CACHE_MAX_ENTRIES) {
            await caches.delete(ASSET_CACHE);
        }
    } catch {
        // Storage errors must never break activation or a fetch response.
    }
}

async function networkFirst(request, cacheName) {
    const cache = await caches.open(cacheName);
    try {
        const response = await fetch(request);
        if (response.ok) {
            cache.put(request, response.clone()).catch(() => undefined);
        }
        return response;
    } catch (error) {
        return (await cache.match(request)) || (await cache.match("/")) || Response.error();
    }
}
