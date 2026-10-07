'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');

const source = fs.readFileSync(path.join(__dirname, '../assets/runtime.js'), 'utf8');
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(source.split(bootstrap).length - 1, 1);

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((accept, refuse) => {resolve = accept; reject = refuse;});
  return {promise, resolve, reject};
};
const settle = async () => {for (let i = 0; i < 30; i++) await Promise.resolve();};

function fixture() {
  const sockets = [], redirects = [], requests = [], errors = [];
  const document = Object.assign(new EventTarget(), {
    documentElement: {getAttribute: () => ''}, activeElement: null, readyState: 'complete',
  });
  const location = {href: 'https://example.test/settings', origin: 'https://example.test',
    pathname: '/settings', search: '',
    assign: value => redirects.push(String(value)), replace: value => redirects.push(String(value))};
  const context = {
    TextEncoder, TextDecoder, URL, queueMicrotask, setTimeout, clearTimeout,
    AbortController, DOMException, Event, EventTarget, Response, Headers,
    document, location, window: Object.assign(new EventTarget(), {location}),
    fetch(url, options) {
      const response = deferred();
      requests.push({url, options, ...response});
      // Deliberately allow a canceled request to return: the runtime must
      // reject stale frames even when cancellation loses a network race.
      return response.promise;
    },
  };
  vm.runInNewContext(source.replace(bootstrap,
    'globalThis.fixture={Runtime:ye,Connection:me,Unit:_,Scope:E};'), context);
  const runtime = new context.fixture.Runtime();
  runtime.reportError = error => errors.push(error);
  runtime.connection.open = () => {
    const listeners = {}, sent = [];
    const socket = {readyState: 0, sent, closed: false,
      addEventListener: (name, listener) => {listeners[name] = listener;},
      send: value => sent.push(JSON.parse(value)),
      close() {this.closed = true;},
      opened() {this.readyState = 1; listeners.open();},
      receive(frame) {listeners.message({data: JSON.stringify(frame)});},
    };
    sockets.push(socket);
    return socket;
  };
  const owner = new AbortController();
  const cx = Object.assign(Object.create(runtime.context), {abortSignal: owner.signal});
  const member = name => ({name, label: 'Shard',
    rerunRequest: () => ({url: `/__native_${name}`, headers: {}, body: '{}'}),
    connectionOpened() {runtime.connection.run(this);},
    reportError: error => errors.push(error),
  });
  function unit(name = 'render') {
    const applied = [];
    class TestUnit extends context.fixture.Unit {
      label = 'Shard';
      readInputs() {}
      rerunRequest() {return {url: `/__native_${name}`, headers: {}, body: '{}'};}
      replaceContent(html) {applied.push(html);}
      applySwap(_region, html) {applied.push(html);}
    }
    return {unit: new TestUnit(runtime.page.contentScope, runtime), applied};
  }
  const requireBridge = () => assert.equal(typeof cx.withSessionChange, 'function',
    'the packaged expression Context exposes the generic native session-change bridge');
  return {runtime, cx, owner, sockets, redirects, requests, errors, member, unit, requireBridge};
}

test('suspending a connection retains members and rejects retired physical socket frames', () => {
  const f = fixture(), connection = f.runtime.connection;
  const first = f.member('first'), second = f.member('second');
  connection.join(first);
  f.sockets[0].opened();
  assert.equal(typeof connection.suspend, 'function', 'Connection has a membership-preserving pause');
  assert.equal(typeof connection.resume, 'function');
  connection.suspend();
  assert.equal(f.sockets[0].closed, true);
  assert.equal(connection.members.has(first), true, 'pause does not use the page-reset operation');
  connection.join(second);
  assert.equal(connection.run(second), false);
  f.sockets[0].receive({frame: {t: 'redirect', location: '/retired'}});
  assert.equal(f.sockets.length, 1, 'joining while paused cannot open a socket with the old cookie');
  assert.deepEqual(f.redirects, []);
  connection.resume();
  assert.equal(f.sockets.length, 2);
  f.sockets[1].opened();
  assert.deepEqual(f.sockets[1].sent.map(frame => frame.path), ['/__native_first', '/__native_second']);
  connection.reset();
});

