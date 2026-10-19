'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const storage = new Map(Object.entries(input.storage ?? {}));
const writes = [];
const runtime = handlerFixture(input.signals, async () => {}, input.browser_source);
let frameId = 0;
const frames = new Map();
runtime.context.requestAnimationFrame = callback => {
  const id = ++frameId;
  frames.set(id, callback);
  return id;
};
runtime.context.cancelAnimationFrame = id => frames.delete(id);
const flushFrames = () => {
  const callbacks = [...frames.values()];
  frames.clear();
  callbacks.forEach(callback => callback());
};
const focus = {count: 0};
const blur = {count: 0};
runtime.context.document.getElementById = id => id === input.input_id ? {
  focus() { focus.count += 1; },
  blur() { blur.count += 1; },
} : null;
runtime.context.localStorage = {
  getItem: key => {
    if (input.storage_denied) throw new Error('storage denied');
    return storage.get(key) ?? null;
  },
  setItem: (key, value) => {
    writes.push([key, String(value)]);
    if (input.storage_denied) throw new Error('storage denied');
    storage.set(key, String(value));
  },
};

const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const read = name => unbox(runtime.cx.signal(input.signal_ids[name]).dehydrate());
const dispatch = (name, type, value = '', key = undefined) => {
  const state = {prevented: false};
  runtime.handler(input.handlers[name])(
    runtime.cx.event({
      type,
      key,
      target: {value},
      currentTarget: {},
      preventDefault() { state.prevented = true; },
    }),
  );
  return state;
};

(async () => {
  dispatch('open', 'click');
  flushFrames();
  assert.equal(read('expanded'), true, 'opening the control expands it');
  assert.equal(focus.count, 1, 'opening focuses the search field');

  dispatch('input', 'input', 'retained query');
  if (input.storage_key && !input.storage_denied) {
    const saved = JSON.parse(storage.get(input.storage_key));
    assert.equal(saved.searchQuery, 'retained query');
    assert.equal(saved.filterStatus, '', 'query writes preserve unrelated preferences');
    assert.equal(saved.sortField, 'priority');
    assert.equal(saved.sortDir, 'asc');
    assert.equal(saved.groupBy, 'status');
  } else if (!input.storage_key) {
    assert.equal(writes.length, 0, 'page search does not write issue preferences');
  }
  const beforeOtherKeySignals = JSON.stringify(Object.fromEntries(
    Object.keys(input.signals).map(id => [id, runtime.cx.signal(id).dehydrate()]),
  ));
  const beforeOtherKeyStorage = JSON.stringify([...storage.entries()]);
  const beforeOtherKeyWrites = writes.length;
  const otherKey = dispatch('escape', 'keydown', 'retained query', 'Enter');
  assert.equal(otherKey.prevented, false, 'other keys keep their browser default');
  assert.equal(
    JSON.stringify(Object.fromEntries(
      Object.keys(input.signals).map(id => [id, runtime.cx.signal(id).dehydrate()]),
    )),
    beforeOtherKeySignals,
    'other keys leave the query and expansion state unchanged',
  );
  assert.equal(JSON.stringify([...storage.entries()]), beforeOtherKeyStorage);
  assert.equal(writes.length, beforeOtherKeyWrites, 'other keys do not write preferences');
  dispatch('blur', 'blur');
  assert.equal(read('expanded'), true, 'a nonempty query stays expanded on blur');

  const escape = dispatch('escape', 'keydown', 'retained query', 'Escape');
  assert.equal(escape.prevented, true, 'Escape prevents the browser default');
  assert.equal(blur.count, 1, 'the emitted Escape handler blurs the search input');
  assert.equal(read('query'), '', 'Escape clears the query');
  assert.equal(read('expanded'), false, 'Escape collapses the control');
  if (input.storage_key && !input.storage_denied) {
    const saved = JSON.parse(storage.get(input.storage_key));
    assert.equal(saved.searchQuery, '');
    assert.equal(saved.filterStatus, '', 'Escape preserves unrelated preferences');
  }

  dispatch('open', 'click');
  flushFrames();
  dispatch('blur', 'blur');
  assert.equal(read('expanded'), false, 'an empty query collapses on blur');

  const focusBeforeRace = focus.count;
  dispatch('open', 'click');
  dispatch('escape', 'keydown', '', 'Escape');
  assert.equal(blur.count, 2, 'both Escape handlers blur the input');
  flushFrames();
  assert.equal(
    focus.count,
    focusBeforeRace,
    'Escape before the next frame prevents focusing the collapsed search input',
  );

  dispatch('open', 'click');
  dispatch('blur', 'blur');
  flushFrames();
  assert.equal(
    focus.count,
    focusBeforeRace,
    'empty blur before the next frame prevents focusing the collapsed search input',
  );

  dispatch('open', 'click');
  const beforeDisposedSignals = JSON.stringify(Object.fromEntries(
    Object.keys(input.signals).map(id => [id, runtime.cx.signal(id).dehydrate()]),
  ));
  const beforeDisposedStorage = JSON.stringify([...storage.entries()]);
  runtime.controller.abort();
  flushFrames();
  assert.equal(focus.count, 2, 'a queued focus callback is cancelled with its owner');
  dispatch('open', 'click');
  dispatch('input', 'input', 'late write');
  dispatch('blur', 'blur');
  dispatch('escape', 'keydown', 'late write', 'Escape');
  assert.equal(
    JSON.stringify(Object.fromEntries(
      Object.keys(input.signals).map(id => [id, runtime.cx.signal(id).dehydrate()]),
    )),
    beforeDisposedSignals,
    'disposed handlers leave their signals unchanged',
  );
  assert.equal(JSON.stringify([...storage.entries()]), beforeDisposedStorage);
  process.stdout.write(JSON.stringify({focuses: focus.count, blurs: blur.count, disposed: true}));
})().catch(error => {
  process.stderr.write(`${error.stack}\n`);
  process.exitCode = 1;
});
