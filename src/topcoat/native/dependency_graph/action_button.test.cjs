'use strict';

// Executes the production Rust-emitted action handler against Topcoat's
// packaged procedure/signal runtime. The procedure response is successful;
// the graph controls must leave the page in its success state.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(runtime.split(bootstrap).length - 1, 1, 'packaged bootstrap');
const eventClass = runtime.match(/event\(\w+\)\{return new (\w+)\(\w+\)\}/)?.[1];
assert.ok(eventClass, 'packaged Event surrogate');

const calls = [];
const context = {
  TextEncoder,
  TextDecoder,
  queueMicrotask,
  document: {documentElement: {getAttribute: () => ''}},
  fetch: async (url, options) => {
    calls.push({url, args: JSON.parse(options.body)});
    return {ok: true, status: 200, statusText: 'OK', json: async () => 'unlinked'};
  },
};
vm.runInNewContext(runtime.replace(bootstrap,
  `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass}};`), context);
const registry = new context.fixture.Registry();
const cx = Object.assign(new context.fixture.Context(registry), {
  event: event => new context.fixture.Event(event),
});
for (const [id, value] of Object.entries(input.signals)) {
  registry.insert(id, cx.hydrate(value));
}

async function run() {
  const handler = vm.runInNewContext(`cx => (${input.handler})`, context)(cx);
  handler(cx.event({type: 'click', preventDefault() {}, stopPropagation() {}}));
  for (let i = 0; i < 30; i++) await Promise.resolve();

  assert.equal(calls.length, 1, 'the relation procedure ran');
  assert.equal(calls[0].url, '/__native_dependency_graph/unlink');
  assert.equal(calls[0].args.length, 4);

  const value = id => {
    const current = registry.read(id);
    return current && typeof current.dehydrate === 'function'
      ? current.dehydrate()
      : current;
  };
  const scalar = id => {
    const current = value(id);
    if (current && typeof current === 'object' && 'v' in current) return Number(current.v);
    return current;
  };
  assert.equal(scalar(input.busy), false, 'busy state clears');
  assert.equal(scalar(input.error), '', 'no failure message is shown');
  assert.equal(scalar(input.revision), 1, 'the graph refresh revision advances');
  assert.equal(scalar(input.menu), '', 'the relation menu closes');
}

run().catch(error => { console.error(error); process.exitCode = 1; });
