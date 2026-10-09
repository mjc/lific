'use strict';

const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');

// Load the packaged runtime without starting its document or socket owners.
// Tests supply browser I/O and execute the actual emitted event handler.
function handlerFixture(signals, fetch, browserSource) {
  const {fixtureRuntime} = require('./runtime_fixture.cjs');
  const controller = new AbortController();
  const context = {
    TextEncoder,
    TextDecoder,
    queueMicrotask,
    Event: class { constructor(type) { this.type = type; } },
    Element: class {},
    window: {dispatchEvent() {}},
    document: {documentElement: {getAttribute: () => ''}, querySelector: () => null},
    fetch,
  };
  vm.runInNewContext(fixtureRuntime(['Context', 'Registry', 'Event']), context);
  const registry = new context.fixture.Registry();
  const cx = Object.assign(new context.fixture.Context(registry), {
    abortSignal: controller.signal,
    event: event => new context.fixture.Event(event),
  });
  for (const [id, value] of Object.entries(signals)) registry.insert(id, cx.hydrate(value));
  if (browserSource) {
    vm.runInNewContext(browserSource.replace(/export const (\w+)=/g, 'globalThis.$1='), context);
    context.__lificNativeMounts = {browser: owner => context.browser(owner)};
  }
  return {
    cx,
    registry,
    context,
    controller,
    handler: (source, owner = cx) => vm.runInNewContext(`cx => (${source})`, context)(owner),
  };
}

module.exports = {handlerFixture};
