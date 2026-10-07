'use strict';

// Executes the actual emitted Fit handler and pointer listeners against the
// packaged Topcoat runtime, then inspects the live transform binding.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(runtime.split(bootstrap).length - 1, 1, 'packaged bootstrap');
const eventClass = runtime.match(/event\(\w+\)\{return new (\w+)\(\w+\)\}/)?.[1];
assert.ok(eventClass, 'packaged Event surrogate');
const viewport = {clientWidth: 800, clientHeight: 600};
const transform = {offsetWidth: input.width, offsetHeight: input.height};
viewport.querySelector = selector => selector === '[data-native-graph-transform]' ? transform : null;
const fitTarget = {closest: selector => selector === '[data-native-graph-viewport]' ? viewport : null};
const listeners = new Map();
const captures = [];
const heldCaptures = new Set();
const releasedCaptures = [];
let abortController = new AbortController();
const listen = (collection, key, listener, options = {}) => {
  collection.set(key, listener);
  options.signal?.addEventListener('abort', () => {
    if (collection.get(key) === listener) collection.delete(key);
  }, {once: true});
};
const surface = {
  style: {},
  addEventListener(type, listener, options) { listen(listeners, `surface:${type}`, listener, options); },
  setPointerCapture(id) {
    assert.equal(typeof id, 'number', 'pointer capture receives a primitive numeric ID');
    captures.push(id);
    heldCaptures.add(id);
  },
  hasPointerCapture(id) { return heldCaptures.has(id); },
  releasePointerCapture(id) {
    assert.equal(typeof id, 'number', 'pointer release receives a primitive numeric ID');
    if (!heldCaptures.has(id)) throw new Error('NotFoundError');
    heldCaptures.delete(id);
    releasedCaptures.push(id);
  },
};
const windowListeners = new Map();
const context = {
  TextEncoder,
  TextDecoder,
  queueMicrotask,
  document: {documentElement: {getAttribute: () => '/app'}},
  window: {addEventListener(type, listener, options) { listen(windowListeners, type, listener, options); }},
  fetch: async () => { throw new Error('viewport controls must not make a request'); },
};
vm.runInNewContext(runtime.replace(bootstrap,
  `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass}};`), context);
const registry = new context.fixture.Registry();
const cx = Object.assign(new context.fixture.Context(registry), {
  abortSignal: abortController.signal,
  event: event => new context.fixture.Event(event),
});
for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
const runHandler = (source, event) => {
  const handler = vm.runInNewContext(`cx => (${source})`, context)(cx);
  handler(cx.event(event));
};
const styleBinding = vm.runInNewContext(`cx => (${input.style_binding})`, context);
const style = () => styleBinding(cx).dehydrate();
const snapshot = () => Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, cx.signal(id).dehydrate().v]));

runHandler(input.fit_handler, {type: 'click', target: fitTarget, preventDefault() {}, stopPropagation() {}});
const fitStyle = style();
runHandler(input.pan_mount, {type: 'mount', target: surface});
const down = (pointerId, {button = 0, isPrimary = true, interactive = false, pointerType = 'mouse'} = {}) => {
  listeners.get('surface:pointerdown')({
    type: 'pointerdown', button, isPrimary, pointerId, pointerType, clientX: 10, clientY: 20,
    currentTarget: surface,
    target: {closest: () => interactive ? {} : null},
  });
};
down(1, {button: 2});
down(2, {isPrimary: false, pointerType: 'touch'});
down(3, {interactive: true});
assert.deepEqual(captures, [], 'buttons, secondary pointers, and interactive descendants do not start panning');
assert.equal(surface.style.touchAction, 'none', 'touch input stays in viewport pan handling');
const guardedStyle = style();
const beforeDrag = snapshot();
down(7, {pointerType: 'touch'});
assert.deepEqual(captures, [7], 'primary touch on the background is captured');
const duringDrag = snapshot();
const activeId = Object.keys(duringDrag).find(id => duringDrag[id] === true && beforeDrag[id] !== true);
assert.ok(activeId, 'background pointerdown records active drag state');
windowListeners.get('pointermove')({type: 'pointermove', pointerId: 7, clientX: 30, clientY: 35});
const panStyle = style();
windowListeners.get('pointercancel')({type: 'pointercancel', pointerId: 7});
heldCaptures.delete(7); // The browser releases capture after pointercancel.
const cancelStyle = style();
down(8, {pointerType: 'touch'});
windowListeners.get('pointermove')({type: 'pointermove', pointerId: 8, clientX: 35, clientY: 40});
const activeDragStyle = style();
abortController.abort();
const afterAbort = snapshot();
assert.equal(afterAbort[activeId], false, 'disposing a viewport cancels its active drag');
assert.deepEqual(releasedCaptures, [8], 'disposing a viewport releases its still-held pointer capture');
const abortStyle = style();
const cursorAfterAbort = surface.style.cursor;
const listenersRemoved = !windowListeners.has('pointermove');
abortController = new AbortController();
cx.abortSignal = abortController.signal;
runHandler(input.pan_mount, {type: 'mount', target: surface});
windowListeners.get('pointermove')({type: 'pointermove', pointerId: 8, clientX: 60, clientY: 70});
const remountIdleStyle = style();
assert.equal(remountIdleStyle, abortStyle, 'a remounted viewport ignores moves until a fresh pointerdown');
down(9, {pointerType: 'touch'});
windowListeners.get('pointermove')({type: 'pointermove', pointerId: 9, clientX: 20, clientY: 30});
const remountDragStyle = style();
heldCaptures.delete(9); // Capture can be lost before the viewport is disposed.
abortController.abort();
assert.deepEqual(releasedCaptures, [8], 'disposing after capture was lost does not attempt a second release');
assert.equal(snapshot()[activeId], false, 'disposing after capture loss still clears the active drag');
process.stdout.write(JSON.stringify({
  fit_style: fitStyle,
  guarded_style: guardedStyle,
  pan_style: panStyle,
  cancel_style: cancelStyle,
  active_drag_style: activeDragStyle,
  drag_active_after_abort: afterAbort[activeId],
  listeners_removed: listenersRemoved,
  cursor_after_abort: cursorAfterAbort,
  remount_idle_style: remountIdleStyle,
  remount_drag_style: remountDragStyle,
}));
