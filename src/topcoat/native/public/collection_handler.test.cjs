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
const read = name => unbox(runtime.cx.signal(input.signal_ids[name]).dehydrate());
const dispatch = (name, type, value = '') => runtime.handler(input.handlers[name])(
  runtime.cx.event({type, target: {value}, currentTarget: {}}),
);

dispatch('query', 'input', 'database');
dispatch('status', 'change', 'active');
dispatch('priority', 'change', 'urgent');
dispatch('label', 'change', 'Roadmap');
dispatch('module', 'change', 'Core');
dispatch('sort', 'change', 'updated');
dispatch('direction', 'change', 'desc');
dispatch('group', 'change', 'module');
const selected = Object.fromEntries(
  ['query', 'status', 'priority', 'label', 'module', 'sort', 'direction', 'group']
    .map(name => [name, read(name)]),
);
assert.deepEqual(selected, {
  query: 'database', status: 'active', priority: 'urgent', label: 'Roadmap',
  module: 'Core', sort: 'updated', direction: 'desc', group: 'module',
});

dispatch('clear', 'click');
const afterClear = Object.fromEntries(
  ['query', 'status', 'priority', 'label', 'module', 'sort', 'direction', 'group']
    .map(name => [name, read(name)]),
);
assert.deepEqual(afterClear, {
  query: '', status: '', priority: '', label: '', module: '', sort: 'priority',
  direction: 'asc', group: 'status',
});

// Disposing the owner installs no global listener and leaves the rendered
// signal state untouched.
const stateBeforeDispose = JSON.stringify(Object.fromEntries(
  Object.entries(input.signal_ids).map(([name, id]) => [name, runtime.cx.signal(id).dehydrate()]),
));
runtime.controller.abort();
assert.equal(runtime.context.window.addEventListener, undefined);
assert.equal(JSON.stringify(Object.fromEntries(
  Object.entries(input.signal_ids).map(([name, id]) => [name, runtime.cx.signal(id).dehydrate()]),
)), stateBeforeDispose);
process.stdout.write(JSON.stringify({
  ...selected,
  after_clear: afterClear,
  cleared: true,
  disposed: runtime.controller.signal.aborted,
}));
