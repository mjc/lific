'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const {fixtureRuntime} = require('../runtime_fixture.cjs');

const values = new Map(Object.entries(input.initial_storage));
const listeners = new Map();
const dispatched = [];
const classes = new Set();
const attributes = new Map();
const media = new Map([
  ['(prefers-color-scheme: dark)', false],
  ['(prefers-reduced-motion: reduce)', true],
]);
const localStorage = {
  getItem(key) {return values.has(key) ? values.get(key) : null;},
  setItem(key, value) {values.set(key, String(value));},
  removeItem(key) {values.delete(key);},
  clear() {values.clear();},
};
class StorageEvent {
  constructor(type, details) {this.type = type; Object.assign(this, details);}
}
class MediaQueryList {
  constructor(query) {this.media = query; this.matches = media.get(query) ?? false;}
  addEventListener(name, listener, options) {
    if (name === 'change') addListener(`media:${this.media}`, listener, options);
  }
}
function addListener(name, listener, options = {}) {
  const entries = listeners.get(name) || [];
  entries.push({listener, signal: options.signal});
  listeners.set(name, entries);
}
function fire(name, event) {
  for (const entry of listeners.get(name) || []) {
    if (!entry.signal?.aborted) entry.listener(event);
  }
}
const context = {
  TextEncoder, TextDecoder, queueMicrotask, Element: class {}, StorageEvent,
  localStorage,
  document: {
    documentElement: {
      setAttribute(key, value) {attributes.set(key, String(value));},
      classList: {
        toggle(name, enabled) {if (enabled) classes.add(name); else classes.delete(name);},
      },
    },
  },
  window: {
    localStorage,
    matchMedia: query => new MediaQueryList(query),
    dispatchEvent(event) {
      dispatched.push(event);
      fire('storage', event);
    },
    addEventListener: addListener,
  },
};
vm.runInNewContext(fixtureRuntime(['Context', 'Registry', 'Event'], '__fixture'), context);
const registry = new context.__fixture.Registry();
const controller = new AbortController();
const cx = Object.assign(new context.__fixture.Context(registry), {
  abortSignal: controller.signal,
  event: event => new context.__fixture.Event(event),
});
for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
registry.insert(input.theme_signal_id, cx.hydrate('system'));
registry.insert(input.motion_signal_id, cx.hydrate('system'));
const motion = cx.signal(input.motion_signal_id);
const theme = cx.signal(input.theme_signal_id);
const handler = source => vm.runInNewContext(`cx => (${source})`, context)(cx);
const snapshot = () => Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, cx.signal(id).dehydrate().v]));
const binding = source => {
  const value = vm.runInNewContext(`cx => (${source})`, context)(cx);
  const result = typeof value === 'function' ? value() : value;
  return result && typeof result.dehydrate === 'function' ? result.dehydrate() : result;
};
const classFor = choice => binding(input.class_bindings[choice]);
const click = source => handler(source)(cx.event({
  type: 'click', target: {}, currentTarget: {contains: () => true},
  preventDefault() {}, stopPropagation() {},
}));

handler(input.preferences_factory)(cx.event({type: 'mount'}), theme);
handler(input.motion_factory)(cx.event({type: 'mount'}), motion);
handler(input.selection_handler)(cx.event({type: 'mount'}));
assert.ok(Object.values(snapshot()).includes('dark'), 'Settings hydrates the persisted theme choice');
assert.ok(Object.values(snapshot()).includes('violet'), 'Settings hydrates the persisted accent choice');
assert.ok(Object.values(snapshot()).includes('compact'), 'Settings hydrates the persisted density choice');
assert.ok(Object.values(snapshot()).includes('lg'), 'Settings hydrates the persisted text size choice');
assert.ok(classFor('lific_theme:dark').includes('bg-[var(--surface)]'));
assert.ok(classFor('lific_theme:light').includes('rounded-md') && classFor('lific_theme:light').includes('text-[var(--text-muted)]'));
assert.ok(classFor('lific_accent:violet').includes('ring-2'));
assert.ok(classFor('lific_accent:teal').includes('size-8') && classFor('lific_accent:teal').includes('hover:scale-110'));
assert.equal(attributes.get('data-theme'), 'dark');
assert.equal(theme.dehydrate().v, 'dark');
assert.equal(attributes.get('data-accent'), 'violet');
assert.equal(attributes.get('data-density'), 'compact');
assert.equal(classes.has('density-compact'), true);
assert.equal(attributes.get('data-font-scale'), 'lg');
assert.equal(attributes.get('data-motion'), 'reduced', 'invalid motion falls back to OS system setting');
assert.equal(motion.dehydrate().v, 'system');

