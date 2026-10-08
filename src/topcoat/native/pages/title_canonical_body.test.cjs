'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8')); // Fixture edits only the reply snapshot to cover client reconciliation.
const calls = [];
let settle;
const fixture = handlerFixture(input.signals, (url, options) => {
  calls.push({url: new URL(url, 'http://localhost').pathname, body: JSON.parse(options.body)});
  return new Promise(resolve => { settle = () => resolve({ok: true, json: async () => input.saved}); });
}, input.browser_source);
const {cx, context, controller, handler} = fixture;
  context.CustomEvent = class { constructor(type, options = {}) { this.type = type; Object.assign(this, options); } };
context.document.documentElement.getAttribute = name => name === 'data-topcoat-runtime-prefix' ? input.mount : '';
context.document.getElementById = () => ({focus() {}});
const fire = (source, event) => handler(source)(cx.event(event));
const key = name => ({type: 'keydown', key: name, cancelable: true, preventDefault() {}});
const text = value => ({type: 'input', target: {value}, cancelable: true, preventDefault() {}});
const flush = async () => { for (let i = 0; i < 60; i++) await Promise.resolve(); };
const unbox = value => {
  while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const read = source => {
  const result = vm.runInNewContext(`cx => (${source})`, context)(cx);
  let value = result && typeof result.dehydrate === 'function' ? result.dehydrate() : result;
  value = unbox(value);
  if (typeof value === 'string' && Object.hasOwn(input.signals, value)) value = unbox(cx.signal(value).get().dehydrate());
  return value;
};
const referenced = new Set();
const originalSignal = cx.signal.bind(cx);
cx.signal = id => { referenced.add(id); return originalSignal(id); };
const bodyId = (() => {
  const before = new Set(referenced);
  vm.runInNewContext(`cx => (${input.body_binding})`, context)(cx);
  return [...referenced].find(id => !before.has(id));
})();
const canonicalBodyId = Object.keys(input.signals).find(id => id !== bodyId && unbox(input.signals[id]) === 'Original body');
assert.ok(canonicalBodyId, 'SSR has separate canonical and draft body signals');
(async () => {
  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, text('Title being saved'));
  fire(input.editor.keydown, key('Enter'));
  for (let attempt = 0; attempt < 60 && !settle; attempt++) await Promise.resolve();
  assert.equal(calls.length, 1, 'the sparse title procedure starts');
  fire(input.body_input, text('Dirty body draft'));
  settle();
  await flush();
  assert.equal(unbox(cx.signal(canonicalBodyId).get().dehydrate()), 'External canonical body',
    'the accepted reply advances the canonical body baseline');
  assert.equal(read(input.body_binding), 'Dirty body draft',
    'canonical adoption retains the independently dirty body draft');
  assert.equal(calls[0].url, `${input.mount}/__native_pages/save_title`);
  assert.equal(Object.keys(calls[0].body).length, 4, 'request carries only account, page, title, and sequence');
  controller.abort();
  process.stdout.write(JSON.stringify({passed: true}));
})().catch(error => { process.stderr.write(`${error.stack}\\n`); process.exitCode = 1; });
