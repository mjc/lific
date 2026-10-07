'use strict';

// Execute the emitted input and blur handlers with replies from the real
// authenticated native save_text procedure.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(runtime.split(bootstrap).length - 1, 1);
const eventClass = runtime.match(/event\(\w+\)\{return new (\w+)\(\w+\)\}/)?.[1];
assert.ok(eventClass);
const settle = async () => { for (let i = 0; i < 60; i++) await Promise.resolve(); };

function makeFixture(replies, delayFirst = false) {
  const requests = [];
  const pending = [];
  const controller = new AbortController();
  let invokeBlur;
  const context = {
    TextEncoder, TextDecoder, queueMicrotask,
    Event: class { constructor(type) { this.type = type; } },
    document: {
      documentElement: {getAttribute: () => ''},
      querySelector: selector => ({
        dataset: {topcoatUsizeBits: '64'},
        dispatchEvent: () => {
          if (selector === 'input[data-native-instance-name]') invokeBlur?.();
          return true;
        },
      }),
    },
    Element: class {}, window: {dispatchEvent() {}},
    fetch: (url, options) => {
      const path = new URL(url, 'http://localhost').pathname;
      assert.ok(path.endsWith('/__native_instance_settings/save_text'), `unexpected route ${path}`);
      assert.equal(options.method, 'POST');
      const body = JSON.parse(options.body);
      requests.push(body);
      const reply = replies[requests.length - 1];
      assert.ok(reply, 'fixture has a serialized reply for each save');
      if (delayFirst && requests.length === 1) {
        return new Promise(resolve => pending.push(() => resolve({ok: true, json: async () => reply})));
      }
      return Promise.resolve({ok: true, json: async () => reply});
    },
  };
  vm.runInNewContext(runtime.replace(bootstrap,
    `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass}};`), context);
  const registry = new context.fixture.Registry();
  const cx = Object.assign(new context.fixture.Context(registry), {
    abortSignal: controller.signal,
    event: event => new context.fixture.Event(event),
  });
  for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
  const handler = source => vm.runInNewContext(`cx => (${source})`, context)(cx);
  const inputHandler = handler(input.input_handler);
  const blurHandler = handler(input.blur_handler);
  const valueId = input.value_binding.match(/"id":"([^"]+)"/)?.[1];
  assert.ok(valueId, 'the emitted value binding identifies the draft signal');
  const fireInput = value => inputHandler(cx.event({type: 'input', target: {value}}));
  const blur = () => blurHandler(cx.event({type: 'blur', target: {}, currentTarget: {}}));
  invokeBlur = blur;
  const current = () => {
    const value = cx.signal(valueId).dehydrate();
    return value !== null && typeof value === 'object' && Object.hasOwn(value, 'v') ? value.v : value;
  };
  return {
    requests, controller, fireInput, blur, current,
    values: () => Object.keys(input.signals).map(id => {
      const value = cx.signal(id).dehydrate();
      return value !== null && typeof value === 'object' && Object.hasOwn(value, 'v') ? value.v : value;
    }),
    releaseFirst: () => {
      const release = pending.shift();
      assert.ok(release, 'the first request is pending');
      release();
    },
  };
}

async function run() {
  const queued = makeFixture([input.save_reply, input.queued_reply], true);
  queued.fireInput('  New name  ');
  queued.blur();
  await settle();
  assert.equal(queued.requests.length, 1);
  assert.deepEqual(queued.requests[0], input.expected_save_args,
    'the delayed first request matches its real procedure reply');
  queued.fireInput('  Latest name  ');
  queued.blur();
  await settle();
  assert.equal(queued.requests.length, 1, 'blur while the save is pending queues the latest value');
  queued.releaseFirst();
  await settle();
  assert.equal(queued.requests.length, 2, 'the queued blur sends a second serialized mutation');
  assert.deepEqual(queued.requests[1], input.expected_queued_args,
    'the queued save sends the latest trimmed draft');
  assert.equal(queued.current(), 'Latest name', 'the first response cannot erase the newer draft');

  const changed = makeFixture([input.save_reply, input.clear_reply]);
  changed.fireInput('  New name  ');
  changed.blur();
  changed.blur();
  await settle();
  assert.equal(changed.requests.length, 1, 'duplicate blur during save does not duplicate mutation');
  assert.deepEqual(changed.requests[0], input.expected_save_args,
    'blur sends the trimmed name through the actual native save_text procedure');
  assert.equal(changed.current(), 'New name');

  changed.fireInput('  New name  ');
  changed.blur();
  await settle();
  assert.equal(changed.requests.length, 1, 'normalized unchanged values are a no-op');

  changed.fireInput('');
  changed.blur();
  await settle();
  assert.deepEqual(changed.requests[1], input.expected_clear_args,
    'blank is sent as the request to restore the host-name fallback');
  assert.equal(changed.current(), '');

  // A server refusal keeps the typed value visible for correction/retry.
  const disposed = makeFixture([input.save_reply]);
  disposed.controller.abort();
  disposed.fireInput('No request after unmount');
  disposed.blur();
  await settle();
  assert.equal(disposed.requests.length, 0, 'a disposed field owner sends no late mutation');

  const disposedPending = makeFixture([input.save_reply], true);
  disposedPending.fireInput('Pending draft');
  disposedPending.blur();
  await settle();
  assert.equal(disposedPending.requests.length, 1);
  disposedPending.controller.abort();
  const stateBeforeLateReply = disposedPending.values();
  disposedPending.releaseFirst();
  await settle();
  assert.deepEqual(disposedPending.values(), stateBeforeLateReply,
    'a late response after owner disposal publishes no status or saved value');

  const failed = makeFixture([input.error_reply]);
  const rejectedValue = 'Draft survives';
  failed.fireInput(rejectedValue);
  failed.blur();
  await settle();
  assert.equal(failed.requests.length, 1);
  assert.equal(failed.current(), rejectedValue);

  const ordinaryFailure = makeFixture([input.ordinary_reply]);
  ordinaryFailure.fireInput('Draft resets');
  ordinaryFailure.blur();
  await settle();
  assert.equal(ordinaryFailure.current(), input.name_signal_value,
    'ordinary refusals restore the authoritative saved value');
  assert.ok(ordinaryFailure.values().includes('only an admin can do this'),
    'ordinary refusals remain visible as an error');
  process.stdout.write(JSON.stringify({
    trimmed_save: true, unchanged_noop: true, blank_clears: true, queued_latest: true,
    disposed_no_request: true, disposed_pending_unchanged: true,
    draft_kept_on_error: true, ordinary_failure_restores: true,
  }));
}
run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
