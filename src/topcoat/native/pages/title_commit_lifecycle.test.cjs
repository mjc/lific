'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const unbox = value => { while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v; return value; };
const flush = async () => { for (let i = 0; i < 60; i++) await Promise.resolve(); };
const fixtureFor = mode => {
  const calls = [], toasts = [];
  let settle, focuses = 0;
  const fixture = handlerFixture(input.signals, (url, options) => {
    calls.push({url: new URL(url, 'http://localhost').pathname, body: JSON.parse(options.body)});
    return new Promise((resolve, reject) => { settle = result => result === 'reject'
      ? reject(new Error('offline'))
      : resolve({ok: true, json: async () => input[mode]}); });
  }, input.browser_source);
  const {cx, context, controller, handler} = fixture;
  context.document.documentElement.getAttribute = name => name === 'data-topcoat-runtime-prefix' ? input.mount : '';
  context.document.getElementById = () => ({focus() { focuses++; }});
  context.window.dispatchEvent = event => { if (event.type === 'lific:native-toast-error') toasts.push(event); return true; };
  const signalIds = new Set();
  const originalSignal = cx.signal.bind(cx);
  cx.signal = id => { signalIds.add(id); return originalSignal(id); };
  const signalFor = expression => {
    const prior = new Set(signalIds);
    vm.runInNewContext(`cx => (${expression})`, context)(cx);
    const found = [...signalIds].filter(id => !prior.has(id));
    assert.equal(found.length, 1);
    return found[0];
  };
  const titleId = signalFor(input.title_binding);
  const value = id => unbox(cx.signal(id).get().dehydrate());
  const read = expression => {
    const result = vm.runInNewContext(`cx => (${expression})`, context)(cx);
    return unbox(result && typeof result.dehydrate === 'function' ? result.dehydrate() : result);
  };
  const snapshot = () => JSON.stringify(Object.fromEntries(Object.keys(input.signals).map(id => [id, cx.signal(id).get().dehydrate()])));
  const fire = (source, event) => handler(source)(cx.event(event));
  const text = value => ({type: 'input', target: {value}, cancelable: true, preventDefault() {}});
  const key = key => ({type: 'keydown', key, cancelable: true, preventDefault() {}});
  return {calls, toasts, controller, fire, text, key, value, read, snapshot, titleId,
    settle: result => settle(result), focusCount: () => focuses};
};
const start = test => {
  test.fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  test.fire(input.editor.input, test.text('Lifecycle title'));
  test.fire(input.editor.keydown, test.key('Enter'));
};
(async () => {
  const queued = fixtureFor('saved');
  start(queued);
  queued.controller.abort();
  await flush();
  assert.equal(queued.calls.length, 0, 'disposal before the procedure starts sends no request');
  for (const result of ['saved', 'reject']) {
    const late = fixtureFor('saved');
    start(late);
    await flush();
    assert.equal(late.calls.length, 1);
    late.controller.abort();
    const before = late.snapshot(), focusBefore = late.focusCount();
    late.fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
    late.fire(input.editor.input, late.text('Ignored after disposal'));
    late.settle(result === 'reject' ? 'reject' : 'resolve');
    await flush();
    assert.equal(late.calls.length, 1, 'disposed events send no further writes');
    assert.equal(late.snapshot(), before, `late ${result} and disposed input leave all signals unchanged`);
    assert.equal(late.toasts.length, 0, 'disposed completion emits no toast');
    assert.equal(late.focusCount(), focusBefore, 'disposed trigger does not focus');
  }
  const conflict = fixtureFor('conflict');
  start(conflict);
  await flush();
  conflict.fire(input.trigger, {type: 'click', cancelable: true, preventDefault() {}});
  conflict.fire(input.editor.input, conflict.text('Newer conflict draft'));
  conflict.settle('resolve');
  await flush();
  assert.equal(conflict.calls.length, 1);
  assert.equal(conflict.value(conflict.titleId), 'Newer conflict draft');
  assert.equal(conflict.read(input.editor_hidden_binding), false, 'newer title draft stays open on conflict');
  assert.equal(conflict.toasts.length, 1, 'conflict remains visible');
  process.stdout.write(JSON.stringify({passed: true}));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
