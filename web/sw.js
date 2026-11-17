// Funzionamento offline: i file dell'app restano in cache e vengono aggiornati in background
// (prima la cache, poi la rete). I dati dell'utente stanno in IndexedDB, non qui.
const CACHE = "tremendouspreparator-v1";
const FILES = ["./", "index.html", "prepapp.js", "prepapp_bg.wasm", "manifest.webmanifest", "icon-128.png", "icon-256.png"];

self.addEventListener("install", (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(FILES)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (e) => {
  if (e.request.method !== "GET") return;
  e.respondWith(
    caches.open(CACHE).then(async (cache) => {
      const cached = await cache.match(e.request);
      const fresh = fetch(e.request).then((r) => {
        if (r.ok) cache.put(e.request, r.clone());
        return r;
      }).catch(() => cached);
      return cached || fresh;
    }),
  );
});
