'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('./handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture({}, async () => { throw Error('JSON bindings perform no I/O'); }, input.browser_source);
const browser = runtime.context.__lificNativeMounts.browser(runtime.cx);
for (const item of input.cases) {
  const wire = runtime.cx.hydrate(item.wire);
  const key = runtime.cx.hydrate(item.key);
  const read = browser.json_array_field(wire, key);
  const written = browser.json_set_array(wire, key, runtime.cx.hydrate(item.values));
  assert.deepEqual(JSON.parse(read.toString()), item.read);
  assert.deepEqual(JSON.parse(written.toString()), item.written);
}
runtime.controller.abort();
process.stdout.write(JSON.stringify({passed: input.cases.length}));
