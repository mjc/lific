'use strict';

// Execute emitted Pages Move handlers in the packaged Topcoat runtime.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
let finishRequest;
const fixture = handlerFixture(input.signals, async (url, options) => {
  requests.push({url, options, arguments: JSON.parse(options.body)});
  if (input.scenario === 'failure') return Promise.reject(new Error('offline'));
  if (input.scenario === 'retired' || input.scenario === 'pending') {
    return new Promise(resolve => {
      finishRequest = () => resolve({ok: true, json: async () => input.reply});
    });
  }
  return {ok: true, json: async () => input.reply};
}, input.browser_source);
const {cx, context} = fixture;
context.document.documentElement.getAttribute = () => input.mount || '';
const referenced = new Set();
const signal = cx.signal.bind(cx);
cx.signal = id => {
  referenced.add(id);
  return signal(id);
};
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const makeEvent = fields => ({
  type: 'click',
  target: {},
  currentTarget: {},
  stopPropagation() {},
  preventDefault() {},
  ...fields,
});
const snapshot = () => Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, cx.signal(id).get().dehydrate()]));

async function run() {
  if (input.scenario === 'filter') {
    fixture.handler(input.filter_handler)(cx.event(makeEvent({
      type: input.filter_type,
      target: input.filter_target,
    })));
    process.stdout.write(JSON.stringify({signals: snapshot()}));
    return;
  }
  if (input.scenario === 'open') {
    let clickStopped = false;
    let enterStopped = false;
    let enterPrevented = false;
    fixture.handler(input.open_handler)(cx.event(makeEvent({
      type: 'click', target: {}, stopPropagation() { clickStopped = true; },
    })));
    fixture.handler(input.key_handler)(cx.event(makeEvent({
      type: 'keydown', key: 'Enter', target: {},
      stopPropagation() { enterStopped = true; },
      preventDefault() { enterPrevented = true; },
    })));
    const signals = snapshot();
    process.stdout.write(JSON.stringify({
      signals,
      click_stopped: clickStopped,
      enter_stopped: enterStopped,
      enter_prevented: enterPrevented,
    }));
    return;
  }

  const readBinding = source => {
    const before = new Set(referenced);
    const value = vm.runInNewContext(`cx => (${source})`, context)(cx);
    const ids = [...referenced].filter(id => !before.has(id));
    return {value: unbox(value), id: ids[0]};
  };
  const open = fixture.handler(input.open_handler);
  const cancel = fixture.handler(input.cancel_handler);
  const select = fixture.handler(input.select_handler);
  const dialog = readBinding(input.open_binding);
  assert.equal(dialog.value, false, 'the shared backdrop is visible after row-action replay');
  const hiddenBinding = vm.runInNewContext(`cx => (${input.open_binding})`, context);
  const readHidden = () => unbox(hiddenBinding(cx));

  cancel(cx.event(makeEvent({type: 'click'})));
  assert.equal(readHidden(), true,
    'the close action cancels the move');
  assert.equal(requests.length, 0, 'cancelling never sends a move request');
  open(cx.event(makeEvent({type: 'click'})));
  select(cx.event(makeEvent({type: 'change', target: {value: String(input.current_folder_id)}})));
  assert.equal(requests.length, 0, 'selecting the current folder does not write');
  assert.equal(readHidden(), true,
    'the unchanged selection closes the picker');
  open(cx.event(makeEvent({type: 'click'})));

  const expandedBinding = input.expanded_binding
    ? vm.runInNewContext(`cx => (${input.expanded_binding})`, context)
    : null;
  const readExpanded = () => expandedBinding
    ? unbox(expandedBinding(cx).dehydrate()) === 'true'
    : null;
  if (input.scenario === 'success' && input.tree_toggle_handler) {
    const folderTarget = {
      closest(selector) {
        return selector === '[data-native-page-folder-toggle]' ? this : null;
      },
      getAttribute(name) {
        if (name === 'data-folder-id') return String(input.tree_folder_id);
        if (name === 'data-folder-revision') return String(input.tree_revision);
        return null;
      },
    };
    if (readExpanded()) {
      fixture.handler(input.tree_toggle_handler)(cx.event(makeEvent({
        type: 'click',
        target: folderTarget,
        currentTarget: {},
      })));
    }
    assert.equal(readExpanded(), false, 'the test collapses the move destination first');
  }

  if (input.scenario === 'overlay') {
    const backdrop = {};
    fixture.handler(input.backdrop_handler)(cx.event(makeEvent({
      type: 'click', target: backdrop, currentTarget: backdrop,
    })));
    assert.equal(readHidden(), true, 'clicking the backdrop closes the picker');
    open(cx.event(makeEvent({type: 'click'})));
    const backdropOwner = {};
    fixture.handler(input.backdrop_handler)(cx.event(makeEvent({
      type: 'click', target: {}, currentTarget: backdropOwner,
    })));
    assert.equal(readHidden(), false, 'clicking inside the dialog leaves it open');
    process.stdout.write(JSON.stringify({ok: true}));
    return;
  }

  const beforeSelection = snapshot();
  if (input.scenario === 'disposed') fixture.controller.abort();
  select(cx.event(makeEvent({type: 'change', target: {value: String(input.folder_id)}})));
  if (input.scenario === 'disposed') {
    for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
    assert.equal(requests.length, 0, 'a retired list cannot queue a move request');
    assert.deepEqual(snapshot(), beforeSelection, 'a retired list cannot mutate picker state');
    process.stdout.write(JSON.stringify({ok: true, requests: requests.length}));
    return;
  }

  if (input.scenario === 'pending') {
    for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
    assert.equal(requests.length, 1, 'the first folder choice starts one request');
    cancel(cx.event(makeEvent({type: 'click'})));
    fixture.handler(input.escape_handler)(cx.event(makeEvent({type: 'keydown', key: 'Escape'})));
    const backdrop = {};
    fixture.handler(input.backdrop_handler)(cx.event(makeEvent({type: 'click', target: backdrop, currentTarget: backdrop})));
    select(cx.event(makeEvent({type: 'change', target: {value: String(input.current_folder_id)}})));
    assert.equal(requests.length, 1, 'pending choices cannot submit twice');
    assert.equal(readHidden(), false,
      'close, Escape, and backdrop cannot dismiss a pending move');
    process.stdout.write(JSON.stringify({ok: true, requests: requests.length}));
    return;
  }

  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
  assert.equal(requests.length, 1, 'choosing a folder makes one typed procedure request');
  const request = requests[0];
  const path = new URL(request.url, 'http://localhost').pathname;
  assert.equal(path, `${input.mount}/__native_pages/move`);
  assert.equal(request.options.method, 'POST');
  if (input.scenario === 'failure') {
    assert.equal(readHidden(), false,
      'a failed request leaves the picker open for retry');
    process.stdout.write(JSON.stringify({signals: snapshot()}));
    return;
  }
  if (input.scenario === 'retired') {
    assert.equal(typeof finishRequest, 'function', 'the move reply is pending');
    const before = snapshot();
    fixture.controller.abort();
    finishRequest();
    for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
    assert.deepEqual(snapshot(), before, 'a retired list owner ignores a late move reply');
  } else {
    assert.equal(readHidden(), true,
      'a successful move closes the picker');
    if (input.scenario === 'success' && input.tree_toggle_handler) {
      assert.equal(readExpanded(), true,
        'a successful move expands its destination folder');
    }
  }
  process.stdout.write(JSON.stringify({url: path, arguments: request.arguments, signals: snapshot()}));
}
run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
