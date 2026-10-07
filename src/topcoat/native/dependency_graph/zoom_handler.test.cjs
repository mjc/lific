'use strict';

// Executes the zoom control handler emitted by the rendered graph against
// the packaged Topcoat signal runtime. The Rust test sends the resulting
// signal snapshot back through the authenticated page renderer.
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

const viewport = {
  clientWidth: input.viewport.width,
  clientHeight: input.viewport.height,
};
const target = {
  closest: selector => selector === '[data-native-graph-viewport]' ? viewport : null,
};

const calls = [];
let motion = 'full';
const context = {
  TextEncoder,
  TextDecoder,
  queueMicrotask,
  document: {documentElement: {getAttribute: name => name === 'data-motion' ? motion : '/app'}},
  fetch: async (url, options) => {
    calls.push({url, options});
    throw new Error('zoom controls must not make a request');
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
  assert.equal(input.handlers.length, 2);
  const snapshots = [];
  const styles = [];
  const styleBinding = vm.runInNewContext(`cx => (${input.style_binding})`, context);
  for (const [index, action] of input.actions.entries()) {
    const source = input.handlers[action === 'in' ? 0 : 1];
    assert.equal(typeof source, 'string',
      `rendered zoom ${action} button exposes an executable handler`);
    const handler = vm.runInNewContext(`cx => (${source})`, context)(cx);
    handler(cx.event({type: 'click', target, preventDefault() {}, stopPropagation() {}}));
    for (let i = 0; i < 20; i++) await Promise.resolve();
    const values = {};
    for (const id of Object.keys(input.signals)) {
      const current = registry.read(id);
      values[id] = current && typeof current.dehydrate === 'function'
        ? current.dehydrate()
        : current;
    }
    snapshots.push(values);
    const style = styleBinding(cx);
    assert.ok(style && typeof style.dehydrate === 'function',
      'transform binding returns a hydrated String surrogate');
    styles.push(style.dehydrate());
    assert.ok(styles.at(-1).includes('transition:translate 0ms,scale 150ms'),
      'zoom transitions scale without animating pan');
  }
  motion = 'reduced';
  const reducedHandler = vm.runInNewContext(`cx => (${input.handlers[0]})`, context)(cx);
  reducedHandler(cx.event({type: 'click', target, preventDefault() {}, stopPropagation() {}}));
  const reducedStyle = styleBinding(cx).dehydrate();
  assert.ok(reducedStyle.includes('transition:translate 0ms,scale 0ms'),
    'reduced motion disables the scale transition');
  assert.equal(calls.length, 0, 'zoom is local and makes no network request');
  process.stdout.write(JSON.stringify({snapshots, styles, reduced_style: reducedStyle}));
}

run().catch(error => { console.error(error); process.exitCode = 1; });