test('session-changing tasks retain scopes and signals until the fresh connection resumes', async () => {
  const f = fixture();
  f.requireBridge();
  const pending = deferred(), first = f.member('first'), second = f.member('second');
  f.runtime.connection.join(first);
  f.sockets[0].opened();
  const scope = f.runtime.page.contentScope;
  f.runtime.registry.insert('draft', f.cx.hydrate('keep this draft'));
  const change = f.cx.withSessionChange(f.owner.signal, async () => {
    assert.equal(f.sockets[0].closed, true, 'pause precedes the native mutation request');
    await pending.promise;
    f.cx.signal('draft').set(f.cx.hydrate('canonical result'));
    return 'saved';
  });
  await settle();
  f.runtime.connection.join(second);
  assert.equal(f.sockets.length, 1);
  assert.equal(scope.isDisposed, false, 'transport suspension must not release the page owner');
  assert.equal(f.cx.signal('draft').dehydrate().v, 'keep this draft');
  pending.resolve();
  assert.equal(await change, 'saved');
  assert.equal(f.runtime.page.contentScope, scope);
  assert.equal(f.cx.signal('draft').dehydrate().v, 'canonical result');
  assert.equal(f.sockets.length, 2);
  f.runtime.connection.reset();
});

test('rotation cancels old HTTP renders and coalesces new shard refreshes until release', async () => {
  const f = fixture();
  f.requireBridge();
  const {unit, applied} = f.unit(), pending = deferred();
  const oldRender = unit.refresh();
  assert.equal(f.requests.length, 1);
  const change = f.cx.withSessionChange(f.owner.signal, () => pending.promise);
  await settle();
  assert.equal(f.requests[0].options.signal.aborted, true, 'old-cookie HTTP renders are canceled too');
  await unit.refresh();
  await unit.refresh();
  assert.equal(f.requests.length, 1, 'a paused non-connected shard cannot fall through to fetch');
  f.requests[0].resolve(new Response('{"t":"redirect","location":"/old-cookie"}\n', {
    headers: {'content-type': 'application/x-ndjson'},
  }));
  await oldRender;
  assert.deepEqual(f.redirects, []);
  assert.deepEqual(applied, []);
  pending.resolve();
  await change;
  await settle();
  assert.equal(f.requests.length, 2, 'pending refreshes coalesce and replay under the current cookie');
  f.requests[1].resolve(new Response('{"t":"snapshot","html":"fresh connection list"}\n', {
    headers: {'content-type': 'application/x-ndjson'},
  }));
  await settle();
  assert.deepEqual(applied, ['fresh connection list']);
  unit.dispose();
  f.runtime.connection.reset();
});

test('rotation invalidates old navigation and defers new navigation requests', async () => {
  const f = fixture();
  f.requireBridge();
  const oldNavigation = f.runtime.navigation.navigate(new URL('https://example.test/old'), 'push');
  assert.equal(f.requests.length, 1);
  const pending = deferred();
  const change = f.cx.withSessionChange(f.owner.signal, () => pending.promise);
  await settle();
  assert.equal(f.requests[0].options.signal.aborted, true);
  const nextNavigation = f.runtime.navigation.navigate(new URL('https://example.test/next'), 'push');
  await settle();
  assert.equal(f.requests.length, 1, 'navigation cannot request old-cookie HTML during the change');
  f.requests[0].resolve(new Response('old document', {headers: {'content-type': 'text/html'}}));
  await oldNavigation;
  assert.deepEqual(f.redirects, [], 'cancellation must also invalidate the navigation fallback load');
  pending.resolve();
  await change;
  await settle();
  assert.equal(f.requests.length, 2);
  f.requests[1].resolve(new Response('fresh document', {headers: {'content-type': 'text/html'}}));
  await nextNavigation;
  assert.deepEqual(f.redirects, ['https://example.test/next']);
  f.runtime.connection.reset();
});

test('session changes serialize and disposed queued owners cannot send mutations', async () => {
  const f = fixture();
  f.requireBridge();
  const pending = deferred(), secondOwner = new AbortController();
  let calls = 0;
  const first = f.cx.withSessionChange(f.owner.signal, () => pending.promise);
  const second = f.cx.withSessionChange(secondOwner.signal, async () => {calls++;});
  const refused = assert.rejects(second, {name: 'AbortError'});
  secondOwner.abort();
  await settle();
  assert.equal(calls, 0);
  pending.resolve();
  await first;
  await refused;
  assert.equal(calls, 0);
});

test('task failure always releases the shared transport gate', async () => {
  const f = fixture();
  f.requireBridge();
  f.runtime.connection.join(f.member('live'));
  f.sockets[0].opened();
  const failure = new Error('native procedure failed');
  await assert.rejects(f.cx.withSessionChange(f.owner.signal, async () => {throw failure;}),
    error => error === failure);
  assert.equal(f.sockets.length, 2, 'a rejected request cannot leave the native app suspended');
  assert.equal(await f.cx.withSessionChange(f.owner.signal, async () => 'next'), 'next');
  f.runtime.connection.reset();
});

