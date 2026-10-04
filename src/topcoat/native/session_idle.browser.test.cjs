// Real production idle session lifecycle at root and mounted Home routes.
// Uses only the real production fixture and framework sockets; no mocked auth.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const readline = require('node:readline');
const {mountedProxy, launchBrowser} = require('./browser_fixture.cjs');

const upstream = new URL(process.argv[2]);
const fixture = JSON.parse(process.argv[4]);
const scenario = process.argv[5];
const initialTitle = 'Visible active initial work';
const cookie = (origin, value) => ({name: 'lific_token', value, url: origin, httpOnly: true, sameSite: 'Lax'});

const replies = readline.createInterface({input: process.stdin, terminal: false});
const pending = new Map();
let sequence = 0;
replies.on('line', line => {
  const result = JSON.parse(line), request = pending.get(result.id);
  assert.ok(request, 'The fixture acknowledged a known non-secret control ID.');
  pending.delete(result.id);
  clearTimeout(request.timer);
  request.resolve(result);
});
function control(action, fields = {}) {
  const id = ++sequence;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Fixture control ${action} did not acknowledge.`));
    }, 8000);
    pending.set(id, {resolve, reject, timer});
    process.stdout.write(`@lific-fixture:idle:${JSON.stringify({id, action, ...fields})}\n`);
  });
}
async function receivers(expected, message, sockets) {
  const result = await control('wait_count', {expected, sockets});
  assert.equal(result.receivers, expected, message);
  if (sockets !== undefined) {
    assert.equal(result.viewerSockets + result.replacementSockets, sockets,
      `${message} The fixture's actual server socket permits must also be released.`);
  }
}
async function bounded(operation, message, milliseconds = 7000) {
  let timer;
  try {
    return await Promise.race([
      operation,
      new Promise((_, reject) => {timer = setTimeout(() => reject(new Error(message)), milliseconds);}),
    ]);
  } finally {clearTimeout(timer);}
}

async function privateHome(browser, proxy, prefix, token) {
  const context = await browser.newContext();
  await context.addCookies([cookie(proxy.origin, token)]);
  const page = await context.newPage();
  page.setDefaultTimeout(7000);
  const errors = [], privateRequests = [], frames = [], sockets = [], inputs = [], transportSockets = [];
  const protocol = await context.newCDPSession(page);
  protocol.on('Network.webSocketCreated', event => {
    transportSockets.push({id: event.requestId, url: event.url, closed: false});
  });
  protocol.on('Network.webSocketClosed', event => {
    const socket = transportSockets.find(socket => socket.id === event.requestId);
    if (socket) socket.closed = true;
  });
  await protocol.send('Network.enable');
  await page.exposeFunction('recordIdleSessionInput', event => inputs.push(event));
  await page.addInitScript(() => {
    window.__nativeIdleDocument = crypto.randomUUID();
    for (const type of ['focus', 'storage', 'input', 'keydown', 'pointerdown']) {
      window.addEventListener(type, event => window.recordIdleSessionInput({
        document: window.__nativeIdleDocument, type, trusted: event.isTrusted,
      }), {capture: true});
    }
    // Test-only protocol observation: replay an existing framework request
    // through its existing socket to exercise same-scope server replacement.
    // There is no extra connection, invented render body, or production state.
    const send = WebSocket.prototype.send;
    let contentSocket, lastRender;
    WebSocket.prototype.send = function (value) {
      if (new URL(this.url).pathname.endsWith('/__native_home/content')) {
        contentSocket = this;
        lastRender = value;
      }
      return send.call(this, value);
    };
    window.__replayNativeIdleContent = () => send.call(contentSocket, lastRender);
  });
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => {
    // Login is a separate surface; retain every error while private Home is
    // active and every framework error even if navigation just started.
    if (message.type() === 'error' &&
        (new URL(page.url()).pathname === `${prefix}/` || message.text().startsWith('[topcoat]'))) {
      errors.push(message.text());
    }
  });
  context.on('request', request => {
    if (new URL(page.url()).pathname === `${prefix}/`) privateRequests.push(request.url());
  });
  page.on('websocket', socket => {
    let close;
    const record = {url: socket.url(), closed: false, closing: new Promise(resolve => {close = resolve;})};
    sockets.push(record);
    socket.on('close', () => {record.closed = true; close();});
    socket.on('framereceived', ({payload}) => {
      try {frames.push({url: socket.url(), value: JSON.parse(payload.toString())});}
      catch {errors.push('A native framework socket delivered a non-JSON frame.');}
    });
  });
  await page.goto(`${proxy.origin}${prefix}/`);
  await page.getByText(initialTitle, {exact: true}).waitFor();
  await page.waitForFunction(() =>
    document.querySelector('.tc-native-home__page')?.getAttribute('data-native-home-connected') === 'true' &&
    document.querySelector('.native-home-palette-results')?.getAttribute('data-native-home-connected') === 'true');
  assert.equal(await page.locator('.native-home-account').textContent(), 'viewer');
  assert.equal(await page.getByText('Private hidden initial work', {exact: true}).count(), 0);
  assert.deepEqual(sockets.map(socket => new URL(socket.url).pathname).sort(),
    [`${prefix}/__native_home/content`, `${prefix}/__native_home/palette`].sort(),
    'The content-owned session listener preserves the two existing sibling framework connections.');
  const scripts = await page.locator('script[src]').evaluateAll(elements => elements.map(element => new URL(element.src).pathname));
  assert.deepEqual(scripts, [`${prefix}/__topcoat-runtime.js`], 'Private Home ships only the framework runtime.');
  return {context, page, errors, privateRequests, frames, sockets, inputs, transportSockets};
}

