'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');

// Load the packaged runtime without starting its document or socket owners.
// Tests supply browser I/O and execute the actual emitted event handler.
function handlerFixture(signals, fetch, browserSource) {
  const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  assert.equal(runtime.split(bootstrap).length - 1, 1, 'packaged runtime bootstrap');
  const eventClass = runtime.match(/event\(\w+\)\{return new (\w+)\(\w+\)\}/)?.[1];
  assert.ok(eventClass, 'packaged Event surrogate');
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
  vm.runInNewContext(runtime.replace(bootstrap,
    `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass}};`), context);
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
    handler: source => vm.runInNewContext(`cx => (${source})`, context)(cx),
  };
}

module.exports = {handlerFixture};
