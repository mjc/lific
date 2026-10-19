'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('./handler_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture({}, async () => { throw Error('DOM bindings never fetch'); }, input.browser_source);
const browser = runtime.context.__lificNativeMounts.browser(runtime.cx);
for (const method of ['visible_attribute_values', 'click_selector', 'blur_id', 'request_frame']) {
  assert.equal(typeof browser[method], 'function', `${method} exposes the required raw browser operation`);
}

let clicks = 0, blurs = 0, callbackRuns = 0;
const row = (id, {hidden = false, rects = true, visibility = 'visible'} = {}) => ({
  closest: () => hidden ? {} : null,
  getClientRects: () => rects ? [{}] : [],
  getAttribute: name => name === 'data-id' ? id : null,
  visibility,
});
let rows = [row('3'), row('2', {hidden: true}), row('1'), row('4', {rects: false}), row('5', {visibility: 'hidden'})];
runtime.context.document.querySelectorAll = selector => {
  assert.equal(selector, '[data-list] [data-row]');
  return rows;
};
runtime.context.getComputedStyle = node => ({visibility: node.visibility});
runtime.context.document.querySelector = selector => {
  assert.equal(selector, '#current-link');
  return {click() { clicks += 1; }};
};
runtime.context.document.getElementById = id => id === 'search' ? {blur() { blurs += 1; }} : null;
const unbox = value => {
  if (Array.isArray(value)) return Array.from(value, unbox);
  if (value && typeof value === 'object' && Object.hasOwn(value, 'v')) return unbox(value.v);
  return value;
};
const readVector = () => browser.visible_attribute_values(runtime.cx.hydrate('[data-list] [data-row]'), runtime.cx.hydrate('data-id'));
const readRows = () => unbox(readVector().dehydrate());
const vector = readVector();
assert.equal(vector.dehydrate().t, 'Vec', 'DOM collections hydrate as Rust vectors, not tuples');
assert.equal(vector.is_empty().dehydrate(), false);
assert.equal(unbox(vector.len().dehydrate()), '2');
const zero = runtime.cx.hydrate({t: 'usize', bits: vector.dehydrate().bits, v: '0'});
assert.equal(vector.index(zero).toString(), '3');
assert.deepEqual(readRows(), ['3', '1'], 'DOM order excludes rows that cannot be seen');
rows = [row('1'), row('3')];
assert.deepEqual(readRows(), ['1', '3'], 'each read uses the current DOM');
rows = [];
assert.equal(readVector().is_empty().dehydrate(), true, 'empty DOM collections retain vector operations');
browser.click_selector(runtime.cx.hydrate('#current-link'));
browser.blur_id(runtime.cx.hydrate('search'));
browser.blur_id(runtime.cx.hydrate('gone'));
assert.equal(clicks, 1);
assert.equal(blurs, 1);

const frames = new Map();
let nextFrame = 0;
runtime.context.requestAnimationFrame = callback => { frames.set(++nextFrame, callback); return nextFrame; };
runtime.context.cancelAnimationFrame = id => frames.delete(id);
browser.request_frame(() => { callbackRuns += 1; });
assert.equal(callbackRuns, 0, 'frame work is deferred');
for (const [id, callback] of frames) { frames.delete(id); callback(); }
assert.equal(callbackRuns, 1);
browser.request_frame(() => { callbackRuns += 1; });
const pendingFrame = Array.from(frames.values())[0];
runtime.controller.abort();
assert.equal(frames.size, 0, 'owner disposal cancels pending frames');
pendingFrame();
assert.equal(callbackRuns, 1, 'an already queued callback cannot run after disposal');
browser.request_frame(() => { callbackRuns += 1; });
assert.equal(frames.size, 0, 'retired owners cannot schedule frames');
assert.equal(callbackRuns, 1);
process.stdout.write(JSON.stringify({passed: true}));