function nativeOnly(state) {
  assert.ok(state.privateRequests.every(url => !new URL(url).pathname.split('/').includes('api')),
    'Native idle session handling makes no REST requests.');
  assert.deepEqual(state.errors, [], 'Revocation is visible navigation, without framework or private-page errors.');
}
async function idleBoundary(state) {
  // A render response establishes the connected live body before the marker.
  // Browser event instrumentation establishes which document stayed idle.
  const document = await state.page.evaluate(() => window.__nativeIdleDocument);
  return {document, inputs: state.inputs.filter(event => event.document === document).length};
}
function noInputAfter(state, boundary) {
  assert.equal(state.inputs.filter(event => event.document === boundary.document).length, boundary.inputs,
    'Retirement required no focus, storage, input, keyboard, or pointer event on the old private document.');
}
function homeDocuments(proxy, prefix) {
  return proxy.requests.filter(request => request.method === 'GET' && request.path === `${prefix}/`).length;
}
async function closed(records, state, stage) {
  try {
    await bounded(Promise.all(records.map(record => record.closing)), `${stage}: retired document sockets did not close.`);
  } catch (error) {
    const server = await control('count');
    throw new Error(`${error.message}\n${JSON.stringify({
      stage, server,
      observedSockets: records.map(({url, closed}) => ({url, closed})),
      transportSockets: state.transportSockets,
    })}`, {cause: error});
  }
  assert.ok(records.every(record => record.closed), 'Every observed socket of the retired document closed.');
}
async function retiredToLogin(state, proxy, prefix, index) {
  const before = homeDocuments(proxy, prefix), boundary = await idleBoundary(state);
  const oldSockets = [...state.sockets];
  await control('revoke', {index});
  await state.page.waitForURL(`${proxy.origin}${prefix}/login`, {timeout: 7000});
  await state.page.waitForLoadState('domcontentloaded');
  assert.ok(state.frames.some(frame => frame.value.t === 'redirect' && frame.value.location === `${prefix}/`),
    'Old connection authority retires to mounted Home for a fresh cookie-authoritative HTTP decision.');
  assert.equal(homeDocuments(proxy, prefix), before + 1, 'Retirement makes one fresh Home document request.');
  assert.equal(await state.page.locator('[data-native-home]').count(), 0);
  assert.equal(await state.page.getByText(initialTitle, {exact: true}).count(), 0);
  noInputAfter(state, boundary);
  await closed(oldSockets, state, 'revoked document navigation');
  await receivers(0, 'Document retirement drops the content-owned revocation receiver.');
  nativeOnly(state);
}

