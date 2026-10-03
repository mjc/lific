/* Native public media sends every network range through an anonymous fetch. */
'use strict';
self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  const request = event.request, url = new URL(request.url);
  const match = url.pathname.match(/^\/public\/([A-Za-z][A-Za-z0-9_-]*)\/_media\/([1-9]\d*)$/);
  if (url.origin !== self.location.origin || request.method !== 'GET' || !match || !['audio', 'video'].includes(request.destination)) return;
  event.respondWith((async () => {
    const client = await self.clients.get(event.clientId);
    const page = client && new URL(client.url);
    const project = page?.pathname.match(/^\/public\/([^/]+)\//)?.[1];
    if (!page || page.origin !== url.origin || project?.toUpperCase() !== match[1].toUpperCase()) return new Response('Public media unavailable.', {status:403});
    const headers = new Headers();
    for (const name of ['Range', 'If-Range']) if (request.headers.has(name)) headers.set(name, request.headers.get(name));
    return fetch(`/public/api/projects/${encodeURIComponent(match[1])}/attachments/${match[2]}`, {
      method:'GET', credentials:'omit', headers, signal:request.signal, redirect:'error',
    });
  })());
});
