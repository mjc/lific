'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const output = {};

function fixture(prefix, busy) {
  const ids = Array.from({length: 12}, (_, index) => `${prefix}-${index}`);
  const signals = Object.fromEntries(ids.map((id, index) => [id, [
    '', '', {t: 'usize', bits: 64, v: '0'}, busy, '', '',
    {t: 'i64', bits: 64, v: '0'}, 0,
    0, '', '', '',
  ][index]]));
  const runtime = handlerFixture(signals, () => {
    assert.fail('menu dispatch does not call the backend');
  }, input.browser_source);
  vm.runInNewContext(input.handler_source.replace(/export const (\w+)=/g, 'globalThis.$1='), runtime.context);
  const {cx, context} = runtime;
  const handles = cx.tuple(ids.map(id => cx.signal(id)));
  const account = cx.hydrate({t: 'i64', bits: 64, v: '7'});
  const signal = index => cx.signal(ids[index]);
  return {runtime, cx, context, ids, handles, account, signal};
}

function event(type, properties = {}) {
  const received = new Event(type, {cancelable: true});
  let stopped = false;
  const stopPropagation = received.stopPropagation.bind(received);
  received.stopPropagation = () => {
    stopped = true;
    stopPropagation();
  };
  Object.defineProperty(received, 'propagationStopped', {get: () => stopped});
  Object.defineProperties(received, Object.fromEntries(Object.entries(properties)
    .map(([key, value]) => [key, {value}])));
  return received;
}

// Initial invocation runs from a mount action. Busy state avoids a write while
// asserting that cancellation happens before the factory returns.
const initial = fixture('initial', true);
vm.runInNewContext('globalThis.mountSourceReady = typeof mount === "function"', initial.context);
assert.equal(initial.context.mountSourceReady, true);
const initialEvent = event('mount');
const initialRequest = initial.cx.tuple([
  initial.cx.hydrate('initial-owner'),
  initial.cx.hydrate(true),
  initial.cx.hydrate('save_group'),
  initial.cx.hydrate({t: 'i64', bits: 64, v: '0'}),
  initial.cx.hydrate(''),
]);
initial.context.mount(
  initial.cx,
  initial.cx.event(initialEvent),
  initial.handles,
  initial.account,
  initialRequest,
);
assert.equal(initialEvent.defaultPrevented, true, 'initial invocation cancels its mount event synchronously');
assert.equal(initialEvent.propagationStopped, true, 'initial invocation stops propagation synchronously');
output.initial_cancelled = initialEvent.defaultPrevented;

// A delegated context menu changes its owner signals in the same event turn.
const delegated = fixture('delegated', false);
const listeners = {};
const root = {
  addEventListener(type, listener) { listeners[type] = listener; },
  contains(node) { return node === target; },
};
class FixtureElement {}
delegated.context.Element = FixtureElement;
const target = new FixtureElement();
target.id = 'sidebar-project-row';
target.closest = selector => selector === '[data-ns-contextmenu]' ? target : selector === '.native-home-shell' ? root : null;
target.getAttribute = name => name === 'data-ns-contextmenu' ? 'menu:project:42' : null;
target.getBoundingClientRect = () => ({left: 24, right: 52, bottom: 68});
const owner = {closest: selector => selector === '.native-home-shell' ? root : null};
delegated.context.document.getElementById = id => id === 'delegated-owner' ? owner : target;
delegated.context.window.matchMedia = () => ({matches: false});
delegated.context.requestAnimationFrame = callback => callback();
delegated.context.document.querySelector = () => null;
const delegatedRequest = delegated.cx.tuple([
  delegated.cx.hydrate('delegated-owner'),
  delegated.cx.hydrate(false),
  delegated.cx.hydrate(''),
  delegated.cx.hydrate({t: 'i64', bits: 64, v: '0'}),
  delegated.cx.hydrate(''),
]);
delegated.context.mount(
  delegated.cx,
  delegated.cx.event(event('mount')),
  delegated.handles,
  delegated.account,
  delegatedRequest,
);
assert.equal(typeof listeners.contextmenu, 'function', 'owner installs delegated menu handling');
const contextMenu = event('contextmenu', {target, clientX: 103, clientY: 47});
listeners.contextmenu(contextMenu);
assert.equal(contextMenu.defaultPrevented, true, 'delegated menu event is canceled synchronously');
assert.equal(contextMenu.propagationStopped, true, 'delegated menu event stops propagation synchronously');
assert.equal(delegated.signal(5).get().toString(), 'project', 'menu kind changes before dispatch returns');
assert.equal(delegated.signal(6).get().toString(), '42', 'selected project changes before dispatch returns');
assert.equal(delegated.signal(7).get().toString(), '103', 'menu x position is retained');
assert.equal(delegated.signal(8).get().toString(), '47', 'menu y position is retained');
assert.equal(delegated.signal(9).get().toString(), 'sidebar-project-row', 'focus owner is retained');
output.delegated_menu = delegated.signal(5).get().toString();
process.stdout.write(JSON.stringify(output));
