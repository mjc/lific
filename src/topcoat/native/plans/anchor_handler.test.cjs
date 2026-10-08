'use strict';

// Exercise shared plan actions through the packaged runtime. Rust sends the
// captured arguments through the authenticated production procedure.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const flush = async () => {
  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
};

async function scenario(kind) {
  const requests = [];
  const navigations = [];
  let resolveRequest;
  let rejectRequest;
  const {cx, controller, handler: emit} = handlerFixture(input.signals, (url, options) => {
    const path = new URL(url, 'http://localhost').pathname;
    assert.ok(path.endsWith('/__native_plans/mutate'), `unexpected procedure ${path}`);
    assert.equal(options.method, 'POST');
    requests.push({path, arguments: JSON.parse(options.body)});
    if (kind.startsWith('retired_')) {
      return new Promise((resolve, reject) => {
        resolveRequest = resolve;
        rejectRequest = reject;
      });
    }
    return Promise.resolve({ok: true, json: async () => input.response});
  }, input.browser_source);
  cx.navigate = destination => navigations.push(destination);
  const snapshot = () => Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, cx.signal(id).dehydrate().v]));
  const initialSignals = snapshot();
  const handler = emit(input.handler);
  const click = () => handler(cx.event({type: 'click', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {}}));
  if (kind === 'disposed') controller.abort();
  click();
  if (kind === 'disposed_before_request') controller.abort();
  if (kind === 'success') click();
  await flush();
  if (kind === 'disposed' || kind === 'disposed_before_request') {
    assert.equal(requests.length, 0, 'a retired owner cannot start a procedure');
    assert.equal(navigations.length, 0, 'a retired owner cannot navigate');
    if (kind === 'disposed') assert.deepEqual(snapshot(), initialSignals);
    return;
  }
  assert.equal(requests.length, 1, 'busy state suppresses a duplicate action');
  if (kind.startsWith('retired_')) {
    const beforeDisposal = snapshot();
    controller.abort();
    if (kind === 'retired_success') resolveRequest({ok: true, json: async () => input.response});
    else rejectRequest(new Error('offline'));
    await flush();
    assert.deepEqual(snapshot(), beforeDisposal, 'retired owners ignore late success and failure');
    assert.equal(navigations.length, 0, 'retired owners cannot navigate after a late reply');
    return;
  }
  const signals = snapshot();
  const changedSignalIds = Object.keys(input.signals).filter(id =>
    JSON.stringify(signals[id]) !== JSON.stringify(initialSignals[id]));
  assert.equal(changedSignalIds.length, 1, 'success refreshes only the owning revision and retains drafts');
  const revision = changedSignalIds[0];
  assert.equal(initialSignals[revision].v, '0', 'owner revision starts at zero');
  assert.equal(signals[revision].v, '1', 'owner revision increments after the saved response');
  return {
    ...requests[0], signals, changed_signal_ids: changedSignalIds,
    revision_before: initialSignals[revision].v,
    revision_after: signals[revision].v,
  };
}
async function run() {
  for (const kind of ['disposed', 'disposed_before_request', 'retired_success', 'retired_failure']) await scenario(kind);
  process.stdout.write(JSON.stringify(await scenario('success')));
}
run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
