'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture(input.signals,
  async () => { throw Error('activity controls do not call an endpoint'); },
  input.browser_source);
const invoke = source => runtime.handler(source)(runtime.cx.event({
  type: 'click', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {},
}));
const snapshot = () => Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, runtime.cx.signal(id).dehydrate().v]));
const initial = snapshot();
invoke(input.show_all);
const afterShowAll = snapshot();
assert.notDeepEqual(afterShowAll, initial,
  'the actual Show all handler updates the shared timeline signal');
invoke(input.show_change);
const afterDescription = snapshot();
assert.notDeepEqual(afterDescription, afterShowAll,
  'the actual description handler updates its row expansion signal');
process.stdout.write(JSON.stringify({signals: afterDescription}));