test('an active disposed owner cannot reconnect before its mutation settles', async () => {
  const f = fixture();
  f.requireBridge();
  f.runtime.connection.join(f.member('live'));
  f.sockets[0].opened();
  const pending = deferred();
  const change = f.cx.withSessionChange(f.owner.signal, () => pending.promise);
  await settle();
  f.owner.abort();
  await settle();
  assert.equal(f.sockets.length, 1, 'the in-flight request can still rotate the cookie after owner disposal');
  assert.equal(f.runtime.connection.suspended, true);
  pending.resolve();
  await change;
  assert.equal(f.sockets.length, 2);
  f.runtime.connection.reset();
});

test('a rejected session change does not prevent a queued live owner from running', async () => {
  const f = fixture();
  f.requireBridge();
  const pending = deferred(), failure = new Error('first task failed');
  const first = f.cx.withSessionChange(f.owner.signal, () => pending.promise);
  const refused = assert.rejects(first, error => error === failure);
  let calls = 0;
  const second = f.cx.withSessionChange(f.owner.signal, async () => ++calls);
  await settle();
  assert.equal(calls, 0);
  pending.reject(failure);
  await refused;
  assert.equal(await second, 1);
  assert.equal(f.runtime.sessionChange, null);
});

test('reconnection failure releases waiters and reports transport failure without losing a saved result', async () => {
  const f = fixture();
  f.requireBridge();
  f.runtime.connection.join(f.member('live'));
  f.sockets[0].opened();
  const pending = deferred(), failure = new Error('WebSocket open failed');
  const change = f.cx.withSessionChange(f.owner.signal, () => pending.promise);
  await settle();
  const gate = f.runtime.sessionChange;
  let released = false;
  gate.done.then(() => {released = true;});
  f.runtime.connection.open = () => {throw failure;};
  pending.resolve('saved');
  assert.equal(await change, 'saved');
  await settle();
  assert.equal(released, true);
  assert.equal(f.runtime.sessionChange, null);
  assert.deepEqual(f.errors, [failure]);
  assert.notEqual(f.runtime.connection.retry, null, 'normal transport backoff handles reconnect failure');
  f.runtime.connection.reset();
});

test('setup failure still releases the gate and permits a later native mutation', async () => {
  const f = fixture();
  f.requireBridge();
  const {unit} = f.unit(), failure = new Error('render cancellation failed');
  const cancel = unit.requestController.cancel;
  let gate;
  unit.requestController.cancel = () => {gate = f.runtime.sessionChange; throw failure;};
  let calls = 0;
  await assert.rejects(f.cx.withSessionChange(f.owner.signal, async () => {calls++;}),
    error => error === failure);
  assert.equal(calls, 0);
  assert.equal(f.runtime.sessionChange, null);
  assert.equal(f.runtime.connection.suspended, false);
  await gate.done;
  unit.requestController.cancel = cancel;
  assert.equal(await f.cx.withSessionChange(f.owner.signal, async () => 'next'), 'next');
  unit.dispose();
});

test('cleanup failure reports separately and preserves the native task error', async () => {
  const f = fixture();
  f.requireBridge();
  f.runtime.connection.join(f.member('live'));
  f.sockets[0].opened();
  const pending = deferred(), nativeFailure = new Error('mutation failed'), transportFailure = new Error('open failed');
  const change = f.cx.withSessionChange(f.owner.signal, () => pending.promise);
  const refused = assert.rejects(change, error => error === nativeFailure);
  await settle();
  const gate = f.runtime.sessionChange;
  f.runtime.connection.open = () => {throw transportFailure;};
  pending.reject(nativeFailure);
  await refused;
  await gate.done;
  assert.deepEqual(f.errors, [transportFailure]);
  f.runtime.connection.reset();
});

test('terminal redirects finish a session change without reopening a revoked connection', async () => {
  const f = fixture();
  f.requireBridge();
  f.runtime.connection.join(f.member('live'));
  f.sockets[0].opened();
  assert.equal(typeof f.cx.redirect, 'function', 'native expressions use the runtime terminal redirect claim');
  await f.cx.withSessionChange(f.owner.signal, async () => {f.cx.redirect('/login');});
  assert.deepEqual(f.redirects, ['/login']);
  assert.equal(f.sockets.length, 1);
  f.runtime.connection.scheduleReconnect();
  assert.equal(f.runtime.connection.suspended, true);
  assert.equal(f.runtime.connection.retry, null, 'terminal navigation cannot schedule a retired-cookie reconnect');
  f.runtime.connection.reset();
});

test('a page reset during suspension cannot reopen the retired connection', () => {
  const f = fixture(), connection = f.runtime.connection;
  connection.join(f.member('old'));
  f.sockets[0].opened();
  connection.suspend();
  connection.reset();
  connection.join(f.member('new'));
  assert.equal(f.sockets.length, 1);
  connection.resume();
  f.sockets[1].opened();
  assert.deepEqual(f.sockets[1].sent.map(frame => frame.path), ['/__native_new']);
  connection.reset();
});
