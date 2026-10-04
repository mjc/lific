// Mounted HTTP and WebSocket proxy for real native production-server tests.
const http = require('node:http');
const net = require('node:net');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const assert = require('node:assert/strict');

async function launchBrowser() {
  assert.ok(process.platform === 'win32' || process.env.PLAYWRIGHT_EXECUTABLE_PATH,
    'Use the repository Chromium environment.');
  const moduleUrl = pathToFileURL(path.resolve(__dirname, '../../../e2e/node_modules/playwright/index.mjs'));
  const {chromium} = await import(moduleUrl.href);
  return chromium.launch({headless: true, channel: 'chromium', executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
}

async function mountedProxy(upstream, prefix) {
  const requests = [], sockets = [], connections = new Set();
  const logicalPath = url => !prefix ? url : url.startsWith(`${prefix}/`) ? url.slice(prefix.length) : null;
  const headersFor = request => {
    const headers = {...request.headers};
    for (const name of ['forwarded', 'x-forwarded-for', 'x-forwarded-host', 'x-forwarded-proto', 'x-forwarded-prefix']) delete headers[name];
    if (prefix) headers['x-forwarded-prefix'] = prefix;
    return headers;
  };
  const server = http.createServer((request, response) => {
    const logical = logicalPath(request.url);
    if (!logical) {response.writeHead(404); response.end(); return;}
    requests.push({method: request.method, path: request.url});
    const forwarded = http.request(upstream, {path: logical, method: request.method, headers: headersFor(request)}, incoming => {
      response.writeHead(incoming.statusCode, incoming.headers);
      incoming.pipe(response);
    });
    forwarded.on('error', () => {if (!response.headersSent) response.writeHead(502); response.end();});
    request.pipe(forwarded);
  });
  server.on('upgrade', (request, socket, head) => {
    const logical = logicalPath(request.url);
    if (!logical) {socket.destroy(); return;}
    sockets.push(request.url);
    const backend = net.connect(Number(upstream.port || 80), upstream.hostname, () => {
      const headers = headersFor(request);
      backend.write(`${request.method} ${logical} HTTP/${request.httpVersion}\r\n${Object.entries(headers).map(([name, value]) => `${name}: ${value}`).join('\r\n')}\r\n\r\n`);
      if (head.length) backend.write(head);
      socket.pipe(backend); backend.pipe(socket);
    });
    connections.add(socket); connections.add(backend);
    socket.on('close', () => {connections.delete(socket); backend.destroy();});
    socket.on('error', () => backend.destroy());
    backend.on('close', () => {connections.delete(backend); socket.destroy();});
    backend.on('error', () => socket.destroy());
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  return {
    origin: `http://127.0.0.1:${server.address().port}`, requests, sockets,
    async close() {
      for (const connection of connections) connection.destroy();
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    },
  };
}

module.exports = {mountedProxy, launchBrowser};
