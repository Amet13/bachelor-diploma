// Offline cache for the proofback app shell and engine. Only same-origin GET
// requests for the app's own files are cached; no user data passes through here.
// The deploy job replaces BUILD with the commit, so every deploy gets a fresh
// cache and the old one is deleted on activation.
const CACHE = 'proofback-app-BUILD';
const FILES = [
  './', 'index.html', 'app.css', 'app.js', 'worker.js',
  'engine/proofback_wasm.js', 'engine/proofback_wasm_bg.wasm', 'engine/pdfium.js', 'engine/pdfium.wasm',
  '../styles.css', '../favicon.svg', '../fonts/schibsted-grotesk.woff2', '../fonts/jetbrains-mono.woff2',
];

self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(FILES)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', (e) => {
  e.waitUntil(caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))).then(() => self.clients.claim()));
});

self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url);
  if (e.request.method !== 'GET' || url.origin !== location.origin) return;
  e.respondWith(caches.match(e.request).then((hit) => hit || fetch(e.request)));
});
