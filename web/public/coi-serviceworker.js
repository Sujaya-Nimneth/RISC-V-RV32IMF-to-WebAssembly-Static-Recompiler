/*! coi-serviceworker fallback for static hosts without COOP/COEP headers (e.g., GitHub Pages) */
if (typeof window === 'undefined') {
  self.addEventListener('install', () => self.skipWaiting());
  self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));
  self.addEventListener('fetch', (e) => {
    if (e.request.cache === 'only-if-cached' && e.request.mode !== 'same-origin') return;
    e.respondWith(
      fetch(e.request).then((res) => {
        if (res.status === 0) return res;
        const headers = new Headers(res.headers);
        headers.set('Cross-Origin-Embedder-Policy', 'require-corp');
        headers.set('Cross-Origin-Opener-Policy', 'same-origin');
        return new Response(res.body, { status: res.status, statusText: res.statusText, headers });
      })
    );
  });
} else if (!window.crossOriginIsolated && 'serviceWorker' in navigator && window.isSecureContext) {
  navigator.serviceWorker.register('/coi-serviceworker.js').catch(() => {});
}
