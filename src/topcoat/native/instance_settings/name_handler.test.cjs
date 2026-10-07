'use strict';

// Draft for src/topcoat/native/instance_settings/name_handler.test.cjs.
// Executes the emitted input and blur handlers with real procedure replies
// serialized by the Rust production fixture.
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

function makeFixture(replies) {
  const requests = [];
  const controller = new AbortController();
  const context = {
    TextEncoder, TextDecoder, queueMicrotask,
    Event: class { constructor(type) { this.type = type; } },
    document: {documentElement: {getAttribute: () => ''}},
    Element: class {}, window: {dispatchEvent() {}},
    fetch: async (url, options) => {
      const path = new URL(url, 'http://localhost').pathname;
      assert.ok(path.endsWith('/__native_instance_settings/save_text'), `unexpected route ${path}`);
      assert.equal(options.method, 'POST');
      const body = JSON.parse(options.body);
      requests.push(body);
      const reply = replies[requests.length - 1];
      assert.ok(reply, 'fixture has a serialized reply for each save');
      return {ok: true, json: async () => reply};
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
  const valueId = input.input_handler.match(/"id":"([^"]+)"/)?.[1];
  assert.ok(valueId, 'the emitted input handler identifies the draft signal');
  const fireInput = value => inputHandler(cx.event({type: 'input', target: {value}}));
  const blur = () => blurHandler(cx.event({type: 'blur', target: {}, currentTarget: {}}));
  const current = () => {
    const value = cx.signal(valueId).dehydrate();
    return value !== null && typeof value === 'object' && Object.hasOwn(value, 'v') ? value.v : value;
  };
  return {requests, controller, fireInput, blur, current};
}

async function run() {
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

  const failed = makeFixture([input.error_reply]);
  const rejectedValue = 'Draft survives';
  failed.fireInput(rejectedValue);
  failed.blur();
  await settle();
  assert.equal(failed.requests.length, 1);
  assert.equal(failed.current(), rejectedValue);
  process.stdout.write(JSON.stringify({
    trimmed_save: true, unchanged_noop: true, blank_clears: true,
    disposed_no_request: true, draft_kept_on_error: true,
  }));
}
run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
