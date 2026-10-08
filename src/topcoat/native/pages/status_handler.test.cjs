'use strict';

// Execute the emitted handler with the packaged Topcoat runtime. Rust sends
// captured typed procedure arguments and real StatusOutcome replies through
// the authenticated production route.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
let finishRequest;
const fixture = handlerFixture(input.signals, (url, options) => {
    const path = new URL(url, 'http://localhost').pathname;
    assert.ok(path.endsWith('/__native_pages/status'), `unexpected procedure ${path}`);
    assert.equal(options.method, 'POST');
    requests.push(JSON.parse(options.body));
    if (input.scenario === 'capture' || input.scenario === 'retired') {
      return new Promise(resolve => { finishRequest = () => resolve({ok: true, json: async () => input.reply}); });
    }
    if (input.scenario === 'transport_failure') return Promise.reject(new Error('offline'));
    return Promise.resolve({ok: true, json: async () => input.reply});
});
const {cx, context, controller} = fixture;
context.cx = cx;
vm.runInNewContext(input.browser_source.replace(/export const (\w+)=/g, 'globalThis.$1='), context);
context.__lificNativeMounts = {browser: owner => context.browser(owner)};
const referencedSignals = new Set();
const signal = cx.signal.bind(cx);
cx.signal = id => {
  referencedSignals.add(id);
  return signal(id);
};
const bindingSignal = source => {
  const previous = new Set(referencedSignals);
  vm.runInNewContext(`cx => (${source})`, context)(cx);
  const ids = [...referencedSignals].filter(id => !previous.has(id));
  assert.equal(ids.length, 1, 'each emitted input binding resolves one owned signal');
  return ids[0];
};
const handler = fixture.handler(input.handler);
const titleSignal = bindingSignal(input.title_binding);
const bodySignal = bindingSignal(input.body_binding);

const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const signalValues = () => Object.keys(input.signals).map(id => unbox(cx.signal(id).dehydrate()));
const referencedValues = () => [...referencedSignals].map(id => unbox(cx.signal(id).dehydrate()));
const sequenceSignal = () => {
  const candidates = [...referencedSignals].filter(id => id !== titleSignal && id !== bodySignal &&
    String(unbox(input.signals[id])) === String(input.expected_seq));
  assert.equal(candidates.length, 1, 'the emitted handler references one page sequence signal');
  return candidates[0];
};
if (input.scenario !== 'capture') {
  cx.signal(titleSignal).set(cx.hydrate('Unsaved title draft'));
  cx.signal(bodySignal).set(cx.hydrate('Unsaved body draft'));
}
handler(cx.event({type: 'change', target: {value: input.status}}));

async function run() {
  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
  assert.equal(requests.length, 1, 'one status selection sends one typed procedure request');
  if (input.scenario === 'capture') {
    process.stdout.write(JSON.stringify({arguments: requests[0]}));
    return;
  }
  if (input.scenario === 'retired') {
    assert.equal(typeof finishRequest, 'function', 'the production reply is pending');
    const stateBeforeDispose = Object.fromEntries(Object.keys(input.signals)
      .map(id => [id, unbox(cx.signal(id).dehydrate())]));
    controller.abort();
    finishRequest();
    for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
    assert.deepEqual(Object.fromEntries(Object.keys(input.signals)
      .map(id => [id, unbox(cx.signal(id).dehydrate())])), stateBeforeDispose,
    'a retired page owner ignores the late status response');
  } else if (input.scenario === 'success') {
    assert.ok(referencedValues().includes('active'),
      'the actual saved status is published');
    assert.equal(unbox(cx.signal(sequenceSignal()).dehydrate()), unbox(input.reply.v.seq),
      'success adopts the sequence from the typed production reply');
    assert.equal(unbox(cx.signal(titleSignal).dehydrate()), 'Unsaved title draft');
    assert.equal(unbox(cx.signal(bodySignal).dehydrate()), 'Unsaved body draft');
  } else if (input.scenario === 'conflict') {
    assert.ok(referencedValues().includes('draft'),
      'a conflict restores the locally selected status');
    assert.equal(unbox(cx.signal(sequenceSignal()).dehydrate()), String(input.expected_seq),
      'a conflict never adopts the unseen remote sequence');
    assert.ok(referencedValues().some(value => typeof value === 'string' && value.includes('changed elsewhere')),
      'the conflict is visible while the editor is closed');
  } else if (input.scenario === 'transport_failure') {
    assert.ok(referencedValues().includes('draft'),
      'a transport failure restores the locally selected status');
    assert.equal(unbox(cx.signal(sequenceSignal()).dehydrate()), String(input.expected_seq),
      'a transport failure cannot advance the sequence');
    assert.ok(referencedValues().some(value => typeof value === 'string' && value.includes("Couldn't save")),
      'the transport failure remains visible');
  }
  process.stdout.write(JSON.stringify({requests: requests.length, signals: signalValues()}));
}
run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
