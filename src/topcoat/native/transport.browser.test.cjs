const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const {wsServer: WebSocketServer} = require('../../../e2e/node_modules/playwright-core/lib/utilsBundle.js');

const runtime = fs.readFileSync(path.join(__dirname, '../assets/runtime.js'), 'utf8');
const escape = value => value.replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
const procedure = endpoint => ({t: 'Procedure', path: endpoint});

// This fixture speaks the pinned framework transport protocol. Domain/auth
// integration is exercised separately against the actual Lific executable.
test('framework procedures, returned surrogates, shards and sockets stay within the Rust-rendered mount', async t => {
  assert.ok(process.env.PLAYWRIGHT_EXECUTABLE_PATH, 'Use the repository e2e Chromium environment.');
  const {chromium} = await import(path.resolve(__dirname, '../../../e2e/node_modules/playwright/index.mjs'));
  const browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
  try {
    for (const prefix of ['', '/app', '/ACC']) await t.test(prefix || 'root', async () => {
      const requests = [], sockets = [];
      const server = http.createServer(async (request, response) => {
        const body = [];
        for await (const chunk of request) body.push(chunk);
        const content = Buffer.concat(body).toString();
        if (request.method === 'POST') requests.push({path: request.url, body: JSON.parse(content), headers: request.headers});
        if (request.url === `${prefix}/runtime.js`) {
          response.setHeader('Content-Type', 'text/javascript'); response.end(runtime); return;
        }
        if (request.method === 'POST') {
          if (request.url.endsWith('/native/shard')) {
            response.setHeader('Content-Type', 'application/x-ndjson');
            response.end(`${JSON.stringify({t: 'snapshot', html: '<p>Shard refreshed</p>'})}\n`); return;
          }
          response.setHeader('Content-Type', 'application/json');
          response.end(JSON.stringify(request.url.endsWith('/native/factory')
            ? {t: 'Result', ok: {t: 'Option', v: procedure('/ACC/native/returned')}} : true));
          return;
        }
        const call = `async()=>{const p=cx.hydrate(${JSON.stringify(procedure('/native/factory'))});const result=await p.call();await result.unwrap().unwrap().call();await cx.hydrate(${JSON.stringify(procedure('/native/echo'))}).call(result);document.querySelector('#done').textContent='done';}`;
        const nested = `async()=>{await cx.hydrate(${JSON.stringify({t: 'Option', v: procedure('/native/nested')})}).unwrap().call();document.querySelector('#nested-done').textContent='done';}`;
        const absolute = `async()=>{await cx.hydrate(${JSON.stringify(procedure(`http://${request.headers.host}/native/absolute`))}).call();await cx.hydrate(${JSON.stringify(procedure(`//${request.headers.host}/native/protocol-relative`))}).call();document.querySelector('#absolute-done').textContent='done';}`;
        const shard = (endpoint, identity, connection) => `<!-- ::topcoat::shard::start(${JSON.stringify(endpoint)}, ${JSON.stringify(identity)}, ["${escape('cx.signal("a").get()')}"]) --><p>Initial shard</p>${connection ? '<!-- ::topcoat::connect -->' : ''}<!-- ::topcoat::shard::end(${JSON.stringify(identity)}) -->`;
        response.setHeader('Content-Type', 'text/html');
        response.end(`<html data-topcoat-runtime-prefix="${prefix}"><head><script>window.originalFetch=window.fetch;</script></head><body>
          <!-- ::topcoat::signal({"t":"signal","id":"a","v":false}) -->
          <button data-topcoat-on:click="${escape(call)}">Call factory</button><output id="done"></output>
          <button data-topcoat-on:click="${escape(nested)}">Call nested</button><output id="nested-done"></output>
          <button data-topcoat-on:click="${escape(absolute)}">Call absolute</button><output id="absolute-done"></output>
          <button data-topcoat-on:click="${escape('()=>cx.signal("a").toggle()')}">Refresh shard</button>
          ${shard('/native/shard', '1', false)}${shard('/native/socket', '2', true)}
          <script type="module" src="${prefix}/runtime.js"></script></body></html>`);
      });
      const ws = new WebSocketServer({noServer: true});
      server.on('upgrade', (request, socket, head) => {
        sockets.push({path: request.url, protocol: request.headers['sec-websocket-protocol']});
        ws.handleUpgrade(request, socket, head, client => ws.emit('connection', client, request));
      });
      await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
      const context = await browser.newContext();
      try {
        const origin = `http://127.0.0.1:${server.address().port}`;
        await context.addCookies([{name: 'lific_session', value: 'fixture-session', url: origin, httpOnly: true}]);
        const page = await context.newPage(), failures = [];
        page.setDefaultTimeout(5000);
        page.on('pageerror', error => failures.push(error.message));
        await page.goto(`${origin}${prefix}/ACC/overview`);
        await page.getByRole('button', {name: 'Call factory', exact: true}).click();
        await page.locator('#done').filter({hasText: 'done'}).waitFor();
        await page.getByRole('button', {name: 'Call nested', exact: true}).click();
        await page.locator('#nested-done').filter({hasText: 'done'}).waitFor();
        await page.getByRole('button', {name: 'Refresh shard', exact: true}).click();
        await page.getByText('Shard refreshed', {exact: true}).waitFor();
        await page.getByRole('button', {name: 'Call absolute', exact: true}).click();
        await page.locator('#absolute-done').filter({hasText: 'done'}).waitFor();
        assert.deepEqual(requests.map(request => request.path), [
          `${prefix}/native/factory`, `${prefix}/ACC/native/returned`, `${prefix}/native/echo`,
          `${prefix}/native/nested`, `${prefix}/native/shard`,
          '/native/absolute', '/native/protocol-relative',
        ]);
        assert.deepEqual(requests[2].body, [{t: 'Result', ok: {t: 'Option', v: procedure('/ACC/native/returned')}}],
          'Dehydration preserves logical procedure paths, preventing a second mount prefix.');
        assert.equal(requests[4].headers['x-topcoat-identity'], '1');
        assert.deepEqual(requests[4].body.args, [true]);
        assert.deepEqual(sockets, [{path: `${prefix}/native/socket`, protocol: 'topcoat-runtime'}]);
        for (const request of requests) {
          assert.equal(request.headers.cookie, 'lific_session=fixture-session', 'Native fetch keeps existing same-origin cookie behavior.');
          assert.equal(request.headers.authorization, undefined, 'The runtime does not synthesize browser bearer credentials.');
        }
        assert.equal(await page.evaluate(() => window.fetch === window.originalFetch), true, 'Framework transport never intercepts global fetch.');
        assert.deepEqual(failures, []);
      } finally {
        await context.close();
        for (const client of ws.clients) client.terminate();
        await new Promise(resolve => ws.close(resolve));
        await new Promise(resolve => server.close(resolve));
      }
    });
  } finally {await browser.close();}
});

