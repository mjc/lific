'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const LEGACY_OVERLAYS = '[role=dialog],[data-native-issue-peek],[data-native-context-menu]';

class FakeElement {
  constructor(descriptor, parent = null) {
    this.descriptor = descriptor;
    this.parentElement = parent;
  }
  hasAttribute(name) { return name === 'hidden' && this.descriptor.selfHidden; }
  closest(selector) {
    if (selector !== '[hidden]') throw new Error(`unexpected closest selector ${selector}`);
    for (let node = this; node; node = node.parentElement) {
      if (node.hasAttribute('hidden')) return node;
    }
    return null;
  }
  getClientRects() {
    return this.descriptor.display === 'none' || !this.descriptor.rects ? [] : [{}];
  }
}

function visibleDescriptor(selector, overrides = {}) {
  return {
    selector,
    selfHidden: false,
    ancestorHidden: false,
    rects: true,
    display: 'block',
    visibility: 'visible',
    ...overrides,
  };
}

function setup({overlays = input.overlays, active = {tagName: 'BODY'}, state = {}, deferMicrotasks = false} = {}) {
  let fetchCalls = 0;
  const fixture = handlerFixture(input.signals, () => {
    fetchCalls += 1;
    throw new Error('the E shortcut must not call a procedure');
  }, input.browser_source);
  const {cx, context, controller, handler} = fixture;
  const registrations = [];
  const pending = [];
  let focusCalls = 0;
  let legacyQueryCalls = 0;
  let allQueryCalls = 0;
  const candidates = overlays.map(descriptor => {
    const parent = descriptor.ancestorHidden
      ? new FakeElement({selfHidden: true, rects: false, display: 'none'})
      : null;
    return new FakeElement(descriptor, parent);
  });
  context.window.addEventListener = (type, callback, options = {}) => {
    const registration = {type, callback, options, removed: false};
    registrations.push(registration);
    options.signal?.addEventListener('abort', () => { registration.removed = true; }, {once: true});
  };
  context.document.activeElement = active;
  context.document.querySelector = selector => {
    legacyQueryCalls += 1;
    assert.equal(selector, LEGACY_OVERLAYS, `unexpected legacy selector ${selector}`);
    return candidates[0] ?? null;
  };
  context.document.querySelectorAll = selector => {
    allQueryCalls += 1;
    assert.equal(selector, LEGACY_OVERLAYS, `Rust must supply the bounded overlay selector: ${selector}`);
    return candidates;
  };
  context.getComputedStyle = element => ({visibility: element.descriptor.visibility});
  context.document.getElementById = id => {
    assert.equal(id, input.body_id, 'focus remains on the existing Page body editor');
    return {focus() { focusCalls += 1; }};
  };
  if (deferMicrotasks) context.queueMicrotask = callback => pending.push(callback);

  const setFromBinding = (expression, value) => {
    const ids = Object.keys(input.signals).filter(id => expression.includes(id));
    assert.equal(ids.length, 1, `expected one signal referenced by actual SSR binding: ${expression}`);
    cx.signal(ids[0]).set(cx.hydrate(value));
  };
  for (const [name, value] of Object.entries(state)) {
    const binding = {
      bodyEditing: input.body_hidden,
      titleEditing: input.title_hidden,
      busy: input.busy_disabled,
      draft: input.textarea.binding,
    }[name];
    setFromBinding(binding, value);
  }
  const values = () => Object.fromEntries(Object.keys(input.signals).map(id => {
    const value = cx.signal(id).get().dehydrate();
    return [id, unbox(value)];
  }));
  const read = expression => {
    const value = vm.runInNewContext(`cx => (${expression})`, context)(cx);
    return unbox(value && typeof value.dehydrate === 'function' ? value.dehydrate() : value);
  };
  const mount = handler(input.keyboard);
  mount(cx.event({type: 'mount'}));
  const keyListener = registrations.find(entry => entry.type === 'keydown');
  assert.ok(keyListener, 'the actual Page mount installs a window key listener');
  assert.equal(keyListener.options.signal, controller.signal, 'the listener uses the Page owner AbortSignal');
  if (overlays.length) {
    assert.ok(context.document.querySelector(LEGACY_OVERLAYS), 'the legacy existence query sees the retained dialog');
  }
  const fire = (key, modifiers = {}) => {
    const beforeAllQueries = allQueryCalls + legacyQueryCalls;
    const event = {
      type: 'keydown', key, ...modifiers, cancelable: true,
      defaultPrevented: false,
      preventDefault() { this.defaultPrevented = true; },
    };
    keyListener.callback(event);
    return {event, allQueryDelta: allQueryCalls + legacyQueryCalls - beforeAllQueries};
  };
  const flush = async () => {
    for (let i = 0; i < 60; i += 1) await Promise.resolve();
    while (pending.length) pending.shift()();
    for (let i = 0; i < 60; i += 1) await Promise.resolve();
  };

  return {
    controller, cx, fetchCalls: () => fetchCalls, fire, flush, focusCalls: () => focusCalls,
    keyListener, pending, read, values, textareaValue: () => read(input.textarea.binding),
  };
}

