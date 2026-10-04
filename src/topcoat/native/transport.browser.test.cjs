const {launchBrowser} = require('./browser_fixture.cjs');
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const {wsServer: WebSocketServer} = require('../../../e2e/node_modules/playwright-core/lib/utilsBundle.js');

const runtime = fs.readFileSync(path.join(__dirname, '../assets/runtime.js'), 'utf8');
const escape = value => value.replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
const procedure = endpoint => ({t: 'Procedure', path: endpoint});

test('one document navigation is claimed across sibling socket and HTTP redirects', async () => {
  const bootstrap = 'var Ve=new ne;Ve.start(document);Ve.page.listenForDevRefresh();';
  assert.equal(runtime.split(bootstrap).length - 1, 1, 'The production document startup is replaced exactly once.');
  const locations = [], failures = [];
  const context = {
    AbortController, queueMicrotask, TextEncoder, TextDecoder,
    location: {assign: destination => locations.push(destination)},
    console: {error: error => failures.push(error)},
  };
  require('node:vm').runInNewContext(runtime.replace(bootstrap,
    'globalThis.fixture={create:()=>new ne,frame:z,Request:H};'), context);
  const {create, frame, Request} = context.fixture;
  const redirect = (document, destination) => frame({runtime: document},
    {t: 'redirect', location: destination}, 'Connected', Symbol('render'));
  const httpRedirect = async (document, destination) => {
    const controller = new Request(new AbortController().signal,
      error => failures.push(error), location => document.redirect(location));
    await controller.run(async () => ({redirected: true, url: destination}),
      () => assert.fail('An HTTP redirect must not produce render frames.'), 'HTTP');
  };
  for (const source of ['sibling sockets', 'socket then HTTP', 'HTTP then socket', 'sibling HTTP']) {
    locations.length = 0;
    const document = create();
    if (source.startsWith('HTTP') || source === 'sibling HTTP') await httpRedirect(document, '/mounted/first');
    else redirect(document, '/mounted/first');
    if (source.endsWith('HTTP')) await httpRedirect(document, '/mounted/second');
    else redirect(document, '/mounted/second');
    assert.deepEqual(locations, ['/mounted/first'], `${source} shares one document navigation claim.`);
    redirect(create(), '/mounted/new-document');
    assert.deepEqual(locations, ['/mounted/first', '/mounted/new-document'],
      'A new document Runtime owns a fresh navigation claim.');
  }
  assert.deepEqual(failures, []);
});