test('Rust tuple arrays hydrate, index and round-trip nested framework values', async () => {
  assert.ok(process.env.PLAYWRIGHT_EXECUTABLE_PATH, 'Use the repository e2e Chromium environment.');
  const {chromium} = await import(path.resolve(__dirname, '../../../e2e/node_modules/playwright/index.mjs'));
  const wire = [
    {t: 'Result', ok: 'saved'},
    {t: 'Option', v: {t: 'i64', bits: 64, v: '9007199254740993'}},
    {t: 'Option', v: [{t: 'usize', bits: 64, v: '7'}, procedure('/ACC/native/returned')]},
    {t: 'Result', err: {t: 'Option', v: null}},
    false, 'web', [true, null, 'draft'],
    {t: 'Vec', bits: 64, v: [[{t: 'Option', v: {t: 'i64', bits: 64, v: '-9223372036854775808'}}, 42]]},
  ];
  const requests = [];
  const server = http.createServer(async (request, response) => {
    if (request.url === '/ACC/runtime.js') {
      response.setHeader('Content-Type', 'text/javascript'); response.end(runtime); return;
    }
    if (request.method === 'POST') {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      requests.push({path: request.url, body: JSON.parse(Buffer.concat(chunks).toString())});
      response.setHeader('Content-Type', 'application/json');
      response.end(JSON.stringify(request.url === '/ACC/native/tuple-factory' ? wire : true)); return;
    }
    const call = `async()=>{try{
      const tuple=await cx.hydrate(${JSON.stringify(procedure('/native/tuple-factory'))}).call();
      if(!Array.isArray(tuple)||tuple.length!==8)throw new Error('Tuple shape');
      if(!tuple[0].is_ok().v||tuple[0].unwrap().v!=='saved')throw new Error('Result indexing');
      if(tuple[1].unwrap().v.toString()!=='9007199254740993')throw new Error('Integer precision');
      const nested=tuple[2].unwrap();
      if(nested[0].v.toString()!=='7'||typeof tuple.dehydrate!=='function'||typeof nested.dehydrate!=='function')throw new Error('Nested tuple methods');
      await nested[1].call();
      document.querySelector('#wire').textContent=JSON.stringify(tuple.dehydrate());
      await cx.hydrate(${JSON.stringify(procedure('/native/tuple-echo'))}).call(tuple,[tuple[1],[nested[0],tuple[3]]]);
      document.querySelector('#result').textContent='ok';
    }catch(error){document.querySelector('#result').textContent=error.message;}}`;
    response.setHeader('Content-Type', 'text/html');
    response.end(`<html data-topcoat-runtime-prefix="/ACC"><body><button data-topcoat-on:click="${escape(call)}">Round-trip tuple</button><output id="result"></output><output id="wire"></output><script type="module" src="/ACC/runtime.js"></script></body></html>`);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
  try {
    const page = await browser.newPage(), failures = [];
    page.on('pageerror', error => failures.push(error.message));
    await page.goto(`http://127.0.0.1:${server.address().port}/ACC/ACC/overview`);
    await page.getByRole('button', {name: 'Round-trip tuple', exact: true}).click();
    await page.waitForFunction(() => document.querySelector('#result').textContent !== '');
    assert.equal(await page.locator('#result').textContent(), 'ok');
    assert.deepEqual(JSON.parse(await page.locator('#wire').textContent()), wire);
    assert.deepEqual(requests, [
      {path: '/ACC/native/tuple-factory', body: []},
      {path: '/ACC/ACC/native/returned', body: []},
      {path: '/ACC/native/tuple-echo', body: [wire, [wire[1], [wire[2].v[0], wire[3]]]]},
    ]);
    assert.deepEqual(failures, []);
  } finally {
    await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
});
