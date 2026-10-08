'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const calls = [], navigations = [];
let failNextFetch = false, holdNextFetch = false, rejectFetch;
const context = {
  TextEncoder, TextDecoder, queueMicrotask,
  requestAnimationFrame: callback => callback(),
  document: {documentElement: {getAttribute: () => input.mount}},
  fetch: async (url, options) => {
    calls.push({url, arguments: JSON.parse(options.body)});
    if (failNextFetch) {
      failNextFetch = false;
      throw new Error('fixture request failure');
    }
    if (holdNextFetch) {
      holdNextFetch = false;
      return await new Promise((_, reject) => { rejectFetch = reject; });
    }
    return {ok: true, json: async () => null};
  },
};
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(runtime.split(bootstrap).length - 1, 1);
vm.runInNewContext(runtime.replace(bootstrap,
  'globalThis.fixture={Context:fe,Registry:ve};'), context);
const registry = new context.fixture.Registry();
const controller = new AbortController();
const cx = Object.assign(new context.fixture.Context(registry), {
  abortSignal: controller.signal,
  navigate: url => navigations.push(url),
  event: event => ({
    inner: {...event, currentTarget: event.currentTarget ?? {
      parentElement: {querySelector: () => ({focus() {}})},
    }},
    target: event.target && {value: cx.hydrate(event.target.value)},
    prevent_default: () => event.preventDefault?.(),
  }),
});
for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
const handler = source => vm.runInNewContext(`cx => (${source})`, context)(cx);
const fire = (source, event) => handler(source)(cx.event(event));
const flush = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
const snapshot = () => Object.fromEntries(Object.keys(input.signals).map(id => [id, cx.signal(id).dehydrate().v]));
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const readBinding = source => {
  const result = vm.runInNewContext(`cx => (${source})`, context)(cx);
  const wire = result && typeof result.dehydrate === 'function' ? result.dehydrate() : result;
  return unbox(wire);
};

(async () => {
  assert.equal(readBinding(input.details.expanded_binding), 'false');
  assert.equal(readBinding(input.details.backdrop_hidden_binding), true,
    'the mobile backdrop is hidden until the drawer opens');
  fire(input.details.toggle, {type: 'click'});
  assert.equal(readBinding(input.details.expanded_binding), 'true');
  assert.equal(readBinding(input.details.backdrop_hidden_binding), false,
    'opening Details shows the dismissible backdrop');
  fire(input.details.backdrop, {type: 'click'});
  assert.equal(readBinding(input.details.expanded_binding), 'false',
    'tapping outside closes the mobile details drawer');
  assert.equal(readBinding(input.details.backdrop_hidden_binding), true);

  fire(input.trigger, {type: 'click'});
  fire(input.input.keydown, {type: 'keydown', key: 'Escape'});
  fire(input.input.blur, {type: 'blur'});
  await flush();
  assert.equal(calls.length, 0, 'Escape cancels and the following blur does not commit');

  fire(input.trigger, {type: 'click'});
  fire(input.input.input, {type: 'input', target: {value: '   '}});
  fire(input.input.keydown, {type: 'keydown', key: 'Enter', preventDefault() {}});
  fire(input.trigger, {type: 'click'});
  fire(input.input.input, {type: 'input', target: {value: 'Handler module'}});
  fire(input.input.keydown, {type: 'keydown', key: 'Enter', preventDefault() {}});
  await flush();
  assert.equal(calls.length, 0, 'blank and unchanged names do not issue writes');

  fire(input.trigger, {type: 'click'});
  fire(input.input.input, {type: 'input', target: {value: '  Renamed once  '}});
  fire(input.input.keydown, {type: 'keydown', key: 'Enter', preventDefault() {}});
  fire(input.input.blur, {type: 'blur'});
  await flush();
  const nameCalls = calls.filter(call => call.arguments[3] === 'name');
  assert.equal(nameCalls.length, 1, 'Enter followed by blur commits once');
  assert.equal(nameCalls[0].arguments[4], 'Renamed once', 'the committed name is trimmed');

  const active = input.status_options.find(option => option.value === 'active');
  const planned = input.status_options.find(option => option.value === 'planned');
  fire(active.handler, {type: 'click', preventDefault() {}});
  await flush();
  assert.equal(calls.filter(call => call.arguments[3] === 'status').length, 0,
    'choosing the current status is a no-op');
  const beforePlannedStatus = snapshot();
  fire(planned.handler, {type: 'click', preventDefault() {}});
  await flush();
  const afterPlannedStatus = snapshot();
  const statusSignalId = Object.keys(beforePlannedStatus).find(id =>
    beforePlannedStatus[id] === 'active' && afterPlannedStatus[id] === 'planned');
  const statusCalls = calls.filter(call => call.arguments[3] === 'status');
  assert.equal(statusCalls.length, 1);
  assert.equal(statusCalls[0].arguments[4], 'planned');

  const beforeRejectedName = snapshot();
  failNextFetch = true;
  fire(input.trigger, {type: 'click'});
  fire(input.input.input, {type: 'input', target: {value: 'Rejected name'}});
  fire(input.input.keydown, {type: 'keydown', key: 'Enter', preventDefault() {}});
  await flush();
  assert.ok(Object.values(snapshot()).includes('Unable to save module name.'),
    'a live owner publishes name-save failure feedback');
  assert.deepEqual(navigations, [input.destination, input.destination],
    'failed name save does not navigate');

  assert.ok(statusSignalId, 'the successful status choice updates its live signal');
  const paused = input.status_options.find(option => option.value === 'paused');
  failNextFetch = true;
  fire(paused.handler, {type: 'click', preventDefault() {}});
  await flush();
  assert.equal(cx.signal(statusSignalId).dehydrate().v, 'planned',
    'a rejected status save restores the previous status');
  assert.ok(Object.values(snapshot()).includes('Unable to save module status.'),
    'a live owner publishes status-save failure feedback');
  assert.deepEqual(navigations, [input.destination, input.destination],
    'failed status save does not navigate');

  const priorNavigationCount = navigations.length;
  holdNextFetch = true;
  fire(input.trigger, {type: 'click'});
  fire(input.input.input, {type: 'input', target: {value: 'Stale module edit'}});
  fire(input.input.keydown, {type: 'keydown', key: 'Enter', preventDefault() {}});
  for (let i = 0; i < 30 && !rejectFetch; i++) await Promise.resolve();
  assert.equal(typeof rejectFetch, 'function', 'the pending update reached the procedure boundary');
  const beforeLateFailure = snapshot();
  controller.abort();
  rejectFetch(new Error('late fixture request failure'));
  await flush();
  assert.equal(navigations.length, priorNavigationCount,
    'a disposed module owner cannot navigate after its pending update');
  assert.deepEqual(snapshot(), beforeLateFailure,
    'a disposed owner ignores a late rejected save without publishing feedback or rollback');
  assert.deepEqual(navigations, [input.destination, input.destination]);
  process.stdout.write(JSON.stringify({
    cancel_requests: 0,
    name_requests: nameCalls.length,
    name_arguments: nameCalls[0].arguments,
    status_requests: statusCalls.length,
    status_arguments: statusCalls[0].arguments,
    navigations: navigations.length,
  }));
})().catch(error => { console.error(error); process.exitCode = 1; });
