'use strict';

// Execute emitted Pages Move handlers in the packaged Topcoat runtime.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
let finishRequest;
const fixture = handlerFixture(input.signals, (url, options) => {
  requests.push({url, options, arguments: JSON.parse(options.body)});
  if (input.scenario === 'failure') return Promise.reject(new Error('offline'));
  if (input.scenario === 'retired' || input.scenario === 'pending') {
    return new Promise(resolve => {
      finishRequest = () => resolve({ok: true, json: async () => ({t: 'Record', v: {status: {ok: 'saved'}}})});
    });
  }
  return {ok: true, json: async () => ({t: 'Record', v: {status: {ok: 'saved'}}})};
}, input.browser_source);
const {cx, context} = fixture;
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
const snapshot = () => Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, cx.signal(id).dehydrate()]));

async function run() {
  if (input.scenario === 'open') {
    fixture.handler(input.open_handler)(cx.event({type: 'click', target: {}}));
    const signals = snapshot();
    process.stdout.write(JSON.stringify({signals}));
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
  assert.equal(dialog.value, false, 'the server-rendered picker is open after row-action replay');

  cancel(cx.event({type: 'click', target: {}}));
  assert.equal(unbox(cx.signal(dialog.id).dehydrate()), true,
    'the close action cancels the move');
  assert.equal(requests.length, 0, 'cancelling never sends a move request');
  open(cx.event({type: 'click', target: {}}));
  select(cx.event({type: 'change', target: {value: String(input.current_folder_id)}}));
  assert.equal(requests.length, 0, 'selecting the current folder does not write');
  assert.equal(unbox(cx.signal(dialog.id).dehydrate()), true,
    'the unchanged selection closes the picker');
  open(cx.event({type: 'click', target: {}}));

  if (input.scenario === 'disposed') fixture.controller.abort();
  select(cx.event({type: 'change', target: {value: String(input.folder_id)}}));
  if (input.scenario === 'disposed') {
    for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
    assert.equal(requests.length, 0, 'a retired list cannot queue a move request');
    process.stdout.write(JSON.stringify({ok: true, requests: requests.length}));
    return;
  }

  if (input.scenario === 'pending') {
    for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
    assert.equal(requests.length, 1, 'the first folder choice starts one request');
    cancel(cx.event({type: 'click', target: {}}));
    fixture.handler(input.escape_handler)(cx.event({type: 'keydown', key: 'Escape', target: {}, currentTarget: {}}));
    const backdrop = {};
    fixture.handler(input.backdrop_handler)(cx.event({type: 'click', target: backdrop, currentTarget: backdrop}));
    select(cx.event({type: 'change', target: {value: String(input.current_folder_id)}}));
    assert.equal(requests.length, 1, 'pending choices cannot submit twice');
    assert.equal(unbox(cx.signal(dialog.id).dehydrate()), false,
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
    assert.equal(unbox(cx.signal(dialog.id).dehydrate()), false,
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
    assert.equal(unbox(cx.signal(dialog.id).dehydrate()), true,
      'a successful move closes the picker');
  }
  process.stdout.write(JSON.stringify({url: path, arguments: request.arguments}));
}
run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
