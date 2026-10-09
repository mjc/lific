'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const flush = async () => {
  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
};

function snapshot(cx, signals) {
  return Object.fromEntries(Object.keys(signals).map(id => [id, cx.signal(id).dehydrate().v]));
}

async function run() {
  if (input.mode === 'mount_sync') {
    const fixture = handlerFixture(input.signals, async () => {
      throw new Error('canonical title reconciliation must not perform network I/O');
    }, input.browser_source);
    fixture.handler(input.mount_handler)(fixture.cx.event({
      type: 'mount', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {},
    }));
    await flush();
    return {signals: snapshot(fixture.cx, input.signals)};
  }

  if (input.mode === 'parent_overlap') {
    const pending = [];
    const fixture = handlerFixture(input.signals, (url, options) => {
      return new Promise(resolve => pending.push({
        path: new URL(url, 'http://localhost').pathname,
        arguments: JSON.parse(options.body),
        signal: options.signal,
        resolve,
      }));
    }, input.browser_source);
    const retiredChildController = new AbortController();
    const retiredChild = new fixture.context.fixture.Context(fixture.registry);
    retiredChild.abortSignal = retiredChildController.signal;
    const handlers = Object.fromEntries(Object.entries(input.handlers).map(([name, source]) => [
      name, fixture.handler(source),
    ]));
    const marker = selector => ({
      value: '',
      closest(candidate) { return candidate === selector ? this : null; },
      getAttribute() { return 'title'; },
    });
    const event = (type, target, key) => ({
      type, key, target, currentTarget: {},
      preventDefault() { this.prevented = true; }, stopPropagation() {},
    });
    const trigger = marker('[data-native-plan-title-trigger]');
    const editor = marker('[data-native-plan-title-input]');
    const click = () => handlers.click(fixture.cx.event(event('click', trigger)));
    const inputDraft = value => {
      editor.value = value;
      handlers.input(fixture.cx.event(event('input', editor)));
    };
    const commit = () => handlers.keydown(fixture.cx.event(event('keydown', editor, 'Enter')));

    click();
    inputDraft('First overlapping title');
    commit();
    await flush();
    click();
    inputDraft('Second overlapping title');
    commit();
    await flush();
    assert.equal(pending.length, 2, 'both commits reach the parent-owned save handler');
    pending[0].resolve({ok: true, json: async () => input.responses[0]});
    await flush();
    retiredChildController.abort();
    assert.equal(retiredChild.abortSignal.aborted, true, 'the replaced saved-shard child is retired');
    assert.equal(pending[1].signal.aborted, false,
      'the second pending save is owned by the persistent parent context');
    pending[1].resolve({ok: true, json: async () => input.responses[1]});
    await flush();
    return {
      requests: pending.map(({path, arguments: args}) => ({path, arguments: args})),
      signals: snapshot(fixture.cx, input.signals),
    };
  }

  const clickFixture = handlerFixture(input.signals, async () => {
    throw new Error('starting title edit must not perform network I/O');
  }, input.browser_source);
  clickFixture.handler(input.click_handler)(clickFixture.cx.event({
    type: 'click', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {},
  }));
  await flush();
  const afterClick = snapshot(clickFixture.cx, input.signals);

  const keyEvent = key => ({
    type: 'keydown', key, target: {}, currentTarget: {},
    preventDefault() { this.prevented = true; }, stopPropagation() {},
  });
  async function activate(key) {
    const fixture = handlerFixture(input.signals, async () => {
      throw new Error('starting title edit must not perform network I/O');
    }, input.browser_source);
    const event = keyEvent(key);
    fixture.handler(input.keydown_handler)(fixture.cx.event(event));
    await flush();
    return {signals: snapshot(fixture.cx, input.signals), prevented: event.prevented};
  }
  const enter = await activate('Enter');
  const space = await activate(' ');
  const afterEnter = enter.signals;
  const afterSpace = space.signals;
  assert.equal(enter.prevented, true, 'Enter prevents its default action');
  assert.equal(space.prevented, true, 'Space prevents its default action');

  if (!input.edit_signals) {
    return {
      click_signals: afterClick,
      keyboard_signals: afterEnter,
      space_signals: afterSpace,
    };
  }

  if (input.mode === 'close') {
    const fixture = handlerFixture(input.edit_signals, async () => {
      throw new Error('Escape must not save the draft');
    }, input.browser_source);
    fixture.handler(input.edit_keydown_handler)(fixture.cx.event({
      type: 'keydown', key: 'Escape', target: {}, currentTarget: {},
      preventDefault() {}, stopPropagation() {},
    }));
    return {close_signals: snapshot(fixture.cx, input.edit_signals)};
  }

  const requests = [];
  const editFixture = handlerFixture(input.edit_signals, async (url, options) => {
    requests.push({path: new URL(url, 'http://localhost').pathname, arguments: JSON.parse(options.body)});
    return {ok: true, json: async () => input.response};
  }, input.browser_source);
  const field = editFixture.handler(input.input_handler);
  const key = editFixture.handler(input.edit_keydown_handler);
  const blur = editFixture.handler(input.blur_handler);
  field(editFixture.cx.event({
    type: 'input', target: {value: '  Renamed plan title\uFEFF'}, currentTarget: {},
  }));
  const commitEnter = {
    type: 'keydown', key: 'Enter', target: {}, currentTarget: {},
    preventDefault() { this.prevented = true; }, stopPropagation() {},
  };
  key(editFixture.cx.event(commitEnter));
  blur(editFixture.cx.event({type: 'blur', target: {}, currentTarget: {}}));
  await flush();
  assert.equal(commitEnter.prevented, true, 'Enter commits and prevents form submission');
  assert.equal(requests.length, 1, 'Enter followed by blur performs one save');
  assert.equal(requests[0].path, '/__native_plans/mutate');

  async function noSave(value, keyName) {
    let requests = 0;
    const fixture = handlerFixture(input.edit_signals, async () => {
      requests += 1;
      throw new Error(`${keyName} must not save this draft`);
    }, input.browser_source);
    fixture.handler(input.input_handler)(fixture.cx.event({
      type: 'input', target: {value}, currentTarget: {},
    }));
    const keyHandler = fixture.handler(input.edit_keydown_handler);
    const keyEvent = {
      type: 'keydown', key: keyName, target: {}, currentTarget: {},
      preventDefault() { this.prevented = true; }, stopPropagation() {},
    };
    keyHandler(fixture.cx.event(keyEvent));
    fixture.handler(input.blur_handler)(fixture.cx.event({
      type: 'blur', target: {}, currentTarget: {},
    }));
    await flush();
    assert.equal(requests, 0, `${keyName} does not save this draft`);
    return {signals: snapshot(fixture.cx, input.edit_signals), prevented: keyEvent.prevented};
  }
  const cancelled = await noSave('Discard this draft', 'Escape');
  const empty = await noSave('   ', 'Enter');
  const unchanged = await noSave(input.title, 'Enter');

  async function saveWith(mode) {
    const saveRequests = [];
    const fixture = handlerFixture(input.edit_signals, async (url, options) => {
      saveRequests.push({path: new URL(url, 'http://localhost').pathname,
        arguments: JSON.parse(options.body)});
      return {ok: true, json: async () => input.response};
    }, input.browser_source);
    fixture.handler(input.input_handler)(fixture.cx.event({
      type: 'input', target: {value: `Saved by ${mode}`}, currentTarget: {},
    }));
    if (mode !== 'blur') {
      const keyEvent = {
        type: 'keydown', key: 's', ctrl_key: mode === 'ctrl+s', meta_key: mode === 'meta+s',
        ctrlKey: mode === 'ctrl+s', metaKey: mode === 'meta+s', target: {}, currentTarget: {},
        preventDefault() { this.prevented = true; }, stopPropagation() {},
      };
      fixture.handler(input.edit_keydown_handler)(fixture.cx.event(keyEvent));
      assert.equal(keyEvent.prevented, true, `${mode} prevents browser save`);
    }
    fixture.handler(input.blur_handler)(fixture.cx.event({
      type: 'blur', target: {}, currentTarget: {},
    }));
    await flush();
    assert.equal(saveRequests.length, 1, `${mode} performs one title save`);
    return saveRequests[0];
  }
  const ctrlSave = await saveWith('ctrl+s');
  const metaSave = await saveWith('meta+s');
  const blurSave = await saveWith('blur');

  const failedFixture = handlerFixture(input.edit_signals, async () => {
    throw new Error('offline');
  }, input.browser_source);
  failedFixture.handler(input.input_handler)(failedFixture.cx.event({
    type: 'input', target: {value: 'Rejected title'}, currentTarget: {},
  }));
  failedFixture.handler(input.edit_keydown_handler)(failedFixture.cx.event({
    type: 'keydown', key: 'Enter', target: {}, currentTarget: {},
    preventDefault() {}, stopPropagation() {},
  }));
  await flush();

  let resolvePending;
  let pendingRequest;
  const pendingFixture = handlerFixture(snapshot(editFixture.cx, input.edit_signals), (url, options) => {
    pendingRequest = {path: new URL(url, 'http://localhost').pathname, arguments: JSON.parse(options.body)};
    return new Promise(resolve => {
    resolvePending = resolve;
    });
  }, input.browser_source);
  pendingFixture.handler(input.click_handler)(pendingFixture.cx.event({
    type: 'click', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {},
  }));
  pendingFixture.handler(input.input_handler)(pendingFixture.cx.event({
    type: 'input', target: {value: 'Saving title'}, currentTarget: {},
  }));
  pendingFixture.handler(input.edit_keydown_handler)(pendingFixture.cx.event({
    type: 'keydown', key: 'Enter', target: {}, currentTarget: {},
    preventDefault() {}, stopPropagation() {},
  }));
  await flush();
  pendingFixture.handler(input.click_handler)(pendingFixture.cx.event({
    type: 'click', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {},
  }));
  pendingFixture.handler(input.input_handler)(pendingFixture.cx.event({
    type: 'input', target: {value: 'Newer unsaved draft'}, currentTarget: {},
  }));
  resolvePending({ok: true, json: async () => input.pending_response});
  await flush();
  const pendingSignals = snapshot(pendingFixture.cx, input.edit_signals);

  let resolveRetired;
  const retiredFixture = handlerFixture(input.edit_signals, () => new Promise(resolve => {
    resolveRetired = resolve;
  }), input.browser_source);
  retiredFixture.handler(input.input_handler)(retiredFixture.cx.event({
    type: 'input', target: {value: 'Retired title'}, currentTarget: {},
  }));
  retiredFixture.handler(input.edit_keydown_handler)(retiredFixture.cx.event({
    type: 'keydown', key: 'Enter', target: {}, currentTarget: {},
    preventDefault() {}, stopPropagation() {},
  }));
  await flush();
  const beforeRetirement = snapshot(retiredFixture.cx, input.edit_signals);
  retiredFixture.controller.abort();
  resolveRetired({ok: true, json: async () => input.response});
  await flush();
  assert.deepEqual(snapshot(retiredFixture.cx, input.edit_signals), beforeRetirement,
    'a retired title editor ignores a late successful save');
  return {
    click_signals: afterClick,
    keyboard_signals: afterEnter,
    space_signals: afterSpace,
    commit_signals: snapshot(editFixture.cx, input.edit_signals),
    request: requests[0],
    cancelled_signals: cancelled.signals,
    cancel_prevented: cancelled.prevented,
    empty_signals: empty.signals,
    unchanged_signals: unchanged.signals,
    ctrl_save_request: ctrlSave,
    meta_save_request: metaSave,
    blur_save_request: blurSave,
    failed_signals: snapshot(failedFixture.cx, input.edit_signals),
    pending_signals: pendingSignals,
    pending_request: pendingRequest,
    click_editing: true,
    keyboard_editing: true,
  };
}

run().then(result => process.stdout.write(JSON.stringify(result))).catch(error => {
  process.stderr.write(`${error.stack}\n`);
  process.exitCode = 1;
});
