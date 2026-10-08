'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const calls = [];
let settleHeld;
const fixture = handlerFixture(input.signals, (url, options) => {
  const index = calls.length;
  calls.push({url: new URL(url, 'http://localhost').pathname, body: JSON.parse(options.body)});
  if (index === 4) {
    return new Promise((resolve, reject) => { settleHeld = outcome => outcome === 'reject'
      ? reject(new Error('offline'))
      : resolve({ok: true, json: async () => input.replies[index]}); });
  }
  if (index === 5) return Promise.reject(new Error('offline'));
  return Promise.resolve({ok: true, json: async () => input.replies[index]});
}, input.browser_source);
const {cx, context, controller, handler} = fixture;
  context.CustomEvent = class { constructor(type, options = {}) { this.type = type; Object.assign(this, options); } };
context.document.documentElement.getAttribute = name =>
  name === 'data-topcoat-runtime-prefix' ? input.mount : '';
context.document.getElementById = () => ({focus() {}});
const fire = (source, event) => handler(source)(cx.event(event));
const key = (name, {ctrlKey = false, metaKey = false} = {}) => ({
  type: 'keydown', key: name, ctrlKey, metaKey, cancelable: true,
  preventDefault() { this.prevented = true; },
});
const inputEvent = value => ({type: 'input', target: {value}, cancelable: true, preventDefault() {}});
const flush = async () => { for (let i = 0; i < 60; i++) await Promise.resolve(); };
const referenced = new Set();
const originalSignal = cx.signal.bind(cx);
cx.signal = id => { referenced.add(id); return originalSignal(id); };
const bindingSignal = source => {
  const before = new Set(referenced);
  vm.runInNewContext(`cx => (${source})`, context)(cx);
  const ids = [...referenced].filter(id => !before.has(id));
  assert.equal(ids.length, 1, 'each title editor binding resolves one signal');
  return ids[0];
};
const titleId = bindingSignal(input.title_binding);
const bodyId = bindingSignal(input.body_binding);
const titleEditorHidden = input.editor_hidden_binding;
const headingHidden = input.heading_hidden_binding;
const readBinding = source => {
  const wire = vm.runInNewContext(`cx => (${source})`, context)(cx);
  return unbox(wire && typeof wire.dehydrate === 'function' ? wire.dehydrate() : wire);
};
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const value = id => unbox(cx.signal(id).dehydrate());
const snapshot = () => Object.fromEntries([...referenced].map(id => [id, cx.signal(id).dehydrate()]));
const canonicalCandidates = Object.keys(input.signals).filter(id =>
  unbox(input.signals[id]) === 'Page metadata test' && id !== titleId);
assert.equal(canonicalCandidates.length, 1, 'the initial page title has one independent canonical signal');
const canonicalTitleId = canonicalCandidates[0];

(async () => {
  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('   '));
  fire(input.editor.keydown, key('Enter'));
  fire(input.editor.blur, {type: 'blur'});
  await flush();
  assert.equal(calls.length, 0, 'blank title is not saved');

  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('Page metadata test'));
  fire(input.editor.keydown, key('Enter'));
  fire(input.editor.blur, {type: 'blur'});
  await flush();
  assert.equal(calls.length, 0, 'unchanged title is not saved');

  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('Discarded title'));
  fire(input.editor.keydown, key('Escape'));
  fire(input.editor.blur, {type: 'blur'});
  await flush();
  assert.equal(calls.length, 0, 'Escape followed by blur does not save');
  assert.equal(value(titleId), 'Page metadata test', 'Escape restores the canonical title draft');
  assert.equal(readBinding(headingHidden), false, 'Escape closes the title input');
  assert.equal(readBinding(titleEditorHidden), true);

  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('  Renamed title  '));
  const enter = key('Enter');
  fire(input.editor.keydown, enter);
  fire(input.editor.blur, {type: 'blur'});
  await flush();
  assert.equal(enter.prevented, true, 'Enter prevents form submission');
  assert.equal(calls.length, 1, 'Enter followed by blur commits once');
  assert.equal(calls[0].url, `${input.mount}/__native_pages/save_title`);
  assert.deepEqual(calls[0].body, input.expected_arguments[0], 'the title write carries only title and expected sequence');
  assert.equal(value(canonicalTitleId), 'Renamed title', 'the canonical title adopts the committed response');
  assert.equal(value(titleId), 'Renamed title', 'a clean title draft follows the committed response');

  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('Blur title'));
  fire(input.editor.blur, {type: 'blur'});
  await flush();
  assert.equal(calls.length, 2, 'blur alone commits an edited title');
  assert.deepEqual(calls[1].body, input.expected_arguments[1]);

  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('Ctrl title'));
  const ctrlShortcut = key('s', {ctrlKey: true});
  fire(input.editor.keydown, ctrlShortcut);
  await flush();
  assert.equal(ctrlShortcut.prevented, true, 'Ctrl+S prevents browser Save');
  assert.equal(calls.length, 3, 'Ctrl+S commits the title');
  assert.deepEqual(calls[2].body, input.expected_arguments[2]);

  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('Cmd title'));
  const metaShortcut = key('s', {metaKey: true});
  fire(input.editor.keydown, metaShortcut);
  await flush();
  assert.equal(metaShortcut.prevented, true, 'Cmd+S prevents browser Save');
  assert.equal(calls.length, 4, 'Cmd+S commits the title');
  assert.deepEqual(calls[3].body, input.expected_arguments[3]);

  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('Pending title'));
  fire(input.editor.keydown, key('Enter'));
  for (let attempt = 0; attempt < 60 && !settleHeld; attempt++) await Promise.resolve();
  assert.equal(typeof settleHeld, 'function', 'pending title save reached the procedure boundary');
  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('Newer title draft'));
  fire(input.editor.input, inputEvent('Cmd title'));
  fire(input.body_input, inputEvent('Dirty body draft'));
  settleHeld('success');
  await flush();
  assert.equal(calls.length, 5);
  assert.deepEqual(calls[4].body, input.expected_arguments[4]);
  assert.equal(calls[4].url, `${input.mount}/__native_pages/save_title`);
  assert.equal(value(canonicalTitleId), 'Pending title', 'the delayed reply advances canonical title');
  assert.equal(value(titleId), 'Cmd title', 'editing away and back to the old canonical title survives the pending response');
  assert.equal(value(bodyId), 'Dirty body draft', 'a dirty body draft survives a title response');
  assert.equal(value(canonicalTitleId), 'Pending title');
  assert.equal(readBinding(headingHidden), true, 'the newer title editor remains open');
  assert.equal(readBinding(titleEditorHidden), false);
  assert.deepEqual(calls.map(call => call.url), Array(5).fill(`${input.mount}/__native_pages/save_title`),
    'title editing never submits a body or combined save');

  fire(input.editor.input, inputEvent('Rejected title'));
  fire(input.editor.blur, {type: 'blur'});
  await flush();
  assert.equal(calls.length, 6, 'blur sends one request for the failed title');
  assert.equal(value(titleId), 'Rejected title', 'a rejected title remains in the draft');
  assert.equal(readBinding(titleEditorHidden), false, 'a rejected title remains editable');
  assert.ok(Object.keys(input.signals).some(id => {
    const current = value(id);
    return typeof current === 'string' && current.includes("Couldn't save the page title");
  }), 'transport failure remains visible to the editor');
  controller.abort();
  process.stdout.write(JSON.stringify({passed: true, calls: calls.length, snapshot: snapshot()}));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
