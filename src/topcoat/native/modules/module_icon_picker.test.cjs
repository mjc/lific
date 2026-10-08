'use strict';

const assert = require('node:assert/strict');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const flush = async () => { for (let i = 0; i < 60; i += 1) await Promise.resolve(); };
const snapshot = cx => Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, cx.signal(id).dehydrate().v]));

function fixture(fetch) {
  const runtime = handlerFixture(input.signals, fetch, input.browser_source);
  runtime.context.document.documentElement.getAttribute = () => input.mount;
  runtime.context.document.getElementById = () => ({
    focus() {},
    dispatchEvent(event) {
      if (event.type === 'native-project-icon-change' && input.change) {
        fire(runtime, input.change, event.type, {}, runtime.cx);
      }
    },
  });
  runtime.context.requestAnimationFrame = callback => callback();
  return runtime;
}

function fire(runtime, source, type, extra = {}, owner = runtime.cx) {
  return runtime.handler(source, owner)(owner.event({
    type,
    target: {},
    currentTarget: {},
    preventDefault() {},
    stopPropagation() {},
    ...extra,
  }));
}

function paneContext(runtime) {
  const controller = new AbortController();
  const cx = Object.assign(new runtime.context.fixture.Context(runtime.registry), {
    abortSignal: controller.signal,
    event: event => new runtime.context.fixture.Event(event),
  });
  return {cx, controller};
}

async function createWithSelectedIcon() {
  const requests = [];
  const navigations = [];
  const runtime = fixture(async (url, options) => {
    requests.push({url: String(url), arguments: JSON.parse(options.body)});
    return {ok: true, json: async () => input.create_response};
  });
  runtime.cx.navigate = destination => navigations.push(String(destination));
  fire(runtime, input.create, 'click');
  fire(runtime, input.trigger, 'click');
  fire(runtime, input.choice, 'click');
  assert.equal(requests.length, 0, 'choosing an icon updates the inline draft only');
  fire(runtime, input.name_input, 'input', {target: {value: 'Icon-selected module'}});
  fire(runtime, input.submit, 'submit');
  await flush();
  assert.equal(requests.length, 1, 'creating submits once after icon selection');
  assert.ok(requests[0].url.endsWith('/__native_modules/create'));
  assert.deepEqual(requests[0].arguments, input.expected_arguments,
    'the create procedure receives the picked icon value');
  assert.deepEqual(navigations, [input.expected_destination],
    'the successful create navigates with the returned module id');
  return {requests, signals: snapshot(runtime.cx), navigations};
}

async function mountedDetailSave() {
  const requests = [];
  const runtime = fixture(async (url, options) => {
    requests.push({url: String(url), arguments: JSON.parse(options.body)});
    return {ok: true, json: async () => null};
  });
  const pane = paneContext(runtime);
  fire(runtime, input.choice, 'click', {}, pane.cx);
  await flush();
  assert.equal(requests.length, 1, 'the mounted picker emits one icon update');
  assert.equal(requests[0].url, `${input.mount}/__native_modules/update`,
    'the emitted update preserves the mounted procedure destination');
  assert.deepEqual(requests[0].arguments, input.expected_arguments,
    'the mounted picker update preserves account, project, module, field, and icon');
  pane.controller.abort();
  return {requests};
}

