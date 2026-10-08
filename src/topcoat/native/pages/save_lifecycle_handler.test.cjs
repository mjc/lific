'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');
const {emittedShard} = require('./activity_shard_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
let settleRequest;
const fixture = handlerFixture(input.signals, (url, options) => {
  const path = new URL(url, 'http://localhost').pathname;
  assert.ok(path.endsWith('/__native_pages/save'), `unexpected procedure ${path}`);
  assert.equal(options.method, 'POST');
  requests.push(JSON.parse(options.body));
  if (input.scenario === 'dispose_before_queue') {
    return Promise.resolve({ok: true, json: async () => input.reply});
  }
  return new Promise((resolve, reject) => {
    settleRequest = outcome => outcome === 'success'
      ? resolve({ok: true, json: async () => input.reply})
      : reject(new Error('offline'));
  });
}, input.browser_source);
const {cx, context, controller} = fixture;
const referencedSignals = new Set();
const originalSignal = cx.signal.bind(cx);
cx.signal = id => {
  referencedSignals.add(id);
  return originalSignal(id);
};
const evaluate = source => vm.runInNewContext(`cx => (${source})`, context)(cx);
const bindingSignal = source => {
  const previous = new Set(referencedSignals);
  evaluate(source);
  const ids = [...referencedSignals].filter(id => !previous.has(id));
  assert.equal(ids.length, 1, 'an emitted editor binding resolves one signal');
  return ids[0];
};
evaluate(input.busy_binding);
const busyIds = [...referencedSignals];
assert.equal(busyIds.length, 1, 'the emitted pin control resolves the shared busy signal');
const busyId = busyIds[0];
const titleDraftId = bindingSignal(input.title_binding);
const bodyDraftId = bindingSignal(input.body_binding);
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const plain = value => {
  if (value && typeof value.dehydrate === 'function') value = value.dehydrate();
  return JSON.parse(JSON.stringify(value));
};
const snapshot = () => Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, JSON.parse(JSON.stringify(cx.signal(id).dehydrate()))]));
const value = id => unbox(cx.signal(id).dehydrate());
const handler = fixture.handler;
const textInput = value => cx.event({
  type: 'input', target: {value}, cancelable: true, preventDefault() {},
});
const click = () => cx.event({
  type: 'click', target: {}, cancelable: true, preventDefault() {}, stopPropagation() {},
});

async function flush() {
  for (let attempt = 0; attempt < 80; attempt += 1) await Promise.resolve();
}

async function run() {
  handler(input.title_handler)(textInput('Retired title'));
  handler(input.body_handler)(textInput('Retired body'));
  if (input.scenario === 'dispose_before_queue') {
    handler(input.save_handler)(click());
    const afterClick = snapshot();
    controller.abort();
    await flush();
    assert.equal(requests.length, 0, 'disposal before queued Save prevents the procedure call');
    assert.deepEqual(snapshot(), afterClick, 'the queued callback makes no post-disposal signal writes');
    process.stdout.write(JSON.stringify({passed: true, requests: requests.length}));
    return;
  }

  handler(input.save_handler)(click());
  await flush();
  assert.equal(requests.length, 1, 'the actual emitted Save handler started one pending request');
  assert.equal(typeof settleRequest, 'function');
  const request = requests[0];
  if (input.scenario === 'live_success' || input.scenario === 'pending_edit_success') {
    if (input.scenario === 'pending_edit_success') {
      handler(input.title_handler)(textInput('Newer title draft'));
      handler(input.body_handler)(textInput('Newer body draft'));
    }
    settleRequest('success');
    await flush();
    const activity = emittedShard(input.shard_marker, context, cx, plain);
    assert.deepEqual(activity.args[1], input.reply.v.seq.v,
      'the activity shard reads the sequence adopted from the committed Save reply');
    process.stdout.write(JSON.stringify({
      passed: true,
      requests: requests.length,
      response: 'success',
      arguments: request,
      activity_shard: activity,
      drafts: [value(titleDraftId), value(bodyDraftId)],
    }));
    return;
  }
  controller.abort();
  cx.signal(busyId).set(cx.hydrate(false));
  cx.signal(busyId).set(cx.hydrate(true));
  const beforeLateReply = snapshot();
  const response = input.scenario === 'retired_success' ? 'success' : 'rejection';
  settleRequest(response);
  await flush();
  assert.deepEqual(snapshot(), beforeLateReply,
    'a retired Save completion cannot mutate any retained editor signal, including another operation busy');
  process.stdout.write(JSON.stringify({
    passed: true,
    requests: requests.length,
    response,
    arguments: request,
  }));
}

run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
