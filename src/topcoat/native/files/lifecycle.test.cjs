'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder, isDeepStrictEqual} = require('node:util');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const listeners = new Map();
const timers = new Map();
const archivedTimers = new Map();
const abortListeners = [];
let nextTimer = 1;
const abortSignal = {
  aborted: false,
  addEventListener(name, callback) {
    if (name === 'abort') abortListeners.push(callback);
  },
};
function listen(target, name, callback, options) {
  const key = `${target}:${name}`;
  listeners.set(key, callback);
  options?.signal?.addEventListener('abort', () => listeners.delete(key));
}
const context = {
  TextEncoder,
  TextDecoder,
  queueMicrotask,
  setTimeout(callback, delay) {
    assert.ok(delay > 0, 'recovery has a bounded positive deadline');
    const id = nextTimer++;
    timers.set(id, callback);
    archivedTimers.set(id, callback);
    return id;
  },
  clearTimeout: id => timers.delete(Number(id)),
  document: {
    hidden: false,
    documentElement: {getAttribute: () => '/app'},
    addEventListener: (name, callback, options) => listen('document', name, callback, options),
  },
  window: {
    addEventListener: (name, callback, options) => listen('window', name, callback, options),
  },
};
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(runtime.split(bootstrap).length - 1, 1, 'packaged runtime bootstrap');
const eventClass = runtime.match(/event\(\w+\)\{return new (\w+)\(\w+\)\}/)?.[1];
assert.ok(eventClass, 'packaged Event surrogate');
vm.runInNewContext(runtime.replace(bootstrap,
  `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass}};`), context);
const registry = new context.fixture.Registry();
const cx = Object.assign(new context.fixture.Context(registry), {abortSignal});
const writes = new Map();
for (const [id, value] of Object.entries(input.signals)) {
  registry.insert(id, cx.hydrate(value));
  const cell = registry.handle(id);
  const set = cell.set;
  cell.set = next => {
    writes.set(id, (writes.get(id) ?? 0) + 1);
    return set(next);
  };
}
function scalar(id) {
  const value = cx.signal(id).get();
  const wire = value && typeof value.dehydrate === 'function' ? value.dehydrate() : value;
  return wire && typeof wire === 'object' && 'v' in wire ? wire.v : wire;
}
function snapshot() {
  return Object.fromEntries(Object.keys(input.signals).map(id => [id, scalar(id)]));
}
function changed(before, after) {
  return Object.keys(before).filter(id => !isDeepStrictEqual(before[id], after[id]));
}
function run(source, event = {}) {
  assert.equal(typeof source, 'string', 'the mounted response exposes its handler');
  return vm.runInNewContext(`cx => (${source})`, context)(cx)(new context.fixture.Event(event));
}
function fire(id) {
  const callback = timers.get(id);
  assert.equal(typeof callback, 'function', 'current recovery timer exists');
  timers.delete(id);
  callback();
}

run(input.mountHandler);
run(input.initialComplete);
const before = snapshot();
run(input.click);
const loading = snapshot();
const loadingId = changed(before, loading).find(id => before[id] === false && loading[id] === true);
assert.ok(loadingId, 'Load more immediately marks its request busy');
const offsetId = changed(before, loading).find(id => String(before[id]) === '0' && String(loading[id]) === '50');
assert.ok(offsetId, 'Load more advances to the second 50-row page');
assert.equal(timers.size, 1, 'append arms one recovery timer');
const appendTimer = [...timers.keys()][0];
const loadingWrites = Object.fromEntries(writes);
run(input.click);
assert.deepEqual(Object.fromEntries(writes), loadingWrites,
  'a second click while loading cannot enqueue another append');

if (input.phase === 'start') {
  const signals = Object.fromEntries(Object.keys(input.signals).map(id => [
    id, cx.signal(id).dehydrate().v,
  ]));
  process.stdout.write(JSON.stringify({signals}));
} else if (input.phase === 'query') {
  run(input.sortHandler, {target: {value: 'filename'}});
  const sorted = snapshot();
  assert.ok(Object.values(sorted).includes('filename'), 'sort changes the query');
  assert.equal(sorted[loadingId], false, 'sort replaces prior append loading state');
  assert.equal(String(sorted[offsetId]), '0', 'new sort starts at page one');
  assert.equal(timers.has(appendTimer), false, 'new query retires previous append timer');
  run(input.completion);
  assert.deepEqual(snapshot(), sorted, 'prior append completion cannot overwrite a new query');
  archivedTimers.get(appendTimer)();
  assert.deepEqual(snapshot(), sorted, 'prior append timeout cannot affect a new query');
} else {
  const focus = listeners.get('window:focus');
  assert.equal(typeof focus, 'function', 'Files installs scoped focus refresh');
  focus();
  const pending = snapshot();
  const pendingIds = changed(loading, pending);
  assert.equal(pendingIds.length, 1, 'focus queues refresh without changing offset or revision');
  const pendingId = pendingIds[0];
  assert.equal(pending[pendingId], true);
  const pendingWrites = Object.fromEntries(writes);
  focus();
  assert.deepEqual(Object.fromEntries(writes), pendingWrites, 'repeated focus coalesces into one refresh');

  run(input.completion);
  const refreshing = snapshot();
  assert.equal(refreshing[loadingId], false, 'append completion releases loading');
  assert.equal(refreshing[pendingId], false, 'append completion consumes queued refresh');
  assert.equal(String(refreshing[offsetId]), '0', 'queued refresh starts at page one');
  const refreshingId = changed(pending, refreshing).find(id =>
    pending[id] === false && refreshing[id] === true);
  assert.ok(refreshingId, 'queued refresh marks itself busy');
  assert.equal(timers.has(appendTimer), false, 'completion clears append recovery timer');
  assert.equal(timers.size, 1, 'queued refresh has its own recovery timer');
  const refreshTimer = [...timers.keys()][0];
  archivedTimers.get(appendTimer)();
  assert.deepEqual(snapshot(), refreshing, 'stale timeout cannot change newer request state');
  run(input.completion);
  assert.deepEqual(snapshot(), refreshing, 'stale completion cannot clear or overwrite newer state');

  fire(refreshTimer);
  const recovered = snapshot();
  assert.equal(recovered[refreshingId], false, 'transport timeout releases refresh busy state');
  assert.equal(String(recovered[offsetId]), '0', 'transport timeout preserves retry offset');
  assert.ok(Object.values(recovered).some(value => typeof value === 'string'
    && value.includes("Couldn't refresh files")), 'transport timeout exposes a retryable error');
  const historyId = Object.keys(recovered).find(id => {
    if (typeof recovered[id] !== 'string' || !recovered[id].startsWith('[{')) return false;
    return JSON.parse(recovered[id]).length === 51;
  });
  assert.ok(historyId, 'completion retains all 51 confirmed rows through transport recovery');
  run(input.retry);
  const retrying = snapshot();
  assert.equal(String(retrying[offsetId]), '0', 'retry keeps failed request offset');
  assert.equal(retrying[historyId], recovered[historyId], 'retry keeps confirmed rows');
  assert.equal(timers.size, 1, 'retry arms a new recovery deadline');
  assert.ok(changed(recovered, retrying).length >= 3, 'retry starts a new request generation');
  assert.ok(abortListeners.length > 0, 'request cleanup is scoped to page lifetime');
  abortSignal.aborted = true;
  for (const callback of abortListeners) callback();
  assert.equal(timers.size, 0, 'page retirement clears recovery timers');
  assert.equal(listeners.has('window:focus'), false, 'page retirement removes focus listener');
}
