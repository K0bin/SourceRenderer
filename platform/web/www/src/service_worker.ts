const CACHE_NAME = "cache";

self.addEventListener('install', event => {
    (event as ExtendableEvent).waitUntil(
        (async () => {
            const cache = await caches.open(CACHE_NAME);
            await cache.addAll([
                '/',
                'index.html',
                "manifest.json",
                'index.js',
                'game_worker.js',
                'asset_worker.js',
                'wasm_lib/sourcerenderer_web.js',
                'wasm_lib/sourcerenderer_web_bg.wasm',
            ]);
        })()
    );
});

self.addEventListener('fetch', event => {
    const fetchEvent = event as FetchEvent;
    fetchEvent.respondWith(
        (async () => {
            const cache = await caches.open(CACHE_NAME);
            const cachedResponse = await cache.match(fetchEvent.request);
            if (cachedResponse) {
                return cachedResponse;
            }
            const response = await fetch(fetchEvent.request);
            return response;
        })()
    );
});
