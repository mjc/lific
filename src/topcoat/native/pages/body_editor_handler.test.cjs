'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const calls = [];
const resolveSaves = [];
const fixture = handlerFixture(input.signals, (url, options) => {
  calls.push({url: new URL(url, 'http://localhost').pathname, args: JSON.parse(options.body)});
  const reply = [input.reply, input.second_reply, input.third_reply, input.fourth_reply][calls.length - 1];
  return new Promise(resolve => { resolveSaves[calls.length - 1] = () => resolve({ok: true, json: async () => reply}); });
}, input.browser_source);
const {cx, context, controller, handler} = fixture;
context.CustomEvent = class { constructor(type, options = {}) { this.type = type; Object.assign(this, options); } };
context.document.documentElement.getAttribute = name => name === 'data-topcoat-runtime-prefix' ? input.mount : '';
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
const key = (name, {ctrlKey = false, metaKey = false} = {}) => ({
  type: 'keydown', key: name, ctrlKey, metaKey, cancelable: true,
  defaultPrevented: false, preventDefault() { this.defaultPrevented = true; },
});
const flush = async () => { for (let i = 0; i < 60; i++) await Promise.resolve(); };
const unbox = value => { while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v; return value; };
const inputSignal = input.textarea.binding;
const read = expression => {
  const result = vm.runInNewContext(`cx => (${expression})`, context)(cx);
  return unbox(result && typeof result.dehydrate === 'function' ? result.dehydrate() : result);
};
const readInput = () => read(inputSignal);
const referenced = new Set();
const originalSignal = cx.signal.bind(cx);
cx.signal = id => { referenced.add(id); return originalSignal(id); };
const bodyDraftId = (() => {
  const before = new Set(referenced);
  vm.runInNewContext(`cx => (${inputSignal})`, context)(cx);
  return [...referenced].find(id => !before.has(id));
})();
const canonicalBodyId = Object.keys(input.signals).find(id =>
  id !== bodyDraftId && unbox(input.signals[id]) === 'Original body'
);
assert.ok(canonicalBodyId, 'SSR exposes a distinct canonical body baseline');
const assertModeClasses = () => {
  for (const binding of [input.mode_edit_class, input.mode_preview_class]) {
    const classes = read(binding).split(/\s+/);
    for (const token of ['inline-flex', 'items-center', 'rounded-full', 'px-2.5', 'py-1',
      'text-body-sm', 'hover:bg-[var(--bg-subtle)]', 'focus-visible:outline']) {
      assert.ok(classes.includes(token), `mode class binding preserves ${token}`);
    }
  }
};
(async () => {
  assertModeClasses();
  assert.equal(read(input.mode_edit_pressed), 'false', 'Edit is not selected on initial Preview');
  assert.equal(read(input.mode_preview_pressed), 'true', 'Preview is selected on initial render');
  assert.equal(read(input.mode_group_hidden), false, 'nonempty body shows the segmented mode control');
  assert.equal(read(input.save_label), 'Save', 'idle body Save label matches Main');
  fire(input.mode_edit);
  await flush();
  assertModeClasses();
  assert.equal(read(input.textarea.hidden), false, 'the Edit control opens the body editor');
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
  assert.equal(read(input.save_label), 'Saving...', 'only a body Save shows Saving feedback');
  assert.equal(calls[0].url, `${input.mount}/__native_pages/save_content`);
  assert.deepEqual(calls[0].args, input.expected_args, 'body save sends a sparse content patch');
  fire(input.textarea.input, text('Newer body draft'));
  fire(input.textarea.input, text('Original body'));
  resolveSaves[0]();
  await flush();
  assert.equal(readInput(), 'Original body', 'editing away and back to the old canonical value survives the pending commit');
  assert.equal(read(input.save_label), 'Save', 'completed Save clears its own busy label');
  fire(input.cancel);
  assert.equal(readInput(), 'Updated body', 'Cancel restores the newly committed canonical body');
  fire(input.mode_edit);
  fire(input.textarea.input, text('Preview body'));
  fire(input.mode_preview);
  await flush();
  assert.equal(calls.length, 2, 'Edit-to-Preview invokes the shared content commit');
  assert.equal(calls[1].url, `${input.mount}/__native_pages/save_content`);
  assert.deepEqual(calls[1].args, input.second_expected_args,
    'Preview uses a second sparse write with the canonical sequence returned by the first write');
  assert.equal(calls[1].args[2], 'Preview body');
  resolveSaves[1]();
  await flush();
  assertModeClasses();
  assert.equal(unbox(cx.signal(canonicalBodyId).get().dehydrate()), 'Preview body',
    'the second real procedure reply advances the canonical body baseline');
  assert.equal(readInput(), 'Preview body', 'Preview completion adopts the submitted body');
  fire(input.mode_edit);
  fire(input.textarea.input, text('Preview body'));
  fire(input.save);
  assert.equal(calls.length, 2, 'saving an unchanged body is a local no-op');
  assert.equal(read(input.textarea.hidden), true, 'unchanged Save closes edit mode');
  fire(input.mode_edit);
  fire(input.textarea.input, text('Discarded body'));
  const escape = key('Escape');
  fire(input.textarea.keydown, escape);
  assert.equal(escape.defaultPrevented, true, 'Escape is consumed while editing');
  assert.equal(calls.length, 2, 'Escape discards without a POST');
  assert.equal(readInput(), 'Preview body', 'Escape restores the canonical body draft');
  assert.equal(read(input.textarea.hidden), true, 'Escape closes edit mode');
  fire(input.mode_edit);
  fire(input.textarea.input, text('Saved by shortcut'));
  const shortcutSave = key('s', {ctrlKey: true});
  fire(input.textarea.keydown, shortcutSave);
  assert.equal(shortcutSave.defaultPrevented, true, 'Ctrl+S is consumed while editing');
  await flush();
  assert.equal(calls.length, 3, 'Ctrl+S starts one content write');
  assert.deepEqual(calls[2].args, input.third_expected_args,
    'the next write uses the sequence from the second real procedure reply');
  resolveSaves[2]();
  await flush();
  assert.equal(readInput(), 'Saved by shortcut');
  fire(input.mode_edit);
  fire(input.textarea.input, text(''));
  const blankSave = key('s', {metaKey: true});
  fire(input.textarea.keydown, blankSave);
  await flush();
  assert.equal(calls.length, 4, 'a changed empty body is saved');
  assert.deepEqual(calls[3].args, input.fourth_expected_args);
  assert.equal(calls[3].args[2], '');
  resolveSaves[3]();
  await flush();
  assert.equal(readInput(), '', 'the blank body reply becomes canonical');
  assert.equal(read(input.mode_group_hidden), true, 'trimmed empty body hides the toolbar mode control');
  assert.equal(read(input.empty_cta_hidden), false, 'trimmed empty body shows the in-body CTA');
  assert.equal(read(input.preview_hidden), true, 'empty body hides the Markdown preview');
  assert.equal(input.empty_cta_text.trim(), 'Click to start writing...', 'empty CTA matches Main copy');
  const hidden = vm.runInNewContext(`cx => (${input.textarea.hidden})`, context)(cx);
  assert.equal(unbox(hidden && typeof hidden.dehydrate === 'function' ? hidden.dehydrate() : hidden), true,
    'Cancel closes body edit mode');
  controller.abort();
  const snapshot = () => Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, JSON.parse(JSON.stringify(cx.signal(id).dehydrate()))]));
  const retired = snapshot();
  fire(input.textarea.input, text('Retired input'));
  fire(input.mode_edit);
  fire(input.cancel);
  fire(input.mode_preview);
  fire(input.save);
  fire(input.textarea.keydown, key('s', {ctrlKey: true}));
  await flush();
  assert.deepEqual(snapshot(), retired, 'retained body controls cannot mutate a disposed Page');
  assert.equal(calls.length, 4, 'retained controls cannot start another save after disposal');
  process.stdout.write(JSON.stringify({passed: true}));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