for (const [key, value] of [
  ['lific_theme', 'light'], ['lific_theme', 'dark'], ['lific_theme', 'system'],
  ['lific_accent', 'indigo'], ['lific_accent', 'teal'], ['lific_density', 'compact'], ['lific_density', 'comfortable'],
  ['lific_font_scale', 'sm'], ['lific_font_scale', 'md'],
  ['lific_motion', 'reduced'], ['lific_motion', 'full'], ['lific_motion', 'system'],
]) {
  click(input.handlers[`${key}:${value}`]);
}
assert.equal(values.has('lific_theme'), false, 'System theme removes the stored default');
assert.equal(attributes.get('data-theme'), 'system', 'System theme preserves OS color-scheme CSS');
assert.equal(theme.dehydrate().v, 'system');
assert.ok(classFor('lific_theme:system').includes('bg-[var(--surface)]'), 'theme selected class changes with the signal');
assert.ok(classFor('lific_theme:dark').includes('text-[var(--text-muted)]'));
assert.equal(values.get('lific_accent'), 'teal');
assert.ok(classFor('lific_accent:teal').includes('ring-2'), 'accent selected class changes with the signal');
assert.ok(classFor('lific_accent:violet').includes('hover:scale-110'));
assert.equal(attributes.get('data-accent'), 'teal');
assert.equal(values.has('lific_density'), false, 'Comfortable density removes the stored default');
assert.equal(attributes.get('data-density'), 'comfortable');
assert.equal(classes.has('density-compact'), false);
assert.equal(values.has('lific_font_scale'), false, 'Medium text removes the stored default');
assert.equal(attributes.get('data-font-scale'), 'md');
assert.equal(values.has('lific_motion'), false, 'System motion removes the stored default');
assert.equal(attributes.get('data-motion'), 'reduced', 'System motion resolves against the OS preference');
assert.equal(dispatched.length, 12, 'each click publishes a truthful storage event');
assert.equal(dispatched.at(-1).newValue, null, 'default removal is published as a removal');
assert.ok(dispatched.every(event => event.storageArea === localStorage));

const storedSetItem = localStorage.setItem;
localStorage.setItem = () => {throw new Error('storage quota exceeded');};
assert.doesNotThrow(() => click(input.handlers['lific_accent:rose']));

assert.ok(Object.values(snapshot()).includes('rose'), 'failed writes retain the choice signal');
assert.ok(classFor('lific_accent:rose').includes('ring-2'), 'failed writes keep the selected control');
assert.equal(attributes.get('data-accent'), 'rose', 'failed writes still apply the selected preference');
assert.doesNotThrow(() => click(input.handlers['lific_theme:dark']));
assert.doesNotThrow(() => click(input.handlers['lific_accent:teal']));
assert.equal(attributes.get('data-theme'), 'dark', 'a later failed preference write preserves the earlier in-memory theme');
assert.equal(attributes.get('data-accent'), 'teal', 'later in-memory accent choice applies independently');
assert.ok(Object.values(snapshot()).includes('dark'));
assert.ok(Object.values(snapshot()).includes('teal'));
assert.doesNotThrow(() => click(input.handlers['lific_motion:full']));
assert.doesNotThrow(() => click(input.handlers['lific_motion:system']));
assert.ok(Object.values(snapshot()).includes('system'), 'unpersisted motion default remains selected');
assert.equal(motion.dehydrate().v, 'system');
assert.equal(attributes.get('data-motion'), 'reduced', 'unrelated failed writes do not reset the system motion choice');
assert.doesNotThrow(() => click(input.handlers['lific_motion:reduced']));
media.set('(prefers-reduced-motion: reduce)', false);
fire('media:(prefers-reduced-motion: reduce)', {type: 'change', matches: false});
assert.equal(attributes.get('data-motion'), 'reduced', 'OS changes preserve an explicit motion choice after a failed write');
assert.equal(motion.dehydrate().v, 'reduced');
localStorage.setItem = storedSetItem;

const storageDescriptor = Object.getOwnPropertyDescriptor(context, 'localStorage');
Object.defineProperty(context, 'localStorage', {configurable: true, get() {throw new Error('storage is blocked');}});
assert.doesNotThrow(() => click(input.handlers['lific_theme:dark']));
assert.ok(classFor('lific_theme:dark').includes('bg-[var(--surface)]'));
assert.equal(attributes.get('data-theme'), 'dark', 'blocked storage still applies the selected theme');
assert.equal(dispatched.at(-1).storageArea, null, 'blocked storage publishes a safe removal-compatible event');
Object.defineProperty(context, 'localStorage', storageDescriptor);

values.set('lific_accent', 'rose');
fire('storage', new StorageEvent('storage', {key: 'lific_accent', newValue: 'rose', storageArea: localStorage}));
assert.equal(attributes.get('data-accent'), 'rose', 'external changes update the rendered preference');
assert.ok(Object.values(snapshot()).includes('rose'), 'Settings updates selected state after external changes');
values.set('lific_accent', 'not-a-preset');
fire('storage', new StorageEvent('storage', {key: 'lific_accent', newValue: 'not-a-preset', storageArea: localStorage}));
assert.equal(attributes.get('data-accent'), 'indigo', 'unknown stored values use the Main default');
values.set('lific_accent', 'teal');
fire('storage', new StorageEvent('storage', {key: 'lific_accent', newValue: 'teal', storageArea: {}}));
assert.equal(attributes.get('data-accent'), 'indigo', 'foreign storage areas are ignored');

values.set('lific_motion', 'system');
media.set('(prefers-reduced-motion: reduce)', false);
fire('media:(prefers-reduced-motion: reduce)', {matches: false});
assert.equal(attributes.get('data-motion'), 'full', 'system motion follows OS changes');
localStorage.clear();
fire('storage', new StorageEvent('storage', {key: null, newValue: null, storageArea: localStorage}));
assert.equal(attributes.get('data-theme'), 'system');
assert.equal(attributes.get('data-accent'), 'indigo');
assert.equal(attributes.get('data-density'), 'comfortable');
assert.equal(classes.has('density-compact'), false);
assert.equal(attributes.get('data-font-scale'), 'md');
assert.equal(attributes.get('data-motion'), 'full');
assert.ok(Object.values(snapshot()).includes('indigo'), 'clearing storage restores selected defaults');

controller.abort();
values.set('lific_accent', 'rose');
fire('storage', new StorageEvent('storage', {key: 'lific_accent', newValue: 'rose', storageArea: localStorage}));
assert.equal(attributes.get('data-accent'), 'indigo', 'disposed owners stop receiving preference events');
process.stdout.write(JSON.stringify({all_choices_applied: true}));
