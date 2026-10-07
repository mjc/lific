'use strict';

// Executes the actual graph-view handler and returns only changed wire values,
// preserving the original JSON representation of every untouched signal.
const fs = require('node:fs');
const vm = require('node:vm');
const {isDeepStrictEqual} = require('node:util');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
const eventClass = runtime.match(/event\(\w+\)\{return new (\w+)\(\w+\)\}/)?.[1];
if (!eventClass) throw new Error('packaged Event surrogate not found');
const context = {
  TextEncoder,
  TextDecoder,
  queueMicrotask,
  document: {documentElement: {getAttribute: () => ''}},
  fetch: async () => { throw new Error('view controls must not make a request'); },
};
vm.runInNewContext(runtime.replace(bootstrap,
  `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass}};`), context);
const registry = new context.fixture.Registry();
const cx = Object.assign(new context.fixture.Context(registry), {
  event: event => new context.fixture.Event(event),
});
for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
const snapshot = () => Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, cx.signal(id).dehydrate().v]));
const baseline = snapshot();
const handler = vm.runInNewContext(`cx => (${input.handler})`, context)(cx);
handler(cx.event({type: 'click', preventDefault() {}, stopPropagation() {}}));
const changes = Object.fromEntries(Object.entries(snapshot())
  .filter(([id, value]) => !isDeepStrictEqual(value, baseline[id])));
process.stdout.write(JSON.stringify(changes));
