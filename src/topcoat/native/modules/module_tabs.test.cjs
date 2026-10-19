'use strict';

const assert = require('node:assert/strict');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const key = `lific:subtab:modules:${input.project_identifier}`;
const otherProjectKey = 'lific:subtab:modules:OTHER';
const stored = new Map([[key, 'archive'], [otherProjectKey, 'backlog']]);
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

const invalidFixture = handlerFixture(input.signals, async () => ({ok: true}), input.browser_source);
invalidFixture.context.localStorage = {
  getItem: name => name === key ? 'unreleased-tab' : null,
  setItem() { assert.fail('invalid stored tabs are not rewritten'); },
};
const invalidNavigations = [];
invalidFixture.cx.navigate = destination => invalidNavigations.push(String(destination));
invalidFixture.handler(input.mount_handler)(invalidFixture.cx.event({type: 'mount'}));
assert.deepEqual(invalidNavigations, [], 'unknown persisted tab IDs are ignored');

const disposedFixture = handlerFixture(input.signals, async () => ({ok: true}), input.browser_source);
disposedFixture.context.localStorage = {
  getItem: () => 'archive',
  setItem() { assert.fail('disposed tab controls cannot persist a choice'); },
};
const disposedNavigations = [];
disposedFixture.cx.navigate = destination => disposedNavigations.push(String(destination));
disposedFixture.controller.abort();
const disposedBefore = Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, disposedFixture.cx.signal(id).dehydrate().v]));
disposedFixture.handler(input.mount_handler)(disposedFixture.cx.event({type: 'mount'}));
disposedFixture.handler(all.handler)(disposedFixture.cx.event({type: 'click'}));
assert.deepEqual(disposedNavigations, [], 'disposed module owners cannot restore or navigate');
assert.deepEqual(Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, disposedFixture.cx.signal(id).dehydrate().v])), disposedBefore,
'disposed module owners cannot mutate stale signals');

assert.equal(stored.get(otherProjectKey), 'backlog', 'selection does not leak to another project');
process.stdout.write(JSON.stringify({
  restored: 'archive',
  saved: stored.get(key),
  destination: navigations[0],
}));
