'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const events = [];
const fixture = handlerFixture(input.signals, () => {
  assert.fail('Module picker dispatches to the durable owner; it does not call a procedure itself.');
}, input.browser_source);
fixture.context.CustomEvent = class extends Event {
  constructor(type, options) {
    super(type, options);
    this.detail = options.detail;
  }
};
fixture.context.window.dispatchEvent = event => {
  events.push(event);
  event.preventDefault();
  return false;
};

const referenced = new Set();
const signal = fixture.cx.signal.bind(fixture.cx);
fixture.cx.signal = id => {
  referenced.add(id);
  return signal(id);
};
const click = cx => cx.event(new Event('click', {cancelable: true}));
const beforeOpen = values(fixture);
fixture.handler(input.trigger_handler)(click(fixture.cx));
const openIds = Object.keys(input.signals).filter(id =>
  beforeOpen[id] === false && unbox(fixture.cx.signal(id).dehydrate()) === true);
assert.equal(openIds.length, 1, 'trigger opens only the picker-owned signal');
const [openId] = openIds;
assert.equal(unbox(fixture.cx.signal(openId).dehydrate()), true, 'trigger opens the module choices');

fixture.handler(input.same_handler)(click(fixture.cx));
assert.equal(events.length, 0, 'choosing the current module closes the picker without a write request');
assert.equal(unbox(fixture.cx.signal(openId).dehydrate()), false, 'same-module selection closes');

fixture.handler(input.trigger_handler)(click(fixture.cx));
fixture.handler(input.option_handler)(click(fixture.cx));
assert.equal(events.length, 1, 'one changed selection emits one owner request');
assert.equal(events[0].type, 'lific:native-issue-module-request');
assert.equal(events[0].cancelable, true);
assert.equal(events[0].defaultPrevented, true, 'accepted owner request closes the picker');
const request = JSON.parse(JSON.stringify(events[0].detail.dehydrate()));
assert.deepEqual(request, input.request,
  'selection emits the typed Rust request surrogate');
assert.equal(unbox(fixture.cx.signal(openId).dehydrate()), false);

const retired = handlerFixture(input.signals, () => {
  assert.fail('a disposed picker must not dispatch an assignment request');
}, input.browser_source);
retired.context.CustomEvent = fixture.context.CustomEvent;
retired.context.window.dispatchEvent = () => {
  assert.fail('a disposed picker must not dispatch an assignment request');
};
const retiredBefore = snapshot(retired);
retired.controller.abort();
retired.handler(input.option_handler)(click(retired.cx));
assert.deepEqual(snapshot(retired), retiredBefore,
  'disposed handler leaves rendered state unchanged');
process.stdout.write(JSON.stringify({request}));

function unbox(value) {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
}

function snapshot(runtime) {
  return Object.fromEntries(Object.keys(input.signals).map(id => [id, runtime.cx.signal(id).dehydrate()]));
}

function values(runtime) {
  return Object.fromEntries(Object.keys(input.signals).map(id => [id, unbox(runtime.cx.signal(id).dehydrate())]));
}
