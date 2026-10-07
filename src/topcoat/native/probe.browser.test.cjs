const {launchBrowser} = require('./browser_fixture.cjs');
const {test} = require('node:test');
const assert = require('node:assert/strict');
const http = require('node:http');
const net = require('node:net');
const path = require('node:path');

// The Rust test owns an ephemeral assembled app and real database/session.
// This proxy changes only the deployment mount; every handler remains real.
test('assembled native page, procedure, shard and socket preserve the mounted session', async t => {
  assert.ok(process.argv[2], 'Rust must supply the assembled fixture URL.');
  assert.ok(process.argv[3], 'Rust must supply a fixture session token.');

  const upstream = new URL(process.argv[2]);
  const cookie = `lific_token=${process.argv[3]}`;
  const separator = cookie.indexOf('=');
  assert.ok(separator > 0, 'Fixture cookie must contain its name.');

  const browser = await launchBrowser();
  try {
    for (const prefix of ['', '/app', '/ACC']) await t.test(prefix || 'root', async () => {
      const requests = [], sockets = [], frames = [], connections = new Set();
      const logicalPath = url => !prefix ? url : url.startsWith(`${prefix}/`) ? url.slice(prefix.length) : null;
      const headersFor = request => {
        const headers = {...request.headers};
        for (const name of ['forwarded', 'x-forwarded-for', 'x-forwarded-host', 'x-forwarded-proto', 'x-forwarded-prefix']) delete headers[name];
        if (prefix) headers['x-forwarded-prefix'] = prefix;
        return headers;
      };
      const proxy = http.createServer((request, response) => {
        const logical = logicalPath(request.url);
        if (!logical) {response.writeHead(404); response.end(); return;}
        requests.push({method: request.method, path: request.url});
        const forwarded = http.request(upstream, {
          path: logical, method: request.method, headers: headersFor(request),
        }, incoming => {
          response.writeHead(incoming.statusCode, incoming.headers);
          incoming.pipe(response);
        });
        forwarded.on('error', () => {response.writeHead(502); response.end();});
        request.pipe(forwarded);
      });
      proxy.on('upgrade', (request, socket, head) => {
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
      await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
      const context = await browser.newContext({locale: 'en-US', timezoneId: 'America/Denver'});
      try {
        const browserRequests = [];
        context.on('request', request => browserRequests.push(request.url()));
        const origin = `http://127.0.0.1:${proxy.address().port}`;
        await context.addCookies([{name: cookie.slice(0, separator), value: cookie.slice(separator + 1), url: origin, httpOnly: true}]);
        const page = await context.newPage(), failures = [], consoleErrors = [], saveResponses = [];
        const clock = '2026-10-03T16:00:00Z';
        const stored = JSON.stringify([{title: '<script>local input</script>', project: 'ACC'}]);
        await page.clock.setFixedTime(clock);
        await page.addInitScript(({stored, denied}) => {
          if (denied) Object.defineProperty(Storage.prototype, 'getItem', {
            value() { throw new DOMException('Storage is unavailable', 'SecurityError'); },
          });
          else localStorage.setItem('lific_recents', stored);
        }, {stored, denied: prefix === '/ACC'});
        page.setDefaultTimeout(10000);
        page.on('pageerror', error => failures.push(error.message));
        page.on('console', message => {
          if (message.type() === 'error') consoleErrors.push(message.text());
        });
        page.on('response', async response => {
          if (new URL(response.url()).pathname.endsWith('/__native_probe/save')) {
            saveResponses.push({status: response.status(), body: await response.text()});
          }
        });
        page.on('websocket', socket => {
          socket.on('framesent', ({payload}) => {
            frames.push({path: new URL(socket.url()).pathname, frame: JSON.parse(payload.toString())});
          });
        });
        const initial = await page.goto(`${origin}${prefix}/ACC/__native_probe`);
        assert.equal(initial.status(), 200);
        assert.ok((await initial.text()).includes('native-probe-title'), 'The server renders the authorized issue in initial HTML.');
        assert.equal(await page.locator('#native-probe-browser-inputs').count(), 1,
          'The real Rust page mounts primitive browser inputs.');
        await page.locator('#native-probe-browser-ready').filter({hasText: 'true'}).waitFor();
        assert.deepEqual(JSON.parse(await page.locator('#native-probe-browser-inputs').textContent()), {
          epochMilliseconds: Date.parse(clock), timezoneOffsetMinutes: 360,
          locale: 'en-US', storedValue: prefix === '/ACC' ? null : stored,
        });
        await page.locator('#native-probe-issue[data-connected="true"]').waitFor();
        const initialFrame = frames.filter(({frame}) => frame.path === '/__native_probe/issue').at(-1).frame;
        const initialRun = initialFrame.run;
        const initialInput = JSON.parse(initialFrame.body);
        assert.equal(initialInput.args[0].t, 'usize');
        assert.equal(initialInput.args[0].v, '0');
        let before = BigInt(await page.locator('#native-probe-sequence').textContent());
        let callsBefore = Number(await page.locator('#native-probe-calls').textContent());
        let previousRun = initialRun;
        for (const saveNumber of [1, 2]) {
          const title = `Mounted native save ${prefix || 'root'} ${saveNumber}`;
          await page.locator('#native-probe-draft').fill(title);
          await page.locator('#native-probe-save').click();
          try {
            await page.locator('#native-probe-saved').filter({hasText: 'true'}).waitFor();
          } catch (error) {
            const handler = await page.locator('#native-probe-save').getAttribute('data-topcoat-on:click');
            const saved = await page.locator('#native-probe-saved').textContent();
            throw new Error(`Native save did not complete: ${JSON.stringify({prefix, saveNumber, failures, consoleErrors, saveResponses, handler, saved, frames})}`, {cause: error});
          }
          await page.locator('#native-probe-title').filter({hasText: title}).waitFor();
          const sequence = BigInt(await page.locator('#native-probe-sequence').textContent());
          const calls = Number(await page.locator('#native-probe-calls').textContent());
          assert.ok(sequence > before);
          assert.equal(calls, callsBefore + 1, 'Each real save commits once through the shared Rust service.');
          const refreshedRun = frames.filter(({frame}) =>
            frame.path === '/__native_probe/issue' && frame.run > previousRun).at(-1);
          assert.ok(refreshedRun, 'The connected shard sends a new real WebSocket render run after each save.');
          assert.deepEqual(JSON.parse(refreshedRun.frame.body).args, [{...initialInput.args[0], v: String(saveNumber)}]);
          assert.equal(refreshedRun.frame.headers['x-topcoat-identity'], initialFrame.headers['x-topcoat-identity'],
            'Refresh retains the same shard invocation.');
          before = sequence; callsBefore = calls; previousRun = refreshedRun.frame.run;
        }
        const nativeRequests = requests.filter(request => request.path.includes('__native_probe'));
        assert.ok(nativeRequests.some(request => request.method === 'POST' && request.path === `${prefix}/__native_probe/save`));
        assert.deepEqual(sockets, [`${prefix}/ACC/__native_probe`], 'Connected targets share the mounted document socket.');
        assert.deepEqual(requests.filter(request => request.method === 'POST').map(request => request.path),
          [`${prefix}/__native_probe/save`, `${prefix}/__native_probe/save`],
          'Browser input initialization needs no procedure or page-render request.');
        assert.equal(browserRequests.some(url => new URL(url).pathname.split('/').includes('api')), false,
          'All browser requests, including unmounted and other-origin URLs, remain outside REST.');
        assert.deepEqual(failures, []);
      } finally {
        await context.close();
        for (const connection of connections) connection.destroy();
        proxy.closeAllConnections();
        await new Promise(resolve => proxy.close(resolve));
      }
    });
  } finally {await browser.close();}
});
