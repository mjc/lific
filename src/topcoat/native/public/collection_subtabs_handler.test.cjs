'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture(input.signals, async () => {});
const value = id => runtime.cx.signal(id).dehydrate().v;
const click = name => runtime.handler(input.handlers[name])(
  runtime.cx.event({type: 'click', target: {}, currentTarget: {}}),
);
const selections = [];
for (const tab of ['all', 'recent', 'open', 'closed']) {
  click(tab);
  selections.push(value(input.signal_ids[tab]));
}
assert.deepEqual(selections, ['all', 'recent', 'open', 'closed']);
runtime.controller.abort();
process.stdout.write(JSON.stringify({after_clicks: selections, disposed: runtime.controller.signal.aborted}));
