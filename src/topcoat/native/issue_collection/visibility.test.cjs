'use strict';

const assert = require('node:assert/strict');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const statuses = ['backlog', 'todo', 'active', 'done', 'cancelled'];
const key = 'lific:board:hidden-statuses:ACC';
const otherKey = 'lific:board:hidden-statuses:OTHER';
const controlIds = statuses.map(status => `column:${status}`);
assert.deepEqual(Object.keys(input.handlers).sort(), controlIds.sort(),
  'real board controls emit one visibility handler per status');

function makeFixture(initial = JSON.stringify(statuses)) {
  const stored = new Map([[key, initial], [otherKey, JSON.stringify(['todo'])]]);
  const writes = [];
  const runtime = handlerFixture(input.signals,
    async () => { throw Error('board visibility does not call a service'); }, input.browser_source);
  runtime.context.window.addEventListener = () => {};
  runtime.context.localStorage = {
    getItem(name) { return stored.get(String(name)) ?? null; },
    setItem(name, value) {
      writes.push([String(name), String(value)]);
      stored.set(String(name), String(value));
    },
  };
  const invoke = id => {
    const source = input.handlers[id];
    assert.ok(source, `actual emitted ${id} visibility handler`);
    runtime.handler(source)(runtime.cx.event({type: 'click', preventDefault() {}, stopPropagation() {}}));
  };
  runtime.handler(input.mount_handler)(runtime.cx.event({type: 'mount'}));
  return {runtime, stored, writes, invoke};
}

function signals(runtime) {
  return Object.fromEntries(Object.keys(input.signals).map(id => [
    id,
    runtime.cx.signal(id).dehydrate().v,
  ]));
}

const live = makeFixture();
assert.deepEqual(JSON.parse(live.stored.get(key)), statuses,
  'mount keeps the saved visibility set');
live.invoke('column:active');
assert.deepEqual(JSON.parse(live.stored.get(key)), ['backlog', 'todo', 'done', 'cancelled'],
  'showing Active changes only that status');
const recoveredSignals = signals(live.runtime);
live.invoke('column:active');
assert.deepEqual(JSON.parse(live.stored.get(key)), statuses,
  'a repeated click hides Active again');
assert.equal(live.stored.get(otherKey), JSON.stringify(['todo']),
  'visibility remains isolated per project');
const hiddenSignals = signals(live.runtime);

for (const initial of ['{invalid', 'null', '{}', '[1,"todo"]', 'true']) {
  const malformed = makeFixture(initial);
  malformed.invoke('column:todo');
  assert.deepEqual(JSON.parse(malformed.stored.get(key)), ['todo'],
    'malformed storage recovers from the visible-by-default state');
}

const disposed = makeFixture();
const before = JSON.stringify([...disposed.stored]);
const writesBefore = disposed.writes.length;
disposed.runtime.controller.abort();
disposed.runtime.handler(input.mount_handler)(disposed.runtime.cx.event({type: 'mount'}));
disposed.invoke('column:active');
assert.equal(JSON.stringify([...disposed.stored]), before,
  'disposed owners do not write storage');
assert.equal(disposed.writes.length, writesBefore,
  'disposed handlers do not attempt writes');

process.stdout.write(JSON.stringify({
  recovered_signals: recoveredSignals,
  hidden_signals: hiddenSignals,
}));
