'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');
const {emittedShard} = require('./activity_shard_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const fixture = handlerFixture(input.signals, () => assert.fail('timeline has no procedures'));
const {cx, context} = fixture;
const referencedSignals = new Set();
const signal = cx.signal.bind(cx);
cx.signal = id => {
  referencedSignals.add(id);
  return signal(id);
};
const invoke = source => fixture.handler(source)(cx.event(new Event('click')));
const evaluate = source => require('node:vm').runInNewContext(`cx => (${source})`, context)(cx);
const unbox = value => {
  if (value && typeof value.dehydrate === 'function') value = value.dehydrate();
  while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const hidden = source => unbox(evaluate(source)) === true;
assert.equal(hidden(input.hidden_rows[0]), true, 'the seventh row starts hidden');
invoke(input.recent_handler);
assert.equal(hidden(input.hidden_rows[0]), false, 'Show all reveals later activity');
invoke(input.recent_handler);
assert.equal(hidden(input.hidden_rows[0]), true, 'Show recent only restores the six-row view');
assert.equal(hidden(input.values_hidden), true, 'content old/new values start collapsed');
invoke(input.change_handler);
assert.equal(hidden(input.values_hidden), false, 'the emitted show change action reveals the diff');
invoke(input.change_handler);
assert.equal(hidden(input.values_hidden), true, 'the emitted show change action hides the diff again');
invoke(input.recent_handler);
assert.equal(hidden(input.hidden_rows[0]), false, 'Show all leaves the timeline expanded for refresh');
invoke(input.change_handler);
assert.equal(hidden(input.values_hidden), false, 'the selected diff is open for refresh');
const signals = Object.fromEntries([...referencedSignals]
  .map(id => [id, cx.signal(id).get().dehydrate()]));
const activity_shard = input.shard_marker
  ? emittedShard(input.shard_marker, context, cx, value => {
    if (value && typeof value.dehydrate === 'function') value = value.dehydrate();
    return JSON.parse(JSON.stringify(value));
  })
  : null;
process.stdout.write(JSON.stringify({ok: true, signals, activity_shard}));
