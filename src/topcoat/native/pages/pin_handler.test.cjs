'use strict';

// Execute the emitted page pin handler in the packaged Topcoat runtime.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
let finishRequest;
const fixture = handlerFixture(input.signals, (url, options) => {
  const path = new URL(url, 'http://localhost').pathname;
  assert.ok(path.endsWith('/__native_pages/pin'), `unexpected procedure ${path}`);
  assert.equal(options.method, 'POST');
  requests.push(JSON.parse(options.body));
  if (input.scenario === 'capture' || input.scenario === 'retired') {
    return new Promise(resolve => { finishRequest = () => resolve({ok: true, json: async () => input.reply}); });
  }
  if (input.scenario === 'transport_failure') return Promise.reject(new Error('offline'));
  return Promise.resolve({ok: true, json: async () => input.reply});
}, input.browser_source);
const {cx, context, controller} = fixture;
const referencedSignals = new Set();
const signal = cx.signal.bind(cx);
cx.signal = id => {
  referencedSignals.add(id);
  return signal(id);
};
const pinSignal = (() => {
  const before = new Set(referencedSignals);
  vm.runInNewContext(`cx => (${input.pin_binding})`, context)(cx);
  const ids = [...referencedSignals].filter(id => !before.has(id));
  assert.equal(ids.length, 1, 'the emitted pin binding resolves one owned signal');
  return ids[0];
})();
const sequenceSignal = () => {
  const candidates = [...referencedSignals].filter(id => id !== pinSignal &&
    String(unbox(input.signals[id])) === String(input.expected_seq));
  assert.equal(candidates.length, 1, 'the emitted pin handler shares the page sequence signal');
  return candidates[0];
};
const titleSignal = (() => {
  const before = new Set(referencedSignals);
  vm.runInNewContext(`cx => (${input.title_binding})`, context)(cx);
  return [...referencedSignals].find(id => !before.has(id));
})();
const bodySignal = (() => {
  const before = new Set(referencedSignals);
  vm.runInNewContext(`cx => (${input.body_binding})`, context)(cx);
  return [...referencedSignals].find(id => !before.has(id));
})();
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const value = id => unbox(cx.signal(id).dehydrate());
const handler = fixture.handler(input.handler);
if (input.scenario !== 'capture') {
  cx.signal(titleSignal).set(cx.hydrate('Unsaved title draft'));
  cx.signal(bodySignal).set(cx.hydrate('Unsaved body draft'));
}
handler(cx.event({type: 'click', target: {}}));

async function run() {
  const disposed = handlerFixture(input.signals, () => {
    assert.fail('a disposed pin control must not submit a request');
  }, input.browser_source);
  disposed.controller.abort();
  const snapshot = () => Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, disposed.cx.signal(id).dehydrate()]));
  const before = snapshot();
  disposed.handler(input.handler)(disposed.cx.event({type: 'click', target: {}}));
  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
  assert.deepEqual(snapshot(), before, 'a disposed pin control cannot change shared editor state');
  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
  assert.equal(requests.length, 1, 'one pin click sends one typed procedure request');
  if (input.scenario === 'capture') {
    process.stdout.write(JSON.stringify({arguments: requests[0]}));
    return;
  }
  if (input.scenario === 'retired') {
    assert.equal(typeof finishRequest, 'function', 'the production reply is pending');
    const stateBeforeDispose = Object.fromEntries(Object.keys(input.signals)
      .map(id => [id, value(id)]));
    controller.abort();
    finishRequest();
    for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
    assert.deepEqual(Object.fromEntries(Object.keys(input.signals)
      .map(id => [id, value(id)])), stateBeforeDispose,
    'a retired page owner ignores the late pin response');
  } else if (input.scenario === 'success') {
    assert.equal(String(value(pinSignal)), String(unbox(input.reply.v.pinned)),
      'the successful server value is published');
    assert.equal(value(sequenceSignal()), unbox(input.reply.v.seq),
      'success adopts the sequence from the typed production reply');
    assert.equal(value(titleSignal), 'Unsaved title draft');
    assert.equal(value(bodySignal), 'Unsaved body draft');
  } else if (input.scenario === 'conflict') {
    assert.equal(String(value(pinSignal)), 'false',
      'a conflict restores the current local pin value');
    assert.equal(value(sequenceSignal()), String(input.expected_seq),
      'a conflict never adopts the unseen remote sequence');
    assert.ok([...referencedSignals].some(id => {
      const current = value(id);
      return typeof current === 'string' && current.includes('changed elsewhere');
    }), 'the conflict is visible while the editor is closed');
  } else if (input.scenario === 'transport_failure') {
    assert.equal(String(value(pinSignal)), 'false',
      'a transport failure restores the local pin value');
    assert.equal(value(sequenceSignal()), String(input.expected_seq),
      'a transport failure cannot advance the sequence');
    assert.ok([...referencedSignals].some(id => {
      const current = value(id);
      return typeof current === 'string' && current.includes("Couldn't save");
    }), 'the transport failure remains visible');
  }
  process.stdout.write(JSON.stringify({requests: requests.length}));
}
run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
