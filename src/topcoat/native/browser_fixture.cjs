// Mounted HTTP and WebSocket proxy for real native production-server tests.
const http = require('node:http');
const net = require('node:net');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const assert = require('node:assert/strict');
const {Transform} = require('node:stream');

async function readDevToolsPort(activePort, platform = process.platform) {
  const deadline = performance.now() + 15000;
  let port;
  while (port === undefined) {
    try {
      const first = (await require('node:fs/promises').readFile(activePort, 'utf8')).split('\n')[0];
      if (/^[0-9]+$/.test(first)) port = first;
    } catch (error) {
      if (error.code !== 'ENOENT' && !(platform === 'win32' && error.code === 'EBUSY')) throw error;
    }
    if (port !== undefined) break;
    assert.ok(performance.now() < deadline, `Chrome did not publish its DevTools port: ${activePort}`);
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  return port;
}


async function launchBrowser() {
  assert.ok(process.platform === 'win32' || process.env.PLAYWRIGHT_EXECUTABLE_PATH,
    'Use the repository Chromium environment.');
  const moduleUrl = pathToFileURL(path.resolve(__dirname, '../../../e2e/node_modules/playwright/index.mjs'));
  const {chromium} = await import(moduleUrl.href);
  return chromium.launch({headless: true, channel: 'chromium', executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
}

// Playwright's normal page initialization forces focus/visibility. This mode
// uses its public CDP noDefaults option and Chrome's real default-context tabs.
async function launchVisibilityBrowser() {
  assert.ok(process.platform === 'win32' || process.env.PLAYWRIGHT_EXECUTABLE_PATH,
    'Use the repository Chromium environment.');
  const moduleUrl = pathToFileURL(path.resolve(__dirname, '../../../e2e/node_modules/playwright/index.mjs'));
  const {chromium} = await import(moduleUrl.href);
  const server = await chromium.launchServer({headless: true, channel: 'chromium',
    executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH, args: ['--remote-debugging-port=0']});
  let browser;
  try {
    const profile = server.process().spawnargs.find(arg => arg.startsWith('--user-data-dir='));
    assert.ok(profile, 'Playwright owns the temporary Chrome profile.');
    const activePort = path.join(profile.slice('--user-data-dir='.length), 'DevToolsActivePort');
    // Chrome's transport can be ready before its discovery file is committed.
    const port = await readDevToolsPort(activePort);
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, {noDefaults: true});
    const context = browser.contexts()[0];
    assert.ok(context, 'Actual default context preserves real tab visibility.');
    return {browser, context, close: async () => {
      try {await browser.close();} finally {await server.close();}
    }};
  } catch (error) {
    try {if (browser) await browser.close();} finally {await server.close();}
    throw error;
  }
}

// Optional gate is fixture-only observation/control of genuine transport bytes.
function makeIncomingGate() {
  let held = false, buffers = [], bytes = 0, failure = null;
  const stream = new Transform({
    transform(chunk, encoding, callback) {
      if (!held) {callback(null, chunk); return;}
      bytes += chunk.length;
      if (bytes > 2 * 1024 * 1024) {
        callback(new Error('Home proof held more than 2 MiB of genuine incoming transport.'));
        return;
      }
      buffers.push(Buffer.from(chunk));
      callback();
    },
  });
  stream.on('error', error => {failure = error;});
  return {
    stream,
    hold() {assert.equal(held, false); held = true; buffers = []; bytes = 0;},
    text() {if (failure) throw failure; return Buffer.concat(buffers).toString('utf8');},
    release() {
      if (failure) throw failure;
      assert.equal(held, true);
      held = false;
      for (const chunk of buffers) stream.push(chunk);
      buffers = []; bytes = 0;
    },
    dispose() {buffers = []; bytes = 0; stream.destroy();},
  };
}

async function mountedProxy(upstream, prefix, options = {}) {
  const incoming = options.incomingPath ? makeIncomingGate() : null;
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
    requests.push({method: request.method, path: request.url, headers: {...request.headers}});
    const forwarded = http.request(upstream, {path: logical, method: request.method, headers: headersFor(request)}, incoming => {
      response.writeHead(incoming.statusCode, incoming.headers);
      incoming.pipe(response);
    });
    connections.add(forwarded);
    forwarded.on('close', () => connections.delete(forwarded));
    request.on('aborted', () => forwarded.destroy());
    response.on('close', () => {if (!response.writableFinished) forwarded.destroy();});
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
      socket.pipe(backend);
      if (incoming && logical.split('?')[0] === options.incomingPath) {
        backend.pipe(incoming.stream).pipe(socket);
        incoming.stream.on('error', () => {socket.destroy(); backend.destroy();});
      } else backend.pipe(socket);
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
    origin: `http://127.0.0.1:${server.address().port}`, requests, sockets, incomingGate: incoming,
    async close() {
      if (incoming) incoming.dispose();
      for (const connection of connections) connection.destroy();
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    },
  };
}

module.exports = {mountedProxy, launchBrowser, launchVisibilityBrowser, readDevToolsPort};
