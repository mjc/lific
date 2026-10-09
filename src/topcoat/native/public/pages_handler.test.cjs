'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture(input.signals, async () => {}, input.browser_source);
const decode = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return Array.isArray(value) ? value.map(decode) : value;
};
const read = name => decode(runtime.cx.signal(input.signal_ids[name]).dehydrate());
const dispatch = (name, type, value = '') => runtime.handler(input.handlers[name])(
  runtime.cx.event({type, target: {value}, currentTarget: {}}),
);

const expandedId = Object.keys(input.signals).find(id => {
  const value = decode(runtime.cx.signal(id).dehydrate());
  return Array.isArray(value) && value.some(item => Number(item) === input.folder_id);
});
assert.ok(expandedId, 'the expanded-folder signal is declared in the rendered page');

const before = {
  tab: read('tab'),
  query: read('query'),
  label: read('label'),
  status: read('status'),
  focus: Number(read('focus')),
  expanded: decode(runtime.cx.signal(expandedId).dehydrate()),
};

dispatch('query', 'input', 'needle');
dispatch('label', 'change', 'guide');
dispatch('status', 'change', 'archived');
dispatch('focus', 'change', String(input.folder_id));
dispatch('tab_archived', 'click');
dispatch('folder', 'click');
const changed = {
  tab: read('tab'),
  query: read('query'),
  label: read('label'),
  status: read('status'),
  focus: Number(read('focus')),
  expanded: decode(runtime.cx.signal(expandedId).dehydrate()),
};
assert.deepEqual(changed, {
  tab: 'archived',
  query: 'needle',
  label: 'guide',
  status: 'archived',
  focus: input.folder_id,
  expanded: [],
});

dispatch('clear', 'click');
const cleared = {
  query: read('query'),
  label: read('label'),
  status: read('status'),
  focus: Number(read('focus')),
};
assert.deepEqual(cleared, {query: '', label: '', status: '__active', focus: 0});

dispatch('tab_browse', 'click');
const tab = read('tab');
assert.equal(tab, 'browse');
runtime.controller.abort();
process.stdout.write(JSON.stringify({before, changed, cleared, tab, disposed: runtime.controller.signal.aborted}));