test('vendored patches reconstruct the exact pinned upstream runtime', () => {
  const start = runtime.indexOf('function A(t,e,n,r,i={})');
  assert.ok(start >= 0, 'The pinned upstream body is present.');
  let original = runtime.slice(start);
  for (const [patched, upstream, count] of [
    ['registry;event(e){return new j(e)}hydrate(e){return V(e,this)}', 'registry;hydrate(e){return V(e,this)}', 1],
    ["push(e){this.inner.set(n=>n.clone_with_push(e))}remove(e){this.inner.set(n=>n.clone_without_index(e))}push_str(e){this.inner.set(n=>new v(`${n}${e}`))}", "push_str(e){this.inner.set(n=>new v(`${n}${e}`))}", 1],
    ["clone(){return this.to_vec()}clone_with_push(e){let n=this.items.map(b);return n.push(b(e)),new F(n,this.usizeType)}clone_without_index(e){let n=e.toIndex(this.items.length,this.usizeType.bits);if(n===void 0)throw new RangeError(\"Vec index out of bounds\");let r=this.items.map(b);return r.splice(n,1),new F(r,this.usizeType)}dehydrate(){return{t:\"Vec\",bits:this.usizeType.bits,v:this.items.map(f)}}", "clone(){return this.to_vec()}dehydrate(){return{t:\"Vec\",bits:this.usizeType.bits,v:this.items.map(f)}}", 1],
    ['call(...e){return this.request(e,!1)}call_keepalive(...e){return this.request(e,!0)}with_keepalive(){return{call:(...e)=>this.call_keepalive(...e)}}request(e,n){return new X(async()=>{let r=await fetch(topcoatMountedEndpoint(this.path),{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(e.map(f)),...(n?{keepalive:!0}:{})});if(!r.ok)throw new Error(`Procedure call failed: ${r.status} ${r.statusText}`);return this.cx.hydrate(await r.json())})}',
      'call(...e){return new X(async()=>{let n=await fetch(topcoatMountedEndpoint(this.path),{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(e.map(f))});if(!n.ok)throw new Error(`Procedure call failed: ${n.status} ${n.statusText}`);return this.cx.hydrate(await n.json())})}', 1],
    ['function pe(t){let e=new DOMParser().parseFromString(t.replaceAll("<","&lt;"),"text/html")',
      'function pe(t){let e=new DOMParser().parseFromString(t,"text/html")', 1],
    ['fetch(topcoatMountedEndpoint(this.path)', 'fetch(this.path', 2],
    ['url(){return topcoatMountedEndpoint(this.path)}', 'url(){return this.path}', 1],
    ['function V(t,e){if(Array.isArray(t))return topcoatHydrateTuple(t,e);if(t!==null)', 'function V(t,e){if(t!==null)', 1],
    ['function f(t){if(t==null)return null;if(Array.isArray(t))return t.map(f);', 'function f(t){if(t==null)return null;', 1],
    ['let r=e.name.substring(ke.length);if(r==="mount"){topcoatMount(t,()=>T(e.value,`event @${r}`)(Object.assign(Object.create(n.runtime.context),{abortSignal:n.abortSignal})),n);return}let o=T(e.value,`event @${r}`)(n.runtime.context);',
      'let r=e.name.substring(ke.length),o=T(e.value,`event @${r}`)(n.runtime.context);', 1],
    ['refresh(){if(this.isDisposed)return Promise.resolve();if(this.connection!==null)return this.connection.requestRun(),Promise.resolve();if(this.requiresConnection){for(let n of this.ancestors())if(n.connection!==null||n.requiresConnection)return n.refresh();return this.connectIfRequired(),Promise.resolve()}',
      'refresh(){if(this.connection?.isOpen)return this.connection.requestRun(),Promise.resolve();if(this.requiresConnection){for(let n of this.ancestors())if(n.connection?.isOpen)return n.refresh()}', 1],
    ['case"redirect":t.runtime.redirect(e.location);break;', 'case"redirect":location.assign(e.location);break;', 1],
    ['constructor(e,n,r){this.lifetime=e;this.reportError=n;this.redirect=r;e.addEventListener',
      'constructor(e,n){this.lifetime=e;this.reportError=n;e.addEventListener', 1],
    ['if(s.redirected){this.redirect(s.url);return}', 'if(s.redirected){location.assign(s.url);return}', 1],
    ['new H(this.lifetime.abortSignal,r=>n.reportError(r),r=>n.redirect(r))',
      'new H(this.lifetime.abortSignal,r=>n.reportError(r))', 1],
    ['var ne=class{navigating=!1;redirect(e){if(this.navigating)return;this.navigating=!0;location.assign(e)}registry=new te;',
      'var ne=class{registry=new te;', 1],
  ]) {
    assert.equal(original.split(patched).length - 1, count, 'Each declared patch has its expected occurrence count.');
    original = original.replaceAll(patched, upstream);
  }
  const digest = require('node:crypto').createHash('sha256').update(original).digest('hex');
  assert.equal(digest, '980dd1be1962006b98b8c1646b0e6a4f86a78ec721e2739f59ddf4c4c1c5c8b4');
});

