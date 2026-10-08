'use strict';

// Replay real Topcoat handlers emitted by authenticated PageDetail HTML.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const output = {};

async function run(phase) {
  const requests = [];
  const listeners = new Map();
  const pendingWrites = [];
  const fixture = handlerFixture(phase.signals, (url, options) => {
    if (phase.mode === 'remounted_late_reply') {
      pendingWrites.push({url, arguments: JSON.parse(options.body)});
      return new Promise(() => {});
    }
    assert.fail('page-label requests are delivered through the durable account owner');
  }, input.browser_source);
  const {cx, context, controller} = fixture;
  context.CustomEvent = class extends Event {
    constructor(type, options) {
      super(type, {cancelable: true});
      this.detail = options.detail;
    }
  };
  context.window.addEventListener = (type, callback, options = {}) => {
    const entries = listeners.get(type) || [];
    entries.push(callback);
    listeners.set(type, entries);
    options.signal?.addEventListener('abort', () => {
      listeners.set(type, (listeners.get(type) || []).filter(item => item !== callback));
    }, {once: true});
  };
  context.window.dispatchEvent = event => {
    if (event.type === 'lific:native-page-label-request') {
      requests.push(event);
      event.preventDefault(); // mimic the durable account owner accepting it
    }
    for (const callback of listeners.get(event.type) || []) callback(event);
    return !event.defaultPrevented;
  };

  const handler = source => fixture.handler(source);
  const click = () => cx.event({type: 'click', target: {}, cancelable: true,
    preventDefault() {}, stopPropagation() {}});
  const textInput = value => cx.event({type: 'input', target: {value}, cancelable: true,
    preventDefault() {}});
  const evaluate = source => vm.runInNewContext(`cx => (${source})`, context)(cx);
  const plain = value => {
    const wire = value && typeof value.dehydrate === 'function' ? value.dehydrate() : value;
    return JSON.parse(JSON.stringify(wire));
  };
  const unbox = value => {
    while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
    return value;
  };
  const readBinding = source => {
    const value = evaluate(source);
    return unbox(value && typeof value.dehydrate === 'function' ? value.dehydrate() : value);
  };
  const signalValues = () => Object.fromEntries(Object.keys(phase.signals)
    .map(id => [id, cx.signal(id).get().dehydrate()]));
  const dispatchReply = reply => context.window.dispatchEvent(
    new context.CustomEvent('lific:native-page-label-applied', {detail: cx.hydrate(reply)}));

  handler(phase.mount_handler)(click());
  if (phase.mode === 'remounted_late_reply') {
    const startPin = phase.start_pin ?? true;
    handler(phase.title_input_handler)(textInput('Remounted dirty title'));
    handler(phase.body_input_handler)(textInput('Remounted dirty body'));
    if (startPin) {
      handler(phase.pin_handler)(click());
      for (let i = 0; i < 20; i++) await Promise.resolve();
      assert.equal(pendingWrites.length, 1, 'the actual remounted pin handler starts one pending write');
      assert.ok(pendingWrites[0].url.endsWith('/__native_pages/pin'));
      assert.deepEqual(pendingWrites[0].arguments, phase.expected_pin_arguments,
        'the pin request uses the newer canonical sequence');
    } else {
      assert.equal(pendingWrites.length, 0, 'an idle remount has no pending metadata request');
    }
    for (const binding of phase.busy_bindings) assert.equal(readBinding(binding), startPin);
    const before = signalValues();
    dispatchReply(phase.reply);
    for (const binding of phase.busy_bindings) {
      assert.equal(readBinding(binding), startPin,
        'a delayed label completion preserves ownership of the current write state');
    }
    assert.deepEqual(signalValues(), before,
      'a delayed label completion preserves all newer canonical fields, drafts, sequence and local operation state');
    output.passed = true;
  } else if (phase.mode === 'request') {
    if (phase.add_disabled_binding != null) {
      assert.equal(readBinding(phase.add_disabled_binding), false, 'Add is initially enabled');
    }
    if (phase.open_handler) {
      handler(phase.open_handler)(click());
      assert.equal(readBinding(phase.hidden_binding), false, 'Add opens the page label picker');
      handler(phase.open_handler)(click());
      assert.equal(readBinding(phase.hidden_binding), true, 'Add also closes an open picker');
      handler(phase.open_handler)(click());
      context.window.dispatchEvent(new Event('click'));
      assert.equal(readBinding(phase.hidden_binding), true, 'an outside click closes the picker');
      handler(phase.open_handler)(click());
    } else {
      assert.equal(readBinding(phase.hidden_binding), false,
        'the PageDetail owner keeps the picker open for a remove action');
    }
    assert.equal(requests.length, 0, 'opening does not write labels');

    handler(phase.choice_handler)(click());
    assert.equal(requests.length, 1, 'one selection emits one request');
    assert.equal(requests[0].type, phase.event_type);
    assert.deepEqual(plain(requests[0].detail), phase.expected_request);
    assert.equal(requests[0].defaultPrevented, true,
      'the durable owner accepts the label request');
    assert.equal(readBinding(phase.hidden_binding), false, 'choosing a label leaves the picker open');
    if (phase.add_disabled_binding != null) {
      assert.equal(readBinding(phase.add_disabled_binding), false, 'Add stays available during a write');
    }
    for (const source of phase.busy_bindings || []) {
      assert.equal(readBinding(source), true,
        'pending label writes disable other page write controls');
    }
    for (const source of phase.draft_bindings || []) {
      if (source !== null) {
        assert.equal(readBinding(source), false,
          'draft fields remain editable while a label write is pending');
      }
    }

    if (phase.dirty_title != null) {
      handler(phase.title_input_handler)(textInput(phase.dirty_title));
      handler(phase.body_input_handler)(textInput(phase.dirty_body));
      assert.equal(readBinding(phase.title_draft_binding), phase.dirty_title,
        'title editing remains available while a label write is pending');
      assert.equal(readBinding(phase.body_draft_binding), phase.dirty_body,
        'body editing remains available while a label write is pending');
    }
    output.request = plain(requests[0].detail);
    output.signals = signalValues();
  } else if (phase.mode === 'applied') {
    const before = signalValues();
    dispatchReply(phase.wrong_account_reply);
    assert.deepEqual(signalValues(), before,
      'the PageDetail listener ignores another account’s completion');
    dispatchReply(phase.wrong_page_reply);
    assert.deepEqual(signalValues(), before,
      'the PageDetail listener ignores another page’s completion');

    dispatchReply(phase.reply);
    handler(phase.stale_choice_handler)(click());
    assert.equal(requests.length, 0,
      'an option rendered before the refreshed catalog cannot send an obsolete choice');
    output.signals = signalValues();
    output.bound = Object.fromEntries(Object.entries(phase.bindings || {})
      .map(([name, source]) => [name, plain(evaluate(source))]));
    assert.equal(readBinding(phase.hidden_binding), false, 'applying a label keeps the picker open');

    const applied = signalValues();
    controller.abort();
    dispatchReply(phase.reply);
    assert.deepEqual(signalValues(), applied,
      'a disposed PageDetail ignores late label completions');
  } else {
    assert.fail(`unknown PageDetail label phase ${phase.mode}`);
  }
}

async function main() {
  for (const phase of input.phases) await run(phase);
  process.stdout.write(JSON.stringify(output));
}
main().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
