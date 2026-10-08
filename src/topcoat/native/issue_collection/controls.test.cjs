'use strict';

const assert = require('node:assert/strict');
const {handlerFixture} = require('../handler_fixture.cjs');
const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
assert.ok(input.handlers['tally:active:click'], 'Main status tallies are native filter shortcuts');
assert.ok(input.handlers['search-open:click'], 'Main search opens from a compact button');
assert.ok(input.handlers['search:blur'], 'empty search collapses on blur');
const key = 'lific:list:state:ACC';
const saved = {
  filterStatus: 'done', filterPriority: '', filterLabel: '', filterModule: '',
  searchQuery: 'seed', sortField: 'number', sortDir: 'asc', groupBy: 'module', density: 'compact',
};

function fixture(initial = JSON.stringify(saved), unavailable = false) {
  const stored = new Map([[key, initial]]);
  const runtime = handlerFixture(input.signals, async () => { throw Error('controls do not call an API'); }, input.browser_source);
  runtime.context.window.addEventListener = () => {};
  runtime.context.document.getElementById = () => ({focus() {}});
  runtime.context.localStorage = {
    getItem(name) { if (unavailable) throw Error('blocked'); return stored.get(String(name)) ?? null; },
    setItem(name, value) { if (unavailable) throw Error('blocked'); stored.set(String(name), String(value)); },
  };
  const invoke = (id, event = 'click', extra = {}) => {
    const source = input.handlers[`${id}:${event}`];
    assert.ok(source, `actual emitted ${id}:${event} handler`);
    runtime.handler(source)(runtime.cx.event({type: event, preventDefault() {}, stopPropagation() {}, target: {}, ...extra}));
  };
  runtime.handler(input.mount_handler)(runtime.cx.event({type: 'mount'}));
  return {runtime, stored, invoke, state: () => JSON.parse(stored.get(key))};
}

const live = fixture();
assert.deepEqual(live.state(), saved, 'mount hydrates before any persistence');
live.invoke('status:active');
assert.equal(live.state().filterStatus, 'active');
live.invoke('status:active');
assert.equal(live.state().filterStatus, '', 'repeat choice toggles off');
live.invoke('priority:high');
live.invoke('sort:updated');
assert.equal(live.state().sortField, 'updated');
assert.equal(live.state().sortDir, 'desc', 'updated initially sorts newest first');
live.invoke('sort:updated');
assert.equal(live.state().sortDir, 'asc');
live.invoke('clear');
assert.deepEqual(live.state(), {...saved, filterStatus: '', filterPriority: '', searchQuery: '', sortField: 'updated'});
live.invoke('search', 'input', {target: {value: 'needle'}});
assert.equal(live.state().searchQuery, 'needle');
live.invoke('search', 'blur');
const searchSignals = Object.fromEntries(Object.keys(input.signals).map(id => [id, live.runtime.cx.signal(id).dehydrate().v]));
live.invoke('search-open');
live.invoke('search', 'keydown', {key: 'Escape'});
assert.equal(live.state().searchQuery, '');
live.invoke('search', 'blur');
live.invoke('tally:active');
assert.equal(live.state().filterStatus, 'active');
const activeSignals = Object.fromEntries(Object.keys(input.signals).map(id => [id, live.runtime.cx.signal(id).dehydrate().v]));
live.invoke('tally:active');
assert.equal(live.state().filterStatus, '', 'status tally toggles off');
assert.deepEqual([...live.stored.keys()], [key, 'lific:list:layout:ACC'], 'other projects are untouched');
assert.equal(live.stored.get('lific:list:layout:ACC'), 'list');
const before = JSON.stringify([...live.stored]);
live.runtime.controller.abort();
live.invoke('status:done');
assert.equal(JSON.stringify([...live.stored]), before, 'disposed controls cannot persist');

const malformed = fixture('{invalid');
malformed.invoke('status:todo');
assert.equal(malformed.state().filterStatus, 'todo', 'malformed storage recovers to defaults');
assert.equal(Object.keys(malformed.state()).length, 9);
const blocked = fixture('', true);
blocked.invoke('status:active');
blocked.invoke('clear');
process.stdout.write(JSON.stringify({checked: true, active_signals: activeSignals, search_signals: searchSignals}));