async function saveDetailIcon() {
  const requests = [];
  let releaseSave;
  const runtime = fixture((url, options) => {
    requests.push({url: String(url), arguments: JSON.parse(options.body)});
    return new Promise(resolve => { releaseSave = resolve; });
  });
  const pane = paneContext(runtime);
  const iconSignalIds = Object.keys(input.signals)
    .filter(id => runtime.cx.signal(id).dehydrate().v === 'lucide:Folder');
  assert.ok(iconSignalIds.length >= 2,
    'the picker draft and canonical module icon signals are present');
  fire(runtime, input.trigger, 'click');
  fire(runtime, input.choice, 'click', {}, pane.cx);
  await flush();
  assert.equal(requests.length, 1, 'selecting a picker item dispatches an immediate save');
  assert.ok(iconSignalIds.every(id => snapshot(runtime.cx)[id] === 'lucide:Folder'),
    'the persisted icon stays canonical while the request is pending');
  fire(runtime, input.choice, 'click', {}, pane.cx);
  await flush();
  assert.equal(requests.length, 1, 'a second choice is ignored while the first save is pending');
  assert.ok(iconSignalIds.every(id => snapshot(runtime.cx)[id] === 'lucide:Folder'),
    'a pending second choice cannot replace the canonical trigger');
  assert.ok(requests[0].url.endsWith('/__native_modules/update'));
  assert.deepEqual(requests[0].arguments, input.expected_arguments,
    'the update procedure receives the selected icon');
  pane.controller.abort();
  releaseSave({ok: true, json: async () => null});
  await flush();
  assert.ok(iconSignalIds.every(id => snapshot(runtime.cx)[id] === input.selected_icon),
    'the first successful response publishes the selected canonical icon');
  fire(runtime, input.remove, 'click');
  await flush();
  assert.equal(requests.length, 2, 'Remove icon dispatches one immediate save');
  assert.deepEqual(requests[1].arguments, input.remove_arguments,
    'removing the icon sends the clear value');
  releaseSave({ok: true, json: async () => null});
  await flush();
  const failed = await failDetailIconSave();
  assert.deepEqual(failed.requests[0].arguments, input.failure_arguments,
    'the failure fixture exercises the same emitted update payload');
  await disposedOwnerDoesNotSave();
  const abandoned = await parentDisposalIgnoresLateResult();
  return {requests, signals: snapshot(runtime.cx), failed, abandoned};
}

async function failDetailIconSave() {
  const requests = [];
  const runtime = fixture(async (url, options) => {
    requests.push({url: String(url), arguments: JSON.parse(options.body)});
    throw new Error('fixture save failure');
  });
  const pane = paneContext(runtime);
  fire(runtime, input.trigger, 'click');
  fire(runtime, input.choice, 'click', {}, pane.cx);
  pane.controller.abort();
  await flush();
  assert.equal(requests.length, 1, 'a failed icon change attempts one update');
  return {requests, signals: snapshot(runtime.cx)};
}

async function disposedOwnerDoesNotSave() {
  const runtime = fixture(async () => {
    assert.fail('a disposed module owner cannot save an icon');
  });
  const before = snapshot(runtime.cx);
  runtime.controller.abort();
  fire(runtime, input.change, 'native-project-icon-change');
  await flush();
  assert.deepEqual(snapshot(runtime.cx), before,
    'a disposed module owner ignores picker actions');
}

async function parentDisposalIgnoresLateResult() {
  let releaseSave;
  const requests = [];
  const runtime = fixture((url, options) => {
    requests.push({url: String(url), arguments: JSON.parse(options.body)});
    return new Promise(resolve => { releaseSave = resolve; });
  });
  const pane = paneContext(runtime);
  fire(runtime, input.trigger, 'click');
  fire(runtime, input.choice, 'click', {}, pane.cx);
  await flush();
  assert.equal(requests.length, 1, 'the parent-owned icon save starts');
  const beforeLateResult = snapshot(runtime.cx);
  runtime.controller.abort();
  releaseSave({ok: true, json: async () => null});
  await flush();
  assert.deepEqual(snapshot(runtime.cx), beforeLateResult,
    'a parent route disposal rejects late icon completion writes');
  return {requests, signals: snapshot(runtime.cx)};
}

(async () => {
  const result = input.phase === 'create'
    ? await createWithSelectedIcon()
    : input.phase === 'mounted_detail_save'
      ? await mountedDetailSave()
    : input.phase === 'detail'
      ? await saveDetailIcon()
      : input.phase === 'failure'
        ? await failDetailIconSave()
        : await disposedOwnerDoesNotSave();
  process.stdout.write(JSON.stringify(result ?? {ok: true}));
})().catch(error => {
  process.stderr.write(`${error.stack}\n`);
  process.exitCode = 1;
});
