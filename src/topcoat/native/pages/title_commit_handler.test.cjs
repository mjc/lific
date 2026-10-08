'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const calls = [];
const fixture = handlerFixture(input.signals, async (url, options) => {
  calls.push({url: new URL(url, 'http://localhost').pathname, body: JSON.parse(options.body)});
  return {ok: true, json: async () => input.reply};
}, input.browser_source);
const {cx, context, controller, handler} = fixture;
context.document.documentElement.getAttribute = name =>
  name === 'data-topcoat-mount' ? input.mount : '';
context.document.getElementById = () => ({focus() {}});
const fire = (source, event) => handler(source)(cx.event(event));
const key = (name, {ctrlKey = false, metaKey = false} = {}) => ({
  type: 'keydown', key: name, ctrlKey, metaKey, cancelable: true,
  preventDefault() { this.prevented = true; },
});
const inputEvent = value => ({type: 'input', target: {value}, cancelable: true, preventDefault() {}});
const flush = async () => { for (let i = 0; i < 50; i++) await Promise.resolve(); };
const bindingSignal = source => {
  const ids = new Set();
  const original = cx.signal.bind(cx);
  cx.signal = id => { ids.add(id); return original(id); };
  vm.runInNewContext(`cx => (${source})`, context)(cx);
  cx.signal = original;
  assert.equal(ids.size, 1, 'title binding resolves one signal');
  return [...ids][0];
};
const titleId = bindingSignal(input.title_binding);
const value = id => {
  let current = cx.signal(id).dehydrate();
  while (current && typeof current === 'object' && 'v' in current) current = current.v;
  return current;
};
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
  fire(input.editor.input, inputEvent('  Renamed title  '));
  const enter = key('Enter');
  fire(input.editor.keydown, enter);
  fire(input.editor.blur, {type: 'blur'});
  await flush();
  assert.equal(enter.prevented, true, 'Enter prevents form submission');
  assert.equal(calls.length, 1, 'Enter followed by blur commits once');
  assert.equal(calls[0].url, `${input.mount}/__native_pages/save_title`);
  assert.deepEqual(calls[0].body, input.expected_arguments, 'the title write carries only title and expected sequence');
  assert.equal(value(titleId), 'Renamed title');

  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('Discarded title'));
  fire(input.editor.keydown, key('Escape'));
  fire(input.editor.blur, {type: 'blur'});
  await flush();
  assert.equal(calls.length, 1, 'Escape followed by blur does not save');

  fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  fire(input.editor.input, inputEvent('Shortcut title'));
  const shortcut = key('s', {metaKey: true});
  fire(input.editor.keydown, shortcut);
  await flush();
  assert.equal(shortcut.prevented, true, 'Cmd+S prevents browser Save');
  assert.equal(calls.length, 2, 'Cmd+S commits the title');
  assert.equal(calls[1].url, `${input.mount}/__native_pages/save_title`);
  controller.abort();
  process.stdout.write(JSON.stringify({passed: true, calls: calls.length}));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