// This fixture speaks the pinned framework transport protocol. Domain/auth
// integration is exercised separately against the actual Lific executable.
test('connected renders wait for load, socket open and reconnect using fresh inputs', async t => {

  const browser = await launchBrowser();
  try {
    for (const ancestor of [false, true]) await t.test(ancestor ? 'ancestor connection' : 'own connection', async () => {
      const requests = [], runs = [], upgrades = [], pendingSockets = new Set();
      let heldImage;
      const connectedContent = value => `<p id="socket-result">Socket ${value}</p><!-- ::topcoat::connect -->`;
      const documentBody = value => `
        <!-- ::topcoat::signal({"t":"signal","id":"a","v":0}) -->
        <!-- ::topcoat::signal({"t":"signal","id":"b","v":0}) -->
        ${ancestor ? '<!-- ::topcoat::connect -->' : ''}
        <button id="mount" data-topcoat-on:mount="${escape('()=>{if(cx.signal("a").get().v===0)cx.signal("a").increment()}')}">Mounted</button>
        <button data-topcoat-on:click="${escape('()=>cx.signal("a").increment()')}">Update connected</button>
        <button data-topcoat-on:click="${escape('()=>cx.signal("b").increment()')}">Update HTTP</button>
        <!-- ::topcoat::shard::start("/native/connected", "1", ["${escape('cx.signal("a").get()')}"]) -->
        ${connectedContent(value)}
        <!-- ::topcoat::shard::end("1") -->
        <!-- ::topcoat::shard::start("/native/http", "2", ["${escape('cx.signal("b").get()')}"]) -->
        <p id="http-result">Initial HTTP</p>
        <!-- ::topcoat::shard::end("2") -->`;
      // Page.prepare parses a complete HTML document. Keep leading signal and
      // connection comments inside its body, as real page snapshots do.
      const pageContent = value => `<html><body>${documentBody(value)}</body></html>`;
      const server = http.createServer(async (request, response) => {
        if (request.url === '/ACC/runtime.js') {
          response.setHeader('Content-Type', 'text/javascript'); response.end(runtime); return;
        }
        if (request.url === '/ACC/held.svg') { heldImage = response; return; }
        if (request.method === 'POST') {
          const chunks = [];
          for await (const chunk of request) chunks.push(chunk);
          const body = JSON.parse(Buffer.concat(chunks).toString());
          requests.push({path: request.url, body});
          const html = request.url === '/ACC/native/http'
            ? `<p id="http-result">HTTP ${body.args[0]}</p>`
            : request.url === '/ACC/fixture' ? pageContent('fallback') : connectedContent('fallback');
          response.setHeader('Content-Type', 'application/x-ndjson');
          response.end(`${JSON.stringify({t: 'snapshot', html})}\n`); return;
        }
        response.setHeader('Content-Type', 'text/html');
        response.end(`<html data-topcoat-runtime-prefix="/ACC"><body><img src="/ACC/held.svg">${documentBody(0)}<script type="module" src="/ACC/runtime.js"></script></body></html>`);
      });
      const ws = new WebSocketServer({noServer: true});
      server.on('upgrade', (request, socket, head) => {
        pendingSockets.add(socket);
        upgrades.push(() => {
          pendingSockets.delete(socket);
          ws.handleUpgrade(request, socket, head, client => {
            client.on('message', message => {
              const run = JSON.parse(message.toString());
              runs.push({path: request.url, run});
              const value = ancestor ? run.signals.a : run.args[0];
              client.send(JSON.stringify({t: 'run', id: run.run}));
              client.send(JSON.stringify({t: 'snapshot', html: ancestor ? pageContent(value) : connectedContent(value)}));
            });
          });
        });
        server.emit('held-upgrade');
      });
      await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
      const page = await browser.newPage(), failures = [];
      page.setDefaultTimeout(5000);
      page.on('pageerror', error => failures.push(error.message));
      try {
        await page.goto(`http://127.0.0.1:${server.address().port}/ACC/fixture`, {waitUntil: 'domcontentloaded'});
        await page.getByRole('button', {name: 'Update connected', exact: true}).click();
        await page.getByRole('button', {name: 'Update connected', exact: true}).click();
        await page.getByRole('button', {name: 'Update HTTP', exact: true}).click();
        await page.getByText('HTTP 1', {exact: true}).waitFor();
        assert.equal(await page.evaluate(() => document.readyState), 'interactive');
        assert.deepEqual(requests.map(request => request.path), ['/ACC/native/http'],
          'A connected render waits for document load while HTTP-only shards still POST.');
        const upgrading = new Promise(resolve => server.once('held-upgrade', resolve));
        heldImage.setHeader('Content-Type', 'image/svg+xml');
        heldImage.end('<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>');
        await upgrading;
        await page.getByRole('button', {name: 'Update connected', exact: true}).click();
        await page.getByRole('button', {name: 'Update HTTP', exact: true}).click();
        await page.getByText('HTTP 2', {exact: true}).waitFor();
        assert.deepEqual(requests.map(request => request.path), ['/ACC/native/http', '/ACC/native/http'],
          'A connected render waits for the pending socket handshake.');
        upgrades.shift()();
        await page.getByText('Socket 4', {exact: true}).waitFor();
        assert.equal(runs[0].path, ancestor ? '/ACC/fixture' : '/ACC/native/connected');
        assert.equal(ancestor ? runs[0].run.signals.a : runs[0].run.args[0], 4,
          'Opening the socket reads the newest inputs, including mount initialization.');
        const reconnecting = new Promise(resolve => server.once('held-upgrade', resolve));
        for (const client of ws.clients) client.terminate();
        await reconnecting;
        await page.getByRole('button', {name: 'Update connected', exact: true}).click();
        await page.getByRole('button', {name: 'Update connected', exact: true}).click();
        await page.getByRole('button', {name: 'Update HTTP', exact: true}).click();
        await page.getByText('HTTP 3', {exact: true}).waitFor();
        assert.ok(requests.every(request => request.path === '/ACC/native/http'),
          'Reconnect never falls back to a connected HTTP render.');
        upgrades.shift()();
        await page.getByText('Socket 6', {exact: true}).waitFor();
        assert.equal(ancestor ? runs.at(-1).run.signals.a : runs.at(-1).run.args[0], 6);
        assert.deepEqual(requests.map(request => request.body.args), [[1], [2], [3]]);
        assert.deepEqual(failures, []);
      } finally {
        heldImage?.end();
        await page.close();
        for (const socket of pendingSockets) socket.destroy();
        for (const client of ws.clients) client.terminate();
        await new Promise(resolve => ws.close(resolve));
        await new Promise(resolve => server.close(resolve));
      }
    });
  } finally { await browser.close(); }
});

