'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
const runtime = handlerFixture(input.signals, async (url, options) => {
  requests.push({path: new URL(url, 'http://localhost').pathname, body: JSON.parse(options.body)});
  return new Promise(() => {});
}, input.browser_source);
const {cx, context} = runtime;
context.document.documentElement.getAttribute = name =>
  name === 'data-topcoat-runtime-prefix' ? input.mount : '';
runtime.handler(input.input_handler)(cx.event({type: 'input', target: {value: input.draft}}));
const disabled = vm.runInNewContext(`cx => (${input.disabled_binding})`, context);
const disabledWire = disabled(cx);
let disabledValue = disabledWire && typeof disabledWire.dehydrate === 'function'
  ? disabledWire.dehydrate() : disabledWire;
while (disabledValue && typeof disabledValue === 'object' && Object.hasOwn(disabledValue, 'v'))
  disabledValue = disabledValue.v;
assert.equal(disabledValue, input.button_disabled,
  'current normalized identifier has Main disabled behavior');
if (input.submit) runtime.handler(input.rename_handler)(cx.event({type: 'click'}));

async function run() {
  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
  assert.equal(requests.length, input.submit ? 1 : 0, 'unchanged identifiers stay disabled and issue no request');
  if (input.submit) {
    assert.equal(requests[0].path, `${input.mount}/__native_overview/save_field`);
    const wireValue = value => {
      while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
      return value;
    };
    assert.equal(wireValue(requests[0].body[2]), 'identifier');
    assert.equal(wireValue(requests[0].body[3]), input.expected,
      'the emitted procedure receives Main-normalized identifier text');
  }
  process.stdout.write(JSON.stringify({requests}));
}
run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
