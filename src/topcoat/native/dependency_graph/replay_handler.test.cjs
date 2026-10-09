'use strict';

// Executes the actual graph-view handler and returns only changed wire values,
// preserving the original JSON representation of every untouched signal.
const fs = require('node:fs');
const vm = require('node:vm');
const {isDeepStrictEqual} = require('node:util');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const {fixtureRuntime} = require('../runtime_fixture.cjs');
const context = {
  TextEncoder,
  TextDecoder,
  queueMicrotask,
  document: {documentElement: {getAttribute: () => ''}},
  fetch: async () => { throw new Error('view controls must not make a request'); },
};
vm.runInNewContext(fixtureRuntime(['Context', 'Registry', 'Event']), context);
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
