'use strict';

// Execute a rendered plan-tree expansion handler through the packaged runtime.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
if (input.mode === 'busy') {
  for (const [id, value] of Object.entries(input.signals)) {
    if (typeof value === 'boolean') input.signals[id] = true;
  }
}
const {cx, handler: emit} = handlerFixture(input.signals, async (url, options) => {
  const path = new URL(url, 'http://localhost').pathname;
  requests.push({path, method: options.method});
  return {ok: true, json: async () => true};
}, input.browser_source);
const before = Object.fromEntries(Object.keys(input.signals).map(id => [id, cx.signal(id).dehydrate().v]));
const handler = emit(input.handler);
handler(cx.event({type: 'click', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {}}));

async function run() {
  for (let attempt = 0; attempt < 20; attempt += 1) await Promise.resolve();
  assert.equal(
    requests.filter(request => request.path.endsWith('/__native_plans/mutate')).length,
    0,
    'expanding or collapsing a step does not call the mutation procedure',
  );
  const signals = Object.fromEntries(Object.keys(input.signals).map(id => [id, cx.signal(id).dehydrate().v]));
  if (input.mode === 'busy') {
    assert.deepEqual(signals, before, 'expansion is ignored while a plan action is busy');
  } else {
    assert.notDeepEqual(signals, before, 'the click updates the rendered expansion state');
  }
  process.stdout.write(JSON.stringify({signals, requests}));
}

run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
