'use strict';

// Capture the request emitted by the real plan-anchor handler through the
// packaged Topcoat runtime. The Rust test sends its arguments to the native
// production procedure.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
const {cx, handler: emit} = handlerFixture(input.signals, async (url, options) => {
  const path = new URL(url, 'http://localhost').pathname;
  assert.ok(path.endsWith('/__native_plans/mutate'), `unexpected procedure ${path}`);
  assert.equal(options.method, 'POST');
  requests.push({path, arguments: JSON.parse(options.body)});
  return {ok: true, json: async () => input.response};
});
const initialSignals = Object.fromEntries(Object.keys(input.signals).map(id => [id, cx.signal(id).dehydrate().v]));
const handler = emit(input.handler);
handler(cx.event({type: 'click', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {}}));

async function run() {
  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
  assert.equal(requests.length, 1, 'clearing the anchor makes one procedure request');
  const signals = Object.fromEntries(Object.keys(input.signals).map(id => [id, cx.signal(id).dehydrate().v]));
  const changedSignalIds = Object.keys(input.signals).filter(id =>
    JSON.stringify(signals[id]) !== JSON.stringify(initialSignals[id]));
  assert.equal(changedSignalIds.length, 1, 'successful anchor clear refreshes only its owner');
  const revision = changedSignalIds[0];
  assert.equal(initialSignals[revision].v, '0', 'owner revision starts at zero');
  assert.equal(signals[revision].v, '1', 'owner revision increments after the saved response');
  for (const [id, value] of Object.entries(initialSignals)) {
    const isString = typeof value === 'string' ||
      (value && typeof value === 'object' && ['String', 'str'].includes(value.t));
    if (isString) {
      assert.deepEqual(signals[id], value,
        'saving the anchor preserves title and step drafts');
    }
  }
  process.stdout.write(JSON.stringify({
    ...requests[0], signals, changed_signal_ids: changedSignalIds,
    revision_before: initialSignals[revision].v,
    revision_after: signals[revision].v,
  }));
}
run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
