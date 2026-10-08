'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const calls = [], navigations = [];
let failNextFetch = false;
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

(async () => {
  if (input.phase === 'enter_edit') {
    fire(input.edit, {type: 'click', preventDefault() {}});
    process.stdout.write(JSON.stringify({signals: snapshot()}));
    return;
  }

  fire(input.input, {type: 'input', target: {value: 'Discarded description'}});
  fire(input.cancel, {type: 'click', preventDefault() {}});
  await flush();
  assert.equal(calls.length, 0, 'Edit and Cancel do not call the module update procedure');
  assert.ok(Object.values(snapshot()).includes(input.initial_description),
    'Cancel restores the canonical description');

  fire(input.edit, {type: 'click', preventDefault() {}});
  fire(input.input, {type: 'input', target: {value: 'Failed module body'}});
  failNextFetch = true;
  const beforeFailure = navigations.length;
  fire(input.save, {type: 'click', preventDefault() {}});
  await flush();
  assert.equal(calls.length, 1, 'a failed Save attempts the production procedure once');
  assert.equal(calls[0].arguments[3], 'description');
  assert.equal(calls[0].arguments[4], 'Failed module body');
  assert.equal(navigations.length, beforeFailure, 'failed Save does not navigate away');
  const canonicalAfterFailure = Object.values(snapshot());
  const failedSaveNavigations = navigations.length - beforeFailure;
  assert.ok(canonicalAfterFailure.includes(input.initial_description),
    'a rejected update leaves the canonical description unchanged');

  fire(input.edit, {type: 'click', preventDefault() {}});
  assert.ok(!Object.values(snapshot()).includes('Failed module body'),
    're-entering edit mode starts from canonical content after failure');
  fire(input.input, {type: 'input', target: {value: 'Preview committed body'}});
  fire(input.preview, {type: 'click', preventDefault() {}});
  await flush();
  assert.equal(calls.length, 2, 'Preview commits a changed description exactly once');
  assert.equal(calls[1].arguments[3], 'description');
  assert.equal(calls[1].arguments[4], 'Preview committed body');

  fire(input.edit, {type: 'click', preventDefault() {}});
  fire(input.input, {type: 'input', target: {value: 'Saved module body'}});
  fire(input.save, {type: 'click', preventDefault() {}});
  await flush();
  assert.equal(calls.length, 3, 'explicit Save commits exactly once');
  assert.equal(calls[2].url, '/app/__native_modules/update');
  assert.equal(calls[2].arguments[3], 'description');
  assert.equal(calls[2].arguments[4], 'Saved module body');

  process.stdout.write(JSON.stringify({
    cancel_requests: 0,
    failed_save_requests: 1,
    failed_save_arguments: calls[0].arguments,
    failed_save_navigations: failedSaveNavigations,
    canonical_after_failure: canonicalAfterFailure,
    preview_save_requests: 1,
    preview_save_arguments: calls[1].arguments,
    explicit_save_requests: 1,
    explicit_save_url: calls[2].url,
    explicit_save_arguments: calls[2].arguments,
  }));
})().catch(error => { console.error(error); process.exitCode = 1; });
