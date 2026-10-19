'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture(input.signals, async () => {}, input.browser_source);
const storage = new Map(Object.entries(input.initial_storage ?? {}));
const privateKey = `lific:list:state:${input.project}`;
if (!storage.has(privateKey)) storage.set(privateKey, 'private preference sentinel');
runtime.context.localStorage = {
  getItem: key => {
    if (input.storage_denied) throw new Error('storage denied');
    return storage.get(key) ?? null;
  },
  setItem: (key, value) => {
    if (input.storage_denied) throw new Error('storage denied');
    storage.set(key, String(value));
  },
  removeItem: key => storage.delete(key),
};

const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const readDisplay = () => JSON.parse(unbox(runtime.cx.signal(input.display_signal).dehydrate()));
const readState = () => input.state_signal
  ? JSON.parse(unbox(runtime.cx.signal(input.state_signal).dehydrate()))
  : undefined;
const dispatch = (name, type, value = '') => runtime.handler(input.handlers[name])(
  runtime.cx.event({type, target: {value}, currentTarget: {}}),
);

if (input.dispose_before_mount) runtime.controller.abort();
if (input.handlers.mount) dispatch('mount', 'mount');
const restoredDisplay = readDisplay();
const restoredState = readState();

if (!input.skip_actions) {
  dispatch('density', 'click');
  dispatch('hide_backlog', 'click');
  dispatch('lane_by', 'change', input.lane_by_value ?? 'module');
  dispatch('collapse_active', 'click');
  if (input.toggle_hidden_twice) dispatch('hide_backlog', 'click');
}

const display = readDisplay();
if (!input.skip_actions && !input.toggle_hidden_twice) {
  assert.equal(display.density, 'comfortable');
  assert.equal(display.laneBy, input.lane_by_value ?? 'module');
  assert.deepEqual(display.hiddenStatuses, ['backlog']);
  assert.deepEqual(display.collapsedColumns, ['active']);
  assert.equal(storage.get(`lific:public:list:state:${input.project}`).includes('comfortable'), true);
  assert.deepEqual(JSON.parse(storage.get(`lific:public:board:hidden-statuses:${input.project}`)), ['backlog']);
  assert.equal(storage.get(`lific:public:board:lanes:${input.project}`), 'module');
  assert.deepEqual(JSON.parse(storage.get(`lific:public:board:collapsed-columns:${input.project}`)), ['active']);
}
if (input.toggle_hidden_twice) assert.deepEqual(display.hiddenStatuses, []);
assert.equal(storage.get(privateKey), 'private preference sentinel');

runtime.controller.abort();
process.stdout.write(JSON.stringify({
  display,
  restored_display: restoredDisplay,
  restored_state: restoredState,
  storage: Object.fromEntries(storage),
  disposed: runtime.controller.signal.aborted,
}));
