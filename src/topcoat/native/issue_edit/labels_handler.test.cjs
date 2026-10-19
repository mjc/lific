'use strict';

// Execute label picker handlers from authenticated production renders using the
// packaged Topcoat runtime. Rust supplies fresh renders between shard phases.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const output = {};

function run(phase) {
  const events = [];
  const fixture = handlerFixture(phase.signals, () => {
    assert.fail('label edits dispatch to the durable issue owner');
  }, input.browser_source);
  const {cx, context} = fixture;
  const controller = fixture.controller;
  const referenced = new Set();
  const signal = cx.signal.bind(cx);
  cx.signal = id => {
    referenced.add(id);
    return signal(id);
  };
  const bindingId = source => {
    referenced.clear();
    vm.runInNewContext(`cx => (${source})`, context)(cx);
    const ids = [...referenced];
    assert.equal(ids.length, 1, 'binding resolves exactly one picker-owned signal');
    return ids[0];
  };
  context.requestAnimationFrame = callback => callback();
  context.document.getElementById = () => ({focus() {}});
  context.CustomEvent = class extends Event {
    constructor(type, options) {
      super(type, options);
      this.detail = options.detail;
    }
  };
  context.window.dispatchEvent = event => {
    events.push(event);
    event.preventDefault();
    return false;
  };
  const read = id => unbox(cx.signal(id).dehydrate());
  const signalValues = () => Object.fromEntries(Object.keys(phase.signals)
    .map(id => [id, cx.signal(id).dehydrate().v]));
  const click = () => cx.event({type: 'click', target: {}, cancelable: true,
    preventDefault() {}, stopPropagation() {}});
  const key = (key, target = {}) => cx.event({type: 'keydown', key, target, cancelable: true,
    preventDefault() {}, stopPropagation() {}});
  const textInput = value => cx.event({type: 'input', target: {value}, cancelable: true,
    preventDefault() {}});
  const enterThroughProjectedAction = (handlerSource, actionSource, rootSelector, actionSelector) => {
    const action = actionSource ? fixture.handler(actionSource) : () => {};
    const target = {
      closest: selector => selector === rootSelector
        ? {querySelector: childSelector => childSelector === actionSelector
          ? {click: () => action(click())} : null}
        : null,
    };
    fixture.handler(handlerSource)(key('Enter', target));
  };

  if (phase.name === 'toggle') {
    const openId = bindingId(phase.open_binding);
    const queryId = bindingId(phase.query_binding);
    fixture.handler(phase.open_handler)(click());
    assert.equal(read(openId), true, 'Add opens the label picker');
    assert.equal(read(queryId), '', 'opening resets the search draft');
    fixture.handler(phase.query_handler)(textInput('New runtime label'));
    assert.equal(read(queryId), 'New runtime label', 'the real input handler retains the user query');
    assert.equal(events.length, 0, 'filter editing does not dispatch a label mutation');
    assert.equal(read(openId), true, 'typing keeps the picker open');
    output.query_id = queryId;
    output.query_value = read(queryId);
    output.query_signals = signalValues();
  } else if (phase.name === 'palette') {
    fixture.handler(phase.color_trigger_handler)(click());
    fixture.handler(phase.palette_handler)(click());
    const ids = signalIds(phase.palette_handler);
    assert.equal(read(ids[0]), phase.palette_color);
    assert.equal(read(bindingId(phase.color_open_binding)), false,
      'palette choice closes only the color popover');
    assert.equal(read(bindingId(phase.open_binding)), true, 'palette choice keeps the label picker open');
    output.palette_signals = signalValues();
  } else if (phase.name === 'hex_input') {
    fixture.handler(phase.color_trigger_handler)(click());
    fixture.handler(phase.hex_input_handler)(textInput('AbC'));
    assert.equal(read(bindingId(phase.hex_binding)), 'AbC',
      'hex input is retained without replacing its focused owner');
    output.hex_signals = signalValues();
  } else if (phase.name === 'hex_set') {
    fixture.handler(phase.hex_set_handler)(click());
    const ids = signalIds(phase.palette_handler);
    assert.equal(read(ids[0]), '#aabbcc', 'Set applies Main’s normalized 3-digit hex color');
    assert.equal(read(bindingId(phase.color_open_binding)), false);
    output.hex_set_signals = signalValues();
  } else if (phase.name === 'create') {
    fixture.handler(phase.create_handler)(click());
    assert.equal(events.length, 1);
    assert.equal(events[0].type, 'lific:native-issue-label-request');
    assert.equal(events[0].defaultPrevented, true);
    assert.deepEqual(plain(events[0].detail.dehydrate()), phase.create_request,
      'the refreshed create handler emits the current query and color');
    assert.equal(read(bindingId(phase.creating_binding)), true);
    assert.equal(read(bindingId(phase.open_binding)), true,
      'creation leaves the picker open while pending');
    assert.equal(read(bindingId(phase.query_binding)), phase.query,
      'query remains until catalog creation succeeds');
    output.create_request = plain(events[0].detail.dehydrate());
  } else if (phase.name === 'filter_enter') {
    enterThroughProjectedAction(
      phase.filter_keydown_handler,
      phase.enter_action_handler,
      '[data-native-issue-label-picker]',
      '[data-native-label-enter="true"]',
    );
    assert.equal(events.length, phase.has_enter_action ? 1 : 0,
      'Enter clicks only the action selected by the current Rust projection');
    if (!phase.has_enter_action) return;
    assert.equal(events[0].type, 'lific:native-issue-label-request');
    assert.deepEqual(plain(events[0].detail.dehydrate()), phase.expected_request,
      'Enter uses the current filtered option or create request');
    assert.equal(read(bindingId(phase.query_binding)), phase.query, 'Enter preserves the filter draft');
    output.enter_request = plain(events[0].detail.dehydrate());
  } else if (phase.name === 'stale_query_enter') {
    fixture.handler(phase.query_handler)(textInput(phase.next_query));
    assert.equal(read(bindingId(phase.query_binding)), phase.next_query);
    enterThroughProjectedAction(
      phase.filter_keydown_handler,
      phase.old_enter_action_handler,
      '[data-native-issue-label-picker]',
      '[data-native-label-enter="true"]',
    );
    assert.equal(events.length, 0,
      'an old Enter action cannot submit after the actual filter input changes its query');
  } else if (phase.name === 'stale_color_create') {
    fixture.handler(phase.palette_handler)(click());
    assert.equal(read(signalIds(phase.palette_handler)[0]), phase.palette_color);
    fixture.handler(phase.create_handler)(click());
    assert.equal(events.length, 1, 'Create remains available after a palette choice');
    assert.equal(plain(events[0].detail.dehydrate()).v.color, phase.palette_color,
      'Create reads the selected palette color before the projection refreshes');
  } else if (phase.name === 'emit_option') {
    fixture.handler(phase.option_handler)(click());
    assert.equal(events.length, 1);
    output.request = plain(events[0].detail.dehydrate());
  } else if (phase.name === 'hex_enter') {
    enterThroughProjectedAction(
      phase.hex_keydown_handler,
      phase.hex_set_handler,
      '[data-native-label-color-area]',
      '[data-native-label-hex-set]',
    );
    assert.equal(read(phase.color_id), phase.normalized_color,
      'Enter applies the current validated custom color');
    assert.equal(read(phase.color_open_id), false);
  } else if (phase.name === 'focus') {
    const openId = bindingId(phase.open_binding);
    const frames = [];
    let focusCount = 0;
    context.requestAnimationFrame = callback => frames.push(callback);
    context.document.getElementById = id => id === phase.input_id
      ? {focus() { focusCount++; }} : null;
    fixture.handler(phase.open_handler)(click());
    assert.equal(frames.length, 1, 'opening schedules focus after the picker projection mounts');
    frames.shift()();
    assert.equal(focusCount, 1, 'the live owner focuses the mounted filter input');

    const disposedFrames = [];
    context.requestAnimationFrame = callback => disposedFrames.push(callback);
    cx.signal(openId).set(cx.hydrate(false));
    fixture.handler(phase.open_handler)(click());
    assert.equal(disposedFrames.length, 1);
    controller.abort();
    disposedFrames.shift()();
    assert.equal(focusCount, 1, 'a disposed picker cannot focus a stale/replaced input');
  } else if (phase.name === 'disposed_callbacks') {
    const signalIds = Object.keys(phase.signals);
    const before = Object.fromEntries(signalIds.map(id => [id, read(id)]));
    const callbacks = phase.callbacks.map(({source, event_type, key: keyName}) => ({
      handler: fixture.handler(source),
      event: event_type === 'keydown' ? key(keyName || 'Enter')
        : event_type === 'input' ? textInput(phase.stale_value || 'stale') : click(),
    }));
    controller.abort();
    for (const callback of callbacks) callback.handler(callback.event);
    assert.equal(events.length, 0, 'disposed picker callbacks cannot dispatch durable mutations');
    assert.deepEqual(Object.fromEntries(signalIds.map(id => [id, read(id)])), before,
      'disposed picker callbacks cannot mutate local picker state');
  } else {
    assert.fail(`unknown label handler phase ${phase.name}`);
  }
}

function signalIds(source) {
  const ids = [...source.matchAll(/"t":"Signal","id":"([^"]+)"/g)].map(match => match[1]);
  assert.ok(ids.length >= 2, 'emitted picker action owns the expected Rust signals');
  return ids;
}

for (const phase of input.phases) run(phase);
process.stdout.write(JSON.stringify(output));

function plain(value) {
  return JSON.parse(JSON.stringify(value));
}

function unbox(value) {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) {
    value = value.v;
  }
  return value;
}
