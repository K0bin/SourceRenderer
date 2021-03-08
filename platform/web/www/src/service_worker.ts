const CACHE_NAME = "cache";

self.addEventListener('install', event => {
    (event as ExtendableEvent).waitUntil(
        caches.open(CACHE_NAME).then(cache => {
            return cache.addAll([
                '../index.html',
                "../manifest.json",
                'index.js',
                '../wasm_lib/sourcerenderer_web.js',
                '../wasm_lib/sourcerenderer_web_bg.wasm',
            ]);
        })
    );
});

self.addEventListener('fetch', event => {
    const fetchEvent = event as FetchEvent;
    if (fetchEvent.request.method !== 'GET') {
        return;
    }
    const url = fetchEvent.request.url.indexOf(self.location.origin) !== -1
      ? fetchEvent.request.url.split(`${self.location.origin}/`)[1]
      : fetchEvent.request.url;

    fetchEvent.respondWith(
        caches.open(CACHE_NAME).then(cache => {
            return cache.match(url)
                .then(response => {
                    if (response) {
                        return response;
                    }
                    throw Error('There is no response for such request: ' + url);
                });
        })
        .catch(error => fetch(fetchEvent.request))
    );
});
