'use strict';

const assert = require('node:assert/strict');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const requests = [];
let failNextFetch = false;
const fixture = handlerFixture(input.signals, async (url, options) => {
  requests.push({url: String(url), arguments: JSON.parse(options.body)});
  if (failNextFetch) {
    failNextFetch = false;
    throw new Error('fixture request failure');
  }
  return {ok: true, json: async () => null};
}, input.browser_source);
fixture.context.document.documentElement.getAttribute = () => input.mount;
const navigations = [];
fixture.cx.navigate = destination => navigations.push(String(destination));
const fire = (source, cx, event = {}) => fixture.handler(source)(cx.event({
  type: 'click', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {}, ...event,
}));
const snapshot = cx => Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, cx.signal(id).dehydrate().v]));
const flush = async () => { for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve(); };

function paneContext(runtime) {
  const controller = new AbortController();
  const cx = Object.assign(new runtime.context.fixture.Context(runtime.registry), {
    abortSignal: controller.signal,
    event: event => new runtime.context.fixture.Event(event),
    navigate: runtime.cx.navigate,
  });
  return {cx, controller};
}

async function runLifecycle() {
  let releaseSuccess;
  const successRequests = [];
  const successFixture = handlerFixture(input.signals, (url, options) => {
    successRequests.push({url: String(url), arguments: JSON.parse(options.body)});
    return new Promise(resolve => { releaseSuccess = resolve; });
  }, input.browser_source);
  successFixture.context.document.documentElement.getAttribute = () => input.mount;
  successFixture.cx.navigate = destination => navigations.push(String(destination));
  const pane = paneContext(successFixture);
  successFixture.handler(input.input)(pane.cx.event({type: 'input', target: {value: 'Saved while pane closes'}}));
  successFixture.handler(input.save)(successFixture.cx.event({type: 'click', preventDefault() {}}));
  await flush();
  assert.equal(successRequests.length, 1, 'the durable owner dispatches the emitted Save once');
  pane.controller.abort();
  releaseSuccess({ok: true, json: async () => null});
  await flush();
  const successSnapshot = snapshot(successFixture.cx);
  assert.ok(Object.values(successSnapshot).includes('Saved while pane closes'),
    'replacing the editor pane does not cancel the owner save or canonical update');

  let rejectFailure;
  const failureRequests = [];
  const failureFixture = handlerFixture(input.signals, (url, options) => {
    failureRequests.push({url: String(url), arguments: JSON.parse(options.body)});
    return new Promise((_, reject) => { rejectFailure = reject; });
  }, input.browser_source);
  failureFixture.context.document.documentElement.getAttribute = () => input.mount;
  const failurePaneController = new AbortController();
  const failurePane = Object.assign(new failureFixture.context.fixture.Context(failureFixture.registry), {
    abortSignal: failurePaneController.signal,
    event: event => new failureFixture.context.fixture.Event(event),
    navigate: failureFixture.cx.navigate,
  });
  failureFixture.handler(input.edit)(failureFixture.cx.event({type: 'click', preventDefault() {}}));
  failureFixture.handler(input.input)(failurePane.event({type: 'input', target: {value: 'Failed draft'}}));
  failureFixture.handler(input.save)(failureFixture.cx.event({type: 'click', preventDefault() {}}));
  await flush();
  failurePaneController.abort();
  rejectFailure(new Error('fixture request failure'));
  await flush();
  const failureSnapshot = Object.values(snapshot(failureFixture.cx));
  assert.equal(failureRequests.length, 1, 'a failed owner save is sent once');
  assert.ok(failureSnapshot.includes(input.initial_description),
    'a failed save keeps the canonical description unchanged after the pane closes');

  let releaseAfterDispose;
  const disposedRequests = [];
  const disposed = handlerFixture(input.signals, (url, options) => {
    disposedRequests.push({url: String(url), arguments: JSON.parse(options.body)});
    return new Promise(resolve => { releaseAfterDispose = resolve; });
  }, input.browser_source);
  disposed.context.document.documentElement.getAttribute = () => input.mount;
  disposed.handler(input.edit)(disposed.cx.event({type: 'click', preventDefault() {}}));
  disposed.handler(input.input)(disposed.cx.event({type: 'input', target: {value: 'Abandoned draft'}}));
  disposed.handler(input.save)(disposed.cx.event({type: 'click', preventDefault() {}}));
  await flush();
  const beforeRouteDisposal = snapshot(disposed.cx);
  disposed.controller.abort();
  releaseAfterDispose({ok: true, json: async () => null});
  await flush();
  assert.equal(disposedRequests.length, 1, 'the already-started owner request reached the endpoint');
  assert.deepEqual(snapshot(disposed.cx), beforeRouteDisposal,
    'a disposed route owner receives no stale completion writes');

  const preDisposed = handlerFixture(input.signals, async () => {
    assert.fail('a disposed owner must not send a Save request');
  }, input.browser_source);
  preDisposed.context.document.documentElement.getAttribute = () => input.mount;
  const beforePreDisposed = snapshot(preDisposed.cx);
  preDisposed.controller.abort();
  for (const source of [input.edit, input.save]) {
    preDisposed.handler(source)(preDisposed.cx.event({type: 'click', preventDefault() {}}));
  }
  await flush();
  assert.deepEqual(snapshot(preDisposed.cx), beforePreDisposed,
    'pre-disposed owners do not issue requests or mutate state');
  return {successRequests: successRequests.length, failureRequests: failureRequests.length,
    disposedRequests: disposedRequests.length};
}

