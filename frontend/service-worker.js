const CACHE_PREFIX = "denpie";
const APP_CACHE = `${CACHE_PREFIX}-app-v1`;
const ASSET_CACHE = `${CACHE_PREFIX}-assets-v1`;
const CDN_HOSTS = new Set([
    "cdn.jsdelivr.net",
    "code.iconify.design",
]);

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
            .then(() => self.clients.claim()),
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
    if (url.origin === self.location.origin) {
        return (
            url.pathname.startsWith("/snippets/") ||
            /^\/frontend-[^/]+\.(?:css|js|wasm)$/.test(url.pathname) ||
            /^\/frontend-[^/]+_bg\.wasm$/.test(url.pathname)
        );
    }

    return false;
}

function isRevalidatedAsset(request, url) {
    if (url.origin === self.location.origin) {
        return url.pathname.startsWith("/static/");
    }

    return CDN_HOSTS.has(url.hostname) && ["script", "style", "font", "image"].includes(request.destination);
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
