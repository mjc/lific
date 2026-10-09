'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture(input.signals, async () => {}, input.browser_source);
const publicKey = `lific:public:subtab:issues:${input.project_id}`;
const privateKey = `lific:subtab:issues:${input.project_id}`;
const storage = new Map([[publicKey, 'recent'], [privateKey, 'private preference sentinel']]);
runtime.context.localStorage = {
  getItem: key => storage.get(key) ?? null,
  setItem: (key, value) => storage.set(key, String(value)),
  removeItem: key => storage.delete(key),
};
const value = id => runtime.cx.signal(id).dehydrate().v;
const click = name => runtime.handler(input.handlers[name])(
  runtime.cx.event({type: 'click', target: {}, currentTarget: {}}),
);
runtime.handler(input.mount)(runtime.cx.event({type: 'mount', target: {}, currentTarget: {}}));
const restored = value(input.signal_ids.recent);
const selections = [];
for (const tab of ['all', 'recent', 'open', 'closed']) {
  click(tab);
  selections.push(value(input.signal_ids[tab]));
}
assert.deepEqual(selections, ['all', 'recent', 'open', 'closed']);
runtime.controller.abort();
click('all');
process.stdout.write(JSON.stringify({
  after_clicks: selections,
  restored,
  persisted: storage.get(publicKey),
  private_key_untouched: storage.get(privateKey) === 'private preference sentinel',
  disposed_write_blocked: storage.get(publicKey) === 'closed',
  disposed: runtime.controller.signal.aborted,
}));