(async () => {
  if (input.phase === 'enter_edit') {
    fire(input.edit, fixture.cx);
    process.stdout.write(JSON.stringify({signals: snapshot(fixture.cx)}));
    return;
  }
  if (input.phase === 'owner_lifecycle') {
    process.stdout.write(JSON.stringify(await runLifecycle()));
    return;
  }

  fire(input.input, fixture.cx, {type: 'input', target: {value: 'Discarded description'}});
  fire(input.cancel, fixture.cx);
  await flush();
  assert.equal(requests.length, 0, 'Edit and Cancel do not call the module update procedure');
  assert.ok(Object.values(snapshot(fixture.cx)).includes(input.initial_description),
    'Cancel restores the canonical description');

  fire(input.edit, fixture.cx);
  fire(input.input, fixture.cx, {type: 'input', target: {value: 'Failed module body'}});
  failNextFetch = true;
  const beforeFailure = navigations.length;
  fire(input.save, fixture.cx);
  await flush();
  assert.equal(requests.length, 1, 'a failed Save attempts the production procedure once');
  assert.equal(requests[0].arguments[3], 'description');
  assert.equal(requests[0].arguments[4], 'Failed module body');
  assert.equal(navigations.length, beforeFailure, 'failed Save does not navigate away');
  const canonicalAfterFailure = Object.values(snapshot(fixture.cx));
  const failedSaveNavigations = navigations.length - beforeFailure;
  assert.ok(canonicalAfterFailure.includes(input.initial_description),
    'a rejected update leaves the canonical description unchanged');

  fire(input.edit, fixture.cx);
  assert.ok(!Object.values(snapshot(fixture.cx)).includes('Failed module body'),
    're-entering edit mode starts from canonical content after failure');
  fire(input.input, fixture.cx, {type: 'input', target: {value: 'Preview committed body'}});
  fire(input.preview, fixture.cx);
  await flush();
  assert.equal(requests.length, 2, 'Preview commits a changed description exactly once');
  assert.equal(requests[1].arguments[3], 'description');
  assert.equal(requests[1].arguments[4], 'Preview committed body');

  fire(input.edit, fixture.cx);
  fire(input.input, fixture.cx, {type: 'input', target: {value: 'Saved module body'}});
  fire(input.save, fixture.cx);
  await flush();
  assert.equal(requests.length, 3, 'explicit Save commits exactly once');
  assert.equal(requests[2].url, '/app/__native_modules/update');
  assert.equal(requests[2].arguments[3], 'description');
  assert.equal(requests[2].arguments[4], 'Saved module body');

  process.stdout.write(JSON.stringify({
    cancel_requests: 0,
    failed_save_requests: 1,
    failed_save_arguments: requests[0].arguments,
    failed_save_navigations: failedSaveNavigations,
    canonical_after_failure: canonicalAfterFailure,
    preview_save_requests: 1,
    preview_save_arguments: requests[1].arguments,
    explicit_save_requests: 1,
    explicit_save_url: requests[2].url,
    explicit_save_arguments: requests[2].arguments,
  }));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