test(`native idle session production ${scenario}`, async t => {
  assert.ok(['idle', 'unrelated', 'replacement', 'lifetime'].includes(scenario));
  const browser = await launchBrowser();
  try {
    for (const [index, prefix] of ['', '/app', '/ACC'].entries()) {
      await t.test(prefix || 'root', async () => {
        const proxy = await mountedProxy(upstream, prefix);
        let state;
        try {
          await control('rename', {title: initialTitle});
          await receivers(0, 'Each isolated browser context starts without a revocation receiver.');
          state = await privateHome(browser, proxy, prefix, fixture.tokens[index]);
          if (scenario === 'idle') {
            await retiredToLogin(state, proxy, prefix, index);
          } else if (scenario === 'unrelated') {
            const before = homeDocuments(proxy, prefix), boundary = await idleBoundary(state);
            const navigated = state.page.waitForEvent('domcontentloaded', {timeout: 500}).then(() => true, () => false);
            await control('unrelated');
            assert.equal(await navigated, false, 'An unrelated user broadcast cannot reload this private Home.');
            assert.equal(homeDocuments(proxy, prefix), before);
            assert.equal(await state.page.locator('.native-home-account').textContent(), 'viewer');
            assert.equal(await state.page.getByText(initialTitle, {exact: true}).count(), 1);
            assert.ok(state.sockets.every(socket => !socket.closed), 'Unrelated signals preserve both live connections.');
            assert.equal(state.frames.some(frame => frame.value.t === 'redirect'), false);
            noInputAfter(state, boundary);
            // Matching canary proves the same listener remains usable.
            await retiredToLogin(state, proxy, prefix, index);
          } else if (scenario === 'replacement') {
            const before = homeDocuments(proxy, prefix), boundary = await idleBoundary(state);
            const oldSockets = [...state.sockets];
            // HttpOnly browser primitive only: no storage or focus event is sent.
            await state.context.addCookies([cookie(proxy.origin, fixture.replacementToken)]);
            const replaced = state.page.waitForEvent('domcontentloaded', {timeout: 7000});
            await control('revoke', {index});
            await replaced;
            await state.page.getByText('Private hidden initial work', {exact: true}).waitFor();
            assert.equal(state.page.url(), `${proxy.origin}${prefix}/`);
            assert.equal(await state.page.locator('.native-home-account').textContent(), 'admin');
            assert.ok(state.frames.some(frame => frame.value.t === 'redirect' && frame.value.location === `${prefix}/`),
              'A stale A socket retires to mounted Home rather than forcing B to login.');
            assert.equal(state.frames.some(frame => frame.value.t === 'redirect' && frame.value.location === `${prefix}/login`), false);
            assert.equal(homeDocuments(proxy, prefix), before + 1);
            noInputAfter(state, boundary);
            await closed(oldSockets, state, 'replacement cookie navigation');
            await receivers(1, 'The replacement document owns one current-account revocation receiver.');
            nativeOnly(state);
          } else {
            await receivers(1, 'One connected content render owns one revocation receiver.', 2);
            const initialSockets = [...state.sockets];
            for (let round = 1; round <= 3; round++) {
              const title = `Visible idle rerender ${index}-${round}`;
              await control('rename', {title});
              await state.page.evaluate(() => window.__replayNativeIdleContent());
              await state.page.getByText(title, {exact: true}).waitFor();
              await receivers(1, 'Replacing the existing connected render retires its previous receiver.', 2);
              assert.equal(state.sockets.length, 2, 'Same-scope replacement creates no extra connection.');
              assert.ok(initialSockets.every(socket => !socket.closed));
            }
            await state.page.reload();
            const reloaded = await control('count');
            process.stdout.write(`Idle lifetime document reload resources: ${JSON.stringify(reloaded)}\n`);
            await closed(initialSockets, state, 'document reload');
            await state.page.getByText(`Visible idle rerender ${index}-3`, {exact: true}).waitFor();
            await state.page.waitForFunction(() =>
              document.querySelector('.tc-native-home__page')?.getAttribute('data-native-home-connected') === 'true' &&
              document.querySelector('.native-home-palette-results')?.getAttribute('data-native-home-connected') === 'true');
            await receivers(1, 'A replacement document releases its predecessor and owns one receiver.', 2);
            const currentSockets = state.sockets.filter(socket => !socket.closed);
            assert.deepEqual(currentSockets.map(socket => new URL(socket.url).pathname).sort(),
              [`${prefix}/__native_home/content`, `${prefix}/__native_home/palette`].sort());
            nativeOnly(state);
            await state.context.close();
            // Destroying the context also destroys its DevTools observer; the
            // production hub remains available to prove both sockets and the
            // content receiver were released. Reload above keeps its observer
            // alive and still requires every predecessor socket close event.
            await receivers(0, 'Disconnect drops the last live receiver without a broadcast.', 0);
            state = null;
          }
        } finally {
          try {
            if (state) {
              try {nativeOnly(state);}
              finally {
                await state.context.close();
                await receivers(0, 'Closing the isolated context releases all native session receivers.', 0);
              }
            }
          } finally {await proxy.close();}
        }
      });
    }
  } finally {
    try {await browser.close();}
    finally {
      replies.close();
      process.stdin.destroy();
      for (const request of pending.values()) clearTimeout(request.timer);
      pending.clear();
    }
  }
});
