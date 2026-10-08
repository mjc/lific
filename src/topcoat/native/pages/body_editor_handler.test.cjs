'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const calls = [];
let resolveSave;
const fixture = handlerFixture(input.signals, (url, options) => {
  calls.push({url: new URL(url, 'http://localhost').pathname, args: JSON.parse(options.body)});
  return new Promise(resolve => { resolveSave = () => resolve({ok: true, json: async () => input.reply}); });
}, input.browser_source);
const {cx, context, controller, handler} = fixture;
context.CustomEvent = class { constructor(type, options = {}) { this.type = type; Object.assign(this, options); } };
context.document.documentElement.getAttribute = name => name === 'data-topcoat-runtime-prefix' ? input.mount : '';
const listeners = new Map();
context.window.addEventListener = (name, callback) => listeners.set(name, callback);
context.document.querySelector = selector => {
  assert.ok(
    selector === '[data-native-page-body-save]' ||
      selector === '[role=dialog],[data-native-issue-peek],[data-native-context-menu]',
    `unexpected DOM query ${selector}`
  );
  return null;
};
assert.equal(context.document.querySelector('[data-native-page-body-save]'), null,
  'shared commit handlers work without finding or clicking a Save button');
context.document.getElementById = () => ({focus() {}});
const fire = (source, event = {type: 'click', cancelable: true, preventDefault() {}}) => handler(source)(cx.event(event));
const text = value => ({type: 'input', target: {value}, cancelable: true, preventDefault() {}});
const key = (name, {ctrlKey = false, metaKey = false} = {}) => ({type: 'keydown', key: name, ctrlKey, metaKey, cancelable: true, preventDefault() {}});
const flush = async () => { for (let i = 0; i < 60; i++) await Promise.resolve(); };
const unbox = value => { while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v; return value; };
const inputSignal = input.textarea.binding;
const read = expression => {
  const result = vm.runInNewContext(`cx => (${expression})`, context)(cx);
  return unbox(result && typeof result.dehydrate === 'function' ? result.dehydrate() : result);
};
const readInput = () => read(inputSignal);
(async () => {
  fire(input.keyboard);
  listeners.get('keydown')({key: 'e', ctrlKey: false, metaKey: false, preventDefault() {}});
  await flush();
  assert.equal(read(input.textarea.hidden), false, 'plain E enters body edit mode outside typing contexts');
  fire(input.textarea.input, text('Updated body'));
  fire(input.textarea.keydown, key('Enter'));
  await flush();
  assert.equal(calls.length, 0, 'ordinary Enter remains a newline in the textarea');
  fire(input.textarea.blur, {type: 'blur'});
  await flush();
  assert.equal(calls.length, 0, 'textarea blur does not save');
  const shortcut = key('s', {metaKey: true});
  fire(input.textarea.keydown, shortcut);
  fire(input.save);
  await flush();
  assert.equal(calls.length, 1, 'shortcut and Save share one pending content commit');
  assert.equal(calls[0].url, `${input.mount}/__native_pages/save_content`);
  assert.deepEqual(calls[0].args, input.expected_args, 'body save sends a sparse content patch');
  fire(input.textarea.input, text('Newer body draft'));
  fire(input.textarea.input, text('Original body'));
  resolveSave();
  await flush();
  assert.equal(readInput(), 'Original body', 'editing away and back to the old canonical value survives the pending commit');
  fire(input.cancel);
  assert.equal(readInput(), 'Updated body', 'Cancel restores the newly committed canonical body');
  fire(input.mode);
  fire(input.textarea.input, text('Preview body'));
  fire(input.mode);
  await flush();
  assert.equal(calls.length, 2, 'Edit-to-Preview invokes the shared content commit');
  assert.equal(calls[1].url, `${input.mount}/__native_pages/save_content`);
  assert.ok(JSON.stringify(calls[1].args).includes('Preview body'),
    'mode commit sends the current content draft through the sparse procedure');
  resolveSave();
  await flush();
  const hidden = vm.runInNewContext(`cx => (${input.textarea.hidden})`, context)(cx);
  assert.equal(unbox(hidden && typeof hidden.dehydrate === 'function' ? hidden.dehydrate() : hidden), true,
    'Cancel closes body edit mode');
  controller.abort();
  process.stdout.write(JSON.stringify({passed: true}));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
