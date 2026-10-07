const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');

const {mount, destination, handlers, signals} = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const calls = [], navigations = [];
const context = {
  TextEncoder, TextDecoder, queueMicrotask,
  document: {documentElement: {getAttribute: () => mount}},
  fetch: async (url, options) => {
    calls.push({url, arguments: JSON.parse(options.body)});
    return {ok: true, json: async () => null};
  },
};
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(runtime.split(bootstrap).length - 1, 1);
vm.runInNewContext(runtime.replace(bootstrap,
  'globalThis.fixture={Context:fe,Registry:ve};'), context);
const registry = new context.fixture.Registry();
const cx = Object.assign(new context.fixture.Context(registry), {
  navigate: url => navigations.push(url),
});
for (const [id, value] of Object.entries(signals)) registry.insert(id, cx.hydrate(value));

(async () => {
  for (const {field, source} of handlers) {
    const handler = vm.runInNewContext(`cx => (${source})`, context)(cx);
    handler({prevent_default() {}, preventDefault() {}});
    for (let i = 0; i < 30; i++) await Promise.resolve();
    assert.equal(calls.at(-1).url, `${mount}/__native_modules/update`);
    assert.equal(calls.at(-1).arguments[3], field);
    assert.equal(navigations.at(-1), destination);
  }
  assert.equal(calls.length, handlers.length);
  assert.equal(navigations.length, handlers.length);
})().catch(error => {console.error(error); process.exitCode = 1;});
