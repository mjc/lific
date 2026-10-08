'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const fixture = handlerFixture(input.signals, () => {
  assert.fail('canonical Module refresh does not issue another request');
}, input.browser_source);
const listeners = new Map();
fixture.context.window.addEventListener = (name, handler) => listeners.set(name, handler);
const referenced = new Set();
const signal = fixture.cx.signal.bind(fixture.cx);
fixture.cx.signal = id => {
  referenced.add(id);
  return signal(id);
};

const bind = source => {
  const before = new Set(referenced);
  vm.runInNewContext(`cx => (${source})`, fixture.context)(fixture.cx);
  const ids = [...referenced].filter(id => !before.has(id));
  assert.equal(ids.length, 1, 'rendered value binding resolves one owned signal');
  return ids[0];
};
const titleDraftId = bind(input.title_binding);
const descriptionDraftId = bind(input.description_binding);
const titleId = findUnique(input.initial_title, titleDraftId);
const descriptionId = findUnique(input.initial_description, descriptionDraftId);
const seqId = findUnique(input.initial_seq);
const read = id => unbox(fixture.cx.signal(id).dehydrate());
fixture.cx.signal(titleDraftId).set(fixture.cx.hydrate('Dirty title draft'));
fixture.cx.signal(descriptionDraftId).set(fixture.cx.hydrate('Dirty description draft'));

fixture.handler(input.mount_handler)(fixture.cx.event({type: 'mount', target: {}}));
const applied = listeners.get('lific:native-issue-module-applied');
assert.equal(typeof applied, 'function', 'actual module document mount listens for canonical owner replies');
const reply = fixture.cx.hydrate(input.reply);
applied({detail: reply});

const canonical = unbox(input.reply.v.canonical);
assert.equal(read(titleId), unbox(canonical.title));
assert.equal(read(descriptionId), unbox(canonical.description));
assert.equal(read(seqId), unbox(input.reply.v.seq), 'sequence advances with its canonical snapshot');
assert.equal(read(titleDraftId), 'Dirty title draft', 'a dirty title draft survives metadata refresh');
assert.equal(read(descriptionDraftId), 'Dirty description draft', 'a dirty body draft survives metadata refresh');
process.stdout.write(JSON.stringify({
  canonical_title: read(titleId),
  canonical_description: read(descriptionId),
  canonical_seq: read(seqId),
  dirty_title: read(titleDraftId),
  dirty_description: read(descriptionDraftId),
}));

function findUnique(expected, excluded) {
  const wanted = JSON.stringify(unbox(expected));
  const ids = Object.keys(input.signals).filter(id => id !== excluded &&
    JSON.stringify(unbox(input.signals[id])) === wanted);
  assert.equal(ids.length, 1, `find exactly one canonical signal for ${wanted}`);
  referenced.add(ids[0]);
  return ids[0];
}

function unbox(value) {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
}