function unbox(value) {
  while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
}

async function entersEdit(key, options = {}) {
  const test = setup({...options, state: {...options.state, draft: 'Unsaved draft'}});
  const {event} = test.fire(key);
  await test.flush();
  assert.equal(test.read(input.textarea.hidden), false, `${key} enters body edit mode with only retained hidden overlays`);
  assert.equal(event.defaultPrevented, true, `${key} is consumed only when entry succeeds`);
  assert.equal(test.textareaValue(), 'Original body', 'entry copies canonical content over the draft');
  assert.equal(test.focusCalls(), 1, 'successful entry focuses once after the microtask');
  assert.equal(test.fetchCalls(), 0, 'keyboard entry issues no procedure');
}

async function staysClosed(options, key = 'e', modifiers = {}) {
  const test = setup(options);
  const before = test.values();
  const {event} = test.fire(key, modifiers);
  await test.flush();
  assert.deepEqual(test.values(), before, `${key} leaves all Page signals unchanged`);
  const bodyAlreadyEditing = options.state?.bodyEditing === true;
  assert.equal(test.read(input.textarea.hidden), !bodyAlreadyEditing, `${key} does not change body edit mode`);
  assert.equal(event.defaultPrevented, false, `${key} is not consumed when entry is blocked`);
  assert.equal(test.focusCalls(), 0, `${key} does not focus when entry is blocked`);
  assert.equal(test.fetchCalls(), 0, `${key} does not call a procedure`);
}

(async () => {
  assert.ok(input.overlays.some(item => item.selfHidden || item.ancestorHidden),
    'actual Page SSR includes a retained hidden shell dialog');
  await entersEdit('e');
  await entersEdit('E');

  await entersEdit('e', {overlays: [visibleDescriptor('[role=dialog]', {
    ancestorHidden: true,
  })]});
  await entersEdit('e', {overlays: [visibleDescriptor('[role=dialog]', {
    rects: false, display: 'none',
  })]});
  await entersEdit('e', {overlays: [visibleDescriptor('[role=dialog]', {
    visibility: 'hidden',
  })]});

  await staysClosed({overlays: [
    visibleDescriptor('[role=dialog]', {ancestorHidden: true}),
    visibleDescriptor('[role=dialog]'),
  ]});
  await staysClosed({overlays: [visibleDescriptor('[data-native-issue-peek]')]});
  await staysClosed({overlays: [visibleDescriptor('[data-native-context-menu]')]});

  for (const tagName of ['INPUT', 'TEXTAREA', 'SELECT']) {
    await staysClosed({active: {tagName}});
  }
  await staysClosed({active: {tagName: 'DIV', isContentEditable: true}});
  await staysClosed({}, 'x');
  await staysClosed({}, 'e', {ctrlKey: true});
  await staysClosed({}, 'e', {metaKey: true});
  await staysClosed({}, 'e', {altKey: true});
  await staysClosed({state: {bodyEditing: true}});
  await staysClosed({state: {titleEditing: true}});
  await staysClosed({state: {busy: true}});

  const alreadyRetired = setup({overlays: []});
  const beforeRetiredCall = alreadyRetired.values();
  alreadyRetired.controller.abort();
  const {event: retiredEvent, allQueryDelta: retiredQueryDelta} = alreadyRetired.fire('e');
  assert.equal(retiredQueryDelta, 0, 'a retained listener does no DOM work after owner disposal');
  assert.deepEqual(alreadyRetired.values(), beforeRetiredCall,
    'an explicitly retained listener cannot mutate a still-closed Page after disposal');
  assert.equal(alreadyRetired.read(input.textarea.hidden), true);
  assert.equal(retiredEvent.defaultPrevented, false);
  assert.equal(alreadyRetired.focusCalls(), 0);
  assert.equal(alreadyRetired.fetchCalls(), 0);

  const retiring = setup({overlays: [], deferMicrotasks: true});
  const {event: entry} = retiring.fire('e');
  assert.equal(retiring.read(input.textarea.hidden), false, 'the accepted key updates the real Page signal before focus');
  assert.equal(entry.defaultPrevented, true);
  assert.equal(retiring.pending.length, 1, 'focus is queued through the existing guarded microtask');
  const beforeRetainedCall = retiring.values();
  retiring.controller.abort();
  assert.equal(retiring.keyListener.removed, true, 'AbortSignal removes the registered key listener');
  retiring.pending.shift()();
  assert.equal(retiring.focusCalls(), 0, 'queued focus is inert after owner disposal');
  const {event: retainedEvent, allQueryDelta: retainedQueryDelta} = retiring.fire('e');
  assert.equal(retainedQueryDelta, 0, 'a retained listener does no DOM work after disposal');
  assert.deepEqual(retiring.values(), beforeRetainedCall, 'an explicitly retained listener cannot mutate after disposal');
  assert.equal(retainedEvent.defaultPrevented, false);
  assert.equal(retiring.focusCalls(), 0);
  assert.equal(retiring.fetchCalls(), 0, 'all keyboard scenarios avoid API procedures');

  process.stdout.write(JSON.stringify({passed: true}));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
