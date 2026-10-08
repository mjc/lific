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
const dispatch = (runtime, source, action, owner = runtime.cx) => runtime.handler(source, owner)(owner.event({
  type: 'click',
  target: {closest: () => ({getAttribute: () => action})},
  currentTarget: {}, preventDefault() {}, stopPropagation() {},
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
  successFixture.handler(input.input, pane.cx)(pane.cx.event({type: 'input', target: {value: 'Saved while pane closes'}}));
  dispatch(successFixture, input.owner, 'save');
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
  failureFixture.handler(input.input, failurePane)(failurePane.event({type: 'input', target: {value: 'Failed draft'}}));
  dispatch(failureFixture, input.owner, 'save');
  await flush();
  failurePaneController.abort();
  rejectFailure(new Error('fixture request failure'));
  await flush();
  const failureSnapshot = Object.values(snapshot(failureFixture.cx));
  assert.equal(failureRequests.length, 1, 'a failed owner save is sent once');
  assert.ok(failureSnapshot.includes(input.initial_description),
    'a failed save keeps the canonical description unchanged after the pane closes');
  const afterFailedRead = snapshot(failureFixture.cx);
  dispatch(failureFixture, input.owner, 'preview');
  await flush();
  assert.equal(failureRequests.length, 1,
    'the already-selected Preview radio does not resubmit a failed hidden draft');
  assert.deepEqual(snapshot(failureFixture.cx), afterFailedRead,
    'selecting read mode after failure leaves canonical and draft state untouched');
  const retryPane = paneContext(failureFixture);
  dispatch(failureFixture, input.owner, 'edit');
  failureFixture.handler(input.input, retryPane.cx)(retryPane.cx.event({
    type: 'input', target: {value: 'Retry after failure'},
  }));
  dispatch(failureFixture, input.owner, 'save');
  await flush();
  assert.equal(failureRequests.length, 2,
    'failure clears busy state so the owner can start a later save');
  rejectFailure(new Error('retry fixture failure'));
  await flush();

  let releaseAfterDispose;
  const disposedRequests = [];
  const disposed = handlerFixture(input.signals, (url, options) => {
    disposedRequests.push({url: String(url), arguments: JSON.parse(options.body)});
    return new Promise(resolve => { releaseAfterDispose = resolve; });
  }, input.browser_source);
  disposed.context.document.documentElement.getAttribute = () => input.mount;
  disposed.handler(input.input)(disposed.cx.event({type: 'input', target: {value: 'Abandoned draft'}}));
  dispatch(disposed, input.owner, 'save');
  await flush();
  const beforeRouteDisposal = snapshot(disposed.cx);
  disposed.controller.abort();
  releaseAfterDispose({ok: true, json: async () => null});
  await flush();
  assert.equal(disposedRequests.length, 1, 'the already-started owner request reached the endpoint');
  assert.deepEqual(snapshot(disposed.cx), beforeRouteDisposal,
    'a disposed route owner receives no stale completion writes');

  let rejectAfterDispose;
  const rejected = handlerFixture(input.signals, () =>
    new Promise((_, reject) => { rejectAfterDispose = reject; }), input.browser_source);
  rejected.context.document.documentElement.getAttribute = () => input.mount;
  rejected.handler(input.input)(rejected.cx.event({type: 'input', target: {value: 'Late failure'}}));
  dispatch(rejected, input.owner, 'save');
  await flush();
  const beforeLateFailure = snapshot(rejected.cx);
  rejected.controller.abort();
  rejectAfterDispose(new Error('late fixture failure'));
  await flush();
  assert.deepEqual(snapshot(rejected.cx), beforeLateFailure,
    'a disposed route owner receives no stale failure writes');

  const preDisposed = handlerFixture(input.signals, async () => {
    assert.fail('a disposed owner must not send a Save request');
  }, input.browser_source);
  preDisposed.context.document.documentElement.getAttribute = () => input.mount;
  const beforePreDisposed = snapshot(preDisposed.cx);
  preDisposed.controller.abort();
  dispatch(preDisposed, input.owner, 'edit');
  dispatch(preDisposed, input.owner, 'save');
  await flush();
  assert.deepEqual(snapshot(preDisposed.cx), beforePreDisposed,
    'pre-disposed owners do not issue requests or mutate state');
  return {
    successRequests: successRequests.length,
    successSignals: successSnapshot,
    failureRequests: failureRequests.length,
    failureSignals: snapshot(failureFixture.cx),
    disposedRequests: disposedRequests.length,
  };
}

(async () => {
  if (input.phase === 'enter_edit') {
    dispatch(fixture, input.owner, 'edit');
    process.stdout.write(JSON.stringify({signals: snapshot(fixture.cx), requests: requests.length}));
    return;
  }
  if (input.phase === 'shortcut') {
    let prevented = false;
    fixture.handler(input.shortcut)(fixture.cx.event({
      type: 'keydown', key: 'e', ctrlKey: false, metaKey: false, altKey: false,
      target: {closest: () => null}, preventDefault() { prevented = true; },
    }));
    assert.equal(prevented, true, 'the E shortcut prevents its browser default');
    const signals = snapshot(fixture.cx);
    assert.notDeepEqual(signals, input.signals,
      'the emitted E shortcut enters edit mode through the durable owner');
    process.stdout.write(JSON.stringify({signals}));
    return;
  }
  if (input.phase === 'editor_mount') {
    let focused = '';
    fixture.context.document.querySelector = selector => ({
      focus() { focused = selector; },
    });
    fixture.handler(input.mount_handler)(fixture.cx.event({type: 'mount'}));
    assert.equal(focused, '[data-native-module-description-editor]',
      'the actual editor mount handler focuses after the shard inserts the textarea');
    process.stdout.write(JSON.stringify({focused}));
    return;
  }
  if (input.phase === 'owner_lifecycle') {
    process.stdout.write(JSON.stringify(await runLifecycle()));
    return;
  }

  dispatch(fixture, input.owner, 'cancel');
  const beforeTab = snapshot(fixture.cx);
  let tabPrevented = false;
  fixture.handler(input.owner)(fixture.cx.event({
    type: 'keydown', key: 'Tab', target: {closest: () => ({getAttribute: () => 'edit'})},
    preventDefault() { tabPrevented = true; },
  }));
  assert.deepEqual(snapshot(fixture.cx), beforeTab,
    'Tab on the Edit radio does not change description mode');
  assert.equal(tabPrevented, false, 'Tab on a mode radio keeps its normal browser behavior');
  dispatch(fixture, input.owner, 'edit');

  const beforeSaveAs = snapshot(fixture.cx);
  fixture.handler(input.owner)(fixture.cx.event({
    type: 'keydown', key: 'S', ctrlKey: true, ctrl_key: true, shiftKey: true,
    target: {closest: selector => selector.includes('data-native-module-description-editor')
      ? {} : null}, preventDefault() {},
  }));
  await flush();
  assert.equal(requests.length, 0, 'Ctrl+Shift+S remains the browser Save As shortcut');
  assert.deepEqual(snapshot(fixture.cx), beforeSaveAs,
    'Save As does not mutate the description');

  const altRequests = [];
  const altFixture = handlerFixture(input.signals, async (url, options) => {
    altRequests.push({url: String(url), arguments: JSON.parse(options.body)});
    return {ok: true, json: async () => null};
  }, input.browser_source);
  altFixture.context.document.documentElement.getAttribute = () => input.mount;
  dispatch(altFixture, input.owner, 'edit');
  altFixture.handler(input.input)(altFixture.cx.event({type: 'input', target: {value: 'Alt save body'}}));
  let altSavePrevented = false;
  altFixture.handler(input.owner)(altFixture.cx.event({
    type: 'keydown', key: 's', ctrlKey: true, altKey: true,
    target: {closest: selector => selector.includes('data-native-module-description-editor')
      ? {} : null},
    preventDefault() { altSavePrevented = true; },
  }));
  await flush();
  assert.equal(altRequests.length, 1, 'Ctrl+Alt+S in the textarea follows Main and saves');
  assert.equal(altRequests[0].arguments[4], 'Alt save body');
  assert.equal(altSavePrevented, true, 'handled Ctrl+Alt+S prevents the browser default');

  const beforeUnrelatedClick = snapshot(fixture.cx);
  let unrelatedClickPrevented = false;
  fixture.handler(input.owner)(fixture.cx.event({
    type: 'click',
    target: {closest: () => null},
    currentTarget: {},
    preventDefault() { unrelatedClickPrevented = true; },
  }));
  assert.equal(unrelatedClickPrevented, false,
    'the description owner leaves ordinary Markdown link clicks alone');
  assert.deepEqual(snapshot(fixture.cx), beforeUnrelatedClick,
    'an unrelated description click does not mutate its owner state');

  fixture.handler(input.input)(fixture.cx.event({type: 'input', target: {value: 'Discarded description'}}));
  dispatch(fixture, input.owner, 'cancel');
  await flush();
  assert.equal(requests.length, 0, 'Edit and Cancel do not call the module update procedure');
  assert.ok(Object.values(snapshot(fixture.cx)).includes(input.initial_description),
    'Cancel restores the canonical description');

  dispatch(fixture, input.owner, 'edit');
  fixture.handler(input.input)(fixture.cx.event({type: 'input', target: {value: 'Failed module body'}}));
  failNextFetch = true;
  const beforeFailure = navigations.length;
  dispatch(fixture, input.owner, 'save');
  await flush();
  assert.equal(requests.length, 1, 'a failed Save attempts the production procedure once');
  assert.equal(requests[0].arguments[3], 'description');
  assert.equal(requests[0].arguments[4], 'Failed module body');
  assert.equal(navigations.length, beforeFailure, 'failed Save does not navigate away');
  const canonicalAfterFailure = Object.values(snapshot(fixture.cx));
  const failedSaveNavigations = navigations.length - beforeFailure;
  assert.ok(canonicalAfterFailure.includes(input.initial_description),
    'a rejected update leaves the canonical description unchanged');

  dispatch(fixture, input.owner, 'edit');
  assert.ok(!Object.values(snapshot(fixture.cx)).includes('Failed module body'),
    're-entering edit mode starts from canonical content after failure');
  fixture.handler(input.input)(fixture.cx.event({type: 'input', target: {value: 'Preview committed body'}}));
  dispatch(fixture, input.owner, 'preview');
  await flush();
  assert.equal(requests.length, 2, 'Preview commits a changed description exactly once');
  assert.equal(requests[1].arguments[3], 'description');
  assert.equal(requests[1].arguments[4], 'Preview committed body');

  dispatch(fixture, input.owner, 'edit');
  fixture.handler(input.input)(fixture.cx.event({type: 'input', target: {value: 'Saved module body'}}));
  dispatch(fixture, input.owner, 'save');
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