test('framework procedures, returned surrogates, shards and sockets stay within the Rust-rendered mount', async t => {

  const browser = await launchBrowser();
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
  const browser = await launchBrowser();
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


test('generic keepalive procedures retain mounted cookie transport and ordinary calls', async () => {
  const browser = await launchBrowser();
  try {
    for (const prefix of ['', '/app', '/ACC']) {
      const requests = [];
      const server = http.createServer(async (request, response) => {
        if (request.url === `${prefix}/runtime.js`) {
          response.setHeader('Content-Type', 'text/javascript'); response.end(runtime); return;
        }
        if (request.method === 'POST') {
          const chunks = [];
          for await (const chunk of request) chunks.push(chunk);
          requests.push({path: request.url, body: JSON.parse(Buffer.concat(chunks).toString()), cookie: request.headers.cookie, authorization: request.headers.authorization});
          response.setHeader('Content-Type', 'application/json');
          if (request.url.endsWith('/denied')) {response.statusCode = 403; response.statusMessage = 'Forbidden'; response.end('not JSON');}
          else response.end('true');
          return;
        }
        const run = `async()=>{try{
          const p=cx.hydrate(${JSON.stringify(procedure('/ACC/native/example'))});
          for(const args of [[],[undefined],[cx.hydrate(true),cx.hydrate('hello')]]){
            const f=p.call_keepalive(...args); const value=await f; await f;
            if(!value.v)throw new Error('Hydration');
          }
          const adapted=p.with_keepalive().call(cx.hydrate('adapter'));
          if(!(await adapted).v)throw new Error('Adapter hydration');
          await adapted;
          await p.call(cx.hydrate('ordinary'));
          try {await cx.hydrate(${JSON.stringify(procedure('/native/denied'))}).call_keepalive();throw new Error('Expected denial');}
          catch(error){if(error.message!=='Procedure call failed: 403 Forbidden')throw error;}
          document.querySelector('output').textContent='ok';
        }catch(error){document.querySelector('output').textContent=error.message;}}`;
        response.setHeader('Content-Type', 'text/html');
        response.end(`<html data-topcoat-runtime-prefix="${prefix}"><body><button data-topcoat-on:click="${escape(run)}">Run procedures</button><output></output><script>window.originalFetch=window.fetch;</script><script type="module" src="${prefix}/runtime.js"></script></body></html>`);
      });
      await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
      const context = await browser.newContext();
      try {
        const origin = `http://127.0.0.1:${server.address().port}`;
        await context.addCookies([{name:'lific_session',value:'fixture-session',url:origin}]);
        const page = await context.newPage(), failures = [];
        page.on('pageerror', error => failures.push(error.message));
        await page.goto(`${origin}${prefix}/ACC/issues`);
        await page.getByRole('button', {name:'Run procedures',exact:true}).click();
        await page.waitForFunction(() => document.querySelector('output').textContent !== '');
        assert.equal(await page.locator('output').textContent(), 'ok');
        assert.deepEqual(requests.map(({path,body}) => ({path,body})), [
          ...[[],[null],[true,'hello'],['adapter'],['ordinary']].map(body => ({path:`${prefix}/ACC/native/example`,body})),
          {path:`${prefix}/native/denied`,body:[]},
        ]);
        for(const request of requests){assert.equal(request.cookie,'lific_session=fixture-session');assert.equal(request.authorization,undefined);}
        assert.equal(await page.evaluate(() => window.fetch === window.originalFetch),true);
        assert.deepEqual(failures,[]);
      } finally {
        await context.close();
        await new Promise(resolve => server.close(resolve));
      }
    }
  } finally {await browser.close();}
});

test('packaged vector signal writes preserve typed snapshots and notify subscribers', async () => {
  const bootstrap = 'var Ve=new ne;Ve.start(document);Ve.page.listenForDevRefresh();';
  assert.equal(runtime.split(bootstrap).length - 1, 1);
  const fixtureRuntime = runtime.replace(bootstrap,
    'globalThis.vectorFixture={Context:Z,Registry:te,Effect:$,flush:Ue};');
  const server = http.createServer((request, response) => {
    if (request.url === '/runtime.js') {
      response.setHeader('Content-Type', 'text/javascript'); response.end(fixtureRuntime); return;
    }
    response.setHeader('Content-Type', 'text/html');
    response.end('<html><body><script type="module" src="/runtime.js"></script></body></html>');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const browser = await launchBrowser();
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForFunction(() => !!window.vectorFixture);
    const results = await page.evaluate(() => {
      const {Context, Registry, Effect, flush} = window.vectorFixture;
      return [16,32,64].map(bits => {
        const registry = new Registry(), cx = new Context(registry);
        const vector = value => cx.hydrate({t:'Vec',bits,v:value});
        const index = (value,width=bits,kind='usize') => cx.hydrate({t:kind,bits:width,v:String(value)});
        const before = vector(['same','same']); registry.insert('vector',before);
        const s = cx.signal('vector'), snapshot = s.get();
        let runs=0; const effect=new Effect(()=>{s.get();runs++;});
        try {
          effect.run(); s.push(cx.hydrate('last')); flush();
          const afterPush=s.dehydrate(), pushRuns=runs;
          s.remove(index(0)); flush();
          const afterRemove=s.dehydrate(), removeRuns=runs;
          const errors=[];
          for(const invalid of [index(2),index(0,64,'i64'),index(0,bits===64?32:64)]) {
            try{s.remove(invalid);errors.push(false);}catch{errors.push(true);}
          }
          flush();
          registry.insert('empty',vector([]));
          let emptyRejected=false;try{cx.signal('empty').remove(index(0));}catch{emptyRejected=true;}
          return {bits,before:before.dehydrate(),snapshot:snapshot.dehydrate(),afterPush,afterRemove,pushRuns,removeRuns,runs,errors,emptyRejected,final:s.dehydrate()};
        } finally {effect.dispose();}
      });
    });
    for (const r of results) {
      assert.deepEqual(r.before,{t:'Vec',bits:r.bits,v:['same','same']});
      assert.deepEqual(r.snapshot,r.before);
      assert.deepEqual(r.afterPush,{t:'Signal',id:'vector',v:{t:'Vec',bits:r.bits,v:['same','same','last']}});
      assert.deepEqual(r.afterRemove,{t:'Signal',id:'vector',v:{t:'Vec',bits:r.bits,v:['same','last']}});
      assert.equal(r.pushRuns,2); assert.equal(r.removeRuns,3); assert.equal(r.runs,3);
      assert.deepEqual(r.errors,[true,true,true]); assert.equal(r.emptyRejected,true);
      assert.deepEqual(r.final,r.afterRemove);
    }
  } finally { await browser.close(); await new Promise(resolve=>server.close(resolve)); }
});


test('expression context adapts real keyboard events using the framework Event vocabulary', async () => {
  const handler = `()=>{document.addEventListener('keydown', native => {
    const event=cx.event(native);
    event.prevent_default();
    document.querySelector('output').textContent=JSON.stringify({key:event.key.v,code:event.code.v,
      shift:event.shift_key.v,target:event.target.id.v,value:event.target.value.v,
      prevented:event.default_prevented.v,nativePrevented:native.defaultPrevented});
  },{signal:cx.abortSignal});}`;
  const server=http.createServer((request,response)=>{
    if(request.url==='/runtime.js'){response.setHeader('Content-Type','text/javascript');response.end(runtime);return;}
    response.setHeader('Content-Type','text/html');
    response.end(`<html><body><input id="actual-keyboard-target" value="draft" data-topcoat-on:mount="${escape(handler)}"><output></output><script type="module" src="/runtime.js"></script></body></html>`);
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const browser=await launchBrowser();
  try{
    const page=await browser.newPage(),errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.locator('input').focus();await page.keyboard.press('Shift+Tab');
    await page.waitForFunction(()=>document.querySelector('output').textContent.includes('"key":"Tab"'));
    assert.deepEqual(JSON.parse(await page.locator('output').textContent()),{key:'Tab',code:'Tab',shift:true,target:'actual-keyboard-target',value:'draft',prevented:true,nativePrevented:true});
    assert.equal(await page.evaluate(()=>document.activeElement.id),'actual-keyboard-target');
    assert.deepEqual(errors,[]);
  }finally{await browser.close();await new Promise(resolve=>server.close(resolve));}
});
