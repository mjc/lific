'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture(input.signals, async () => {}, input.browser_source);
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
let currentTime = input.first_tick;
let nextInterval = 1;
const intervals = new Map();
const visibilityListeners = [];
runtime.context.Date = {now: () => currentTime};
runtime.context.document.visibilityState = 'visible';
runtime.context.document.addEventListener = (name, listener) => {
  if (name === 'visibilitychange') visibilityListeners.push(listener);
};
runtime.context.setInterval = (callback, delay) => {
  const id = nextInterval++;
  intervals.set(id, {callback, delay});
  return id;
};
runtime.context.clearInterval = id => intervals.delete(id);
const dispatchVisibility = () => {
  for (const listener of visibilityListeners) listener({type: 'visibilitychange'});
};
const clockValue = () => unbox(runtime.registry.read(input.clock_signal_id).dehydrate());

runtime.handler(input.mount_handler)(runtime.cx.event({type: 'mount'}));
assert.equal(clockValue(), input.first_tick, 'mount writes the current browser time to the shared signal');
assert.equal(intervals.size, 1);
assert.equal(intervals.values().next().value.delay, 30000);
const activeAfterMount = intervals.size;

runtime.context.document.visibilityState = 'hidden';
dispatchVisibility();
assert.equal(intervals.size, 0, 'hidden documents stop the shared clock');
const activeWhileHidden = intervals.size;

runtime.context.document.visibilityState = 'visible';
dispatchVisibility();
assert.equal(intervals.size, 1, 'visible documents resume the shared clock');
currentTime += 30000;
intervals.values().next().value.callback();
const clockAfterResume = clockValue();

runtime.controller.abort();
assert.equal(intervals.size, 0, 'disposing the thread clears its active interval');
const activeAfterDispose = intervals.size;
process.stdout.write(JSON.stringify({
  active_after_mount: activeAfterMount,
  active_while_hidden: activeWhileHidden,
  active_after_resume: 1,
  clock_after_resume: clockAfterResume,
  active_after_dispose: activeAfterDispose,
}));
