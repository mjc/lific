'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
let settle;
let reject;
let calls = 0;
const fixture = handlerFixture(input.signals, () => {
  calls += 1;
  return new Promise((resolve, fail) => { settle = resolve; reject = fail; });
}, input.browser_source);
const {cx, context, handler} = fixture;
context.CustomEvent = class { constructor(type, options = {}) { this.type = type; Object.assign(this, options); } };
context.document.documentElement.getAttribute = name => name === 'data-topcoat-runtime-prefix' ? input.mount : '';
context.document.querySelector = () => null;
const fire = (source, event = {type: 'click', cancelable: true, preventDefault() {}}) => handler(source)(cx.event(event));
const text = value => ({type: 'input', target: {value}, cancelable: true, preventDefault() {}});
const flush = async () => { for (let i = 0; i < 60; i++) await Promise.resolve(); };
const unbox = value => { while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v; return value; };
const read = expression => {
  const result = vm.runInNewContext(`cx => (${expression})`, context)(cx);
  return unbox(result && typeof result.dehydrate === 'function' ? result.dehydrate() : result);
};
(async () => {
  fire(input.mode);
  fire(input.textarea.input, text('Draft cancelled while pending'));
  fire(input.save);
  await flush();
  assert.equal(calls, 1, 'one body request is pending');
  fire(input.textarea.keydown, {type: 'keydown', key: 'Escape', cancelable: true, preventDefault() {}});
  assert.equal(read(input.textarea.hidden), true, 'Escape closes the editor while the request is pending');
  assert.equal(read(input.textarea.binding), 'Original body', 'Cancel restores the pre-request canonical draft');
  if (input.scenario === 'success') {
    settle({ok: true, json: async () => input.success_reply});
  } else if (input.scenario === 'conflict') {
    settle({ok: true, json: async () => input.conflict_reply});
  } else {
    reject(new Error('network failure'));
  }
  await flush();
  assert.equal(read(input.textarea.hidden), true, 'late completion never reopens the cancelled editor');
  assert.equal(read(input.textarea.binding), 'Original body', 'late completion preserves the cancelled draft');
  assert.equal(calls, 1, 'settlement does not retry the write');
  process.stdout.write(JSON.stringify({passed: true}));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
