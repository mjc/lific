'use strict';

const assert = require('node:assert/strict');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const key = `lific:subtab:modules:${input.project_id}`;
const stored = new Map([[key, 'archive']]);
const fixture = handlerFixture(input.signals, async () => ({ok: true}), input.browser_source);
fixture.context.localStorage = {
  getItem: name => stored.get(String(name)) ?? null,
  setItem: (name, value) => stored.set(String(name), String(value)),
};
const {cx} = fixture;
const navigations = [];
cx.navigate = destination => navigations.push(String(destination));
const invoke = source => fixture.handler(source)(cx.event({type: 'mount'}));

invoke(input.mount_handler);
assert.ok(navigations.at(-1)?.endsWith('/ACC/modules?tab=archive'),
  'mount restores the saved lifecycle tab through the mounted route');

const all = input.tabs.find(tab => tab.id === 'all');
assert.ok(all, 'the All tab has a persistence handler');
fixture.handler(all.handler)(cx.event({type: 'click', preventDefault() {}}));
assert.equal(stored.get(key), 'all', 'selecting a lifecycle tab stores it per project');

const otherProjectKey = `lific:subtab:modules:${input.project_id + 1}`;
assert.equal(stored.has(otherProjectKey), false, 'selection does not leak to another project');
process.stdout.write(JSON.stringify({
  restored: 'archive',
  saved: stored.get(key),
  destination: navigations[0],
}));
