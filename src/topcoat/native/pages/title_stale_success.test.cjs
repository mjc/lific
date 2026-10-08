'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const unbox = value => { while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v; return value; };
const hasSeq = (value, target) => value?.t === 'i64' && String(value.v) === String(target);
const calls = [];
let settle;
const fixture = handlerFixture(input.signals, (url, options) => {
  calls.push({url: new URL(url, 'http://localhost').pathname, body: JSON.parse(options.body)});
  return new Promise(resolve => { settle = () => resolve({ok: true, json: async () => input.saved}); });
}, input.browser_source);
const {cx, context, handler} = fixture;
context.CustomEvent = class { constructor(type, options = {}) { this.type = type; Object.assign(this, options); } };
context.document.documentElement.getAttribute = name => name === 'data-topcoat-runtime-prefix' ? input.mount : '';
context.document.getElementById = () => ({focus() {}});
const signalIds = new Set();
const originalSignal = cx.signal.bind(cx);
cx.signal = id => { signalIds.add(id); return originalSignal(id); };
const signalFor = source => {
  const before = new Set(signalIds);
  vm.runInNewContext(`cx => (${source})`, context)(cx);
  const ids = [...signalIds].filter(id => !before.has(id));
  assert.equal(ids.length, 1);
  return ids[0];
};
const titleId = signalFor(input.title_binding);
const bodyId = signalFor(input.body_binding);
const saveBusyId = signalFor(input.saving_binding);
const fire = (source, event) => handler(source)(cx.event(event));
const text = value => ({type: 'input', target: {value}, cancelable: true, preventDefault() {}});
const key = name => ({type: 'keydown', key: name, cancelable: true, preventDefault() {}});
const flush = async () => { for (let i = 0; i < 60; i++) await Promise.resolve(); };
const snapshot = () => Object.fromEntries(Object.keys(input.signals).map(id => [id, cx.signal(id).get().dehydrate()]));
const read = source => {
  const value = vm.runInNewContext(`cx => (${source})`, context)(cx);
  return unbox(value && typeof value.dehydrate === 'function' ? value.dehydrate() : value);
};
(async () => {
  const fireHandler = source => handler(source)(cx.event({type: 'click', cancelable: true, preventDefault() {}}));
  fireHandler(input.trigger);
  fire(input.editor.input, text('Sent title'));
  const beforeStart = snapshot();
  fire(input.editor.keydown, key('Enter'));
  const afterStart = snapshot();
  const busyIds = Object.keys(input.signals).filter(id =>
    id !== saveBusyId && unbox(beforeStart[id]) === false && unbox(afterStart[id]) === true);
  assert.equal(busyIds.length, 1, 'starting this title write sets one shared mutation busy signal');
  const busyId = busyIds[0];
  const seqIds = Object.entries(input.signals)
    .filter(([id, value]) => signalIds.has(id) && hasSeq(value, input.initial_seq))
    .map(([id]) => id);
  assert.equal(seqIds.length, 1, 'the actual title handler reads one canonical Page sequence signal');
  const seqId = seqIds[0];
  for (let attempt = 0; attempt < 60 && !settle; attempt++) await Promise.resolve();
  assert.equal(calls.length, 1, 'title save is pending');
  fireHandler(input.trigger);
  fire(input.editor.input, text('Newer title draft'));
  fire(input.body_input, text('Dirty body draft'));
  cx.signal(seqId).set(cx.hydrate(input.newer_seq));
  const before = snapshot();
  settle();
  await flush();
  const after = snapshot();
  for (const id of Object.keys(input.signals)) {
    if (id !== busyId && id !== saveBusyId) {
      assert.deepEqual(after[id], before[id], `stale reply preserves signal ${id}`);
    }
  }
  assert.equal(unbox(after[busyId]), false, 'stale completion releases page busy state');
  assert.equal(unbox(after[saveBusyId]), false, 'stale completion releases Save feedback');
  assert.equal(read(input.title_binding), 'Newer title draft');
  assert.equal(read(input.body_binding), 'Dirty body draft');
  assert.equal(read(input.title_hidden_binding), false, 'newer title editor remains open');
  assert.equal(calls[0].url, `${input.mount}/__native_pages/save_title`);
  assert.equal(Object.keys(calls[0].body).length, 4, 'stale check follows a sparse title request');
  process.stdout.write(JSON.stringify({passed: true}));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
