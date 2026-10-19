'use strict';

// Runs the emitted node pointer handlers and their live style/edge bindings.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const {fixtureRuntime} = require('../runtime_fixture.cjs');
const context = {
  TextEncoder,
  TextDecoder,
  queueMicrotask,
  document: {documentElement: {getAttribute: () => '/app'}},
};
vm.runInNewContext(fixtureRuntime(['Context', 'Registry', 'Event']), context);
const registry = new context.fixture.Registry();
const cx = Object.assign(new context.fixture.Context(registry), {
  event: event => new context.fixture.Event(event),
});
for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));

const held = new Set();
const node = {
  setPointerCapture(id) { held.add(id); },
  hasPointerCapture(id) { return held.has(id); },
  releasePointerCapture(id) { held.delete(id); },
};
const run = (source, event) => {
  const handler = vm.runInNewContext(`cx => (${source})`, context)(cx);
  handler(cx.event(event));
};
const makeEvent = (type, {pointerId = 1, x = 10, y = 20, button = 0, primary = true} = {}) => {
  let prevented = false;
  let stopped = false;
  return {
    event: {
      type,
      pointerId,
      clientX: x,
      clientY: y,
      button,
      isPrimary: primary,
      pointerType: 'touch',
      currentTarget: node,
      target: node,
      preventDefault() { prevented = true; },
      stopPropagation() { stopped = true; },
    },
    prevented: () => prevented,
    stopped: () => stopped,
  };
};
const style = vm.runInNewContext(`cx => (${input.style_binding})`, context);
const edgePath = vm.runInNewContext(`cx => (${input.edge_binding})`, context);
const readStyle = () => style(cx).dehydrate();
const readEdge = () => edgePath(cx).dehydrate();
const before = readStyle();
const edgeBefore = readEdge();

const secondary = makeEvent('pointerdown', {pointerId: 8, button: 2, primary: false});
run(input.handlers.pointerdown, secondary.event);
assert.equal(secondary.stopped(), false, 'non-primary/right clicks remain available to the browser');
assert.equal(held.size, 0, 'non-primary/right clicks are not captured');

const shortDown = makeEvent('pointerdown', {pointerId: 9});
run(input.handlers.pointerdown, shortDown.event);
run(input.handlers.pointermove, makeEvent('pointermove', {pointerId: 9, x: 12, y: 21}).event);
assert.equal(readStyle(), before, 'movement below the drag threshold leaves the node in place');
run(input.handlers.pointerup, makeEvent('pointerup', {pointerId: 9}).event);
const shortClick = makeEvent('click', {pointerId: 9});
run(input.handlers.click, shortClick.event);
assert.equal(shortClick.prevented(), false, 'a click-sized pointer movement preserves navigation');

const down = makeEvent('pointerdown');
run(input.handlers.pointerdown, down.event);
assert.equal(down.stopped(), true, 'node drag does not also start viewport pan');
assert.deepEqual([...held], [1], 'node captures touch pointers');
run(input.handlers.pointermove, makeEvent('pointermove', {pointerId: 2, x: 80, y: 90}).event);
assert.equal(readStyle(), before, 'a different pointer cannot move this node');
run(input.handlers.pointermove, makeEvent('pointermove', {x: 50, y: 40}).event);
const moved = readStyle();
assert.ok(
  moved.includes(`left:${input.origin_x + 40}px`) && moved.includes(`top:${input.origin_y + 20}px`),
  `one-to-one pointer delta moves the node exactly: ${moved}`,
);
assert.notEqual(moved, before, 'matching touch pointer moves the node');
assert.notEqual(readEdge(), edgeBefore, 'connected edge follows the moved node');
run(input.handlers.pointerup, makeEvent('pointerup').event);
assert.equal(held.size, 0, 'pointerup releases node capture');
const click = makeEvent('click');
run(input.handlers.click, click.event);
assert.equal(click.prevented(), true, 'drag completion suppresses anchor navigation');

const secondDown = makeEvent('pointerdown', {pointerId: 3});
run(input.handlers.pointerdown, secondDown.event);
run(input.handlers.pointercancel, makeEvent('pointercancel', {pointerId: 3}).event);
held.delete(3); // The browser releases capture when pointercancel is dispatched.
run(input.handlers.lostpointercapture, makeEvent('lostpointercapture', {pointerId: 3}).event);
const cancelledClick = makeEvent('click', {pointerId: 3});
run(input.handlers.click, cancelledClick.event);
assert.equal(cancelledClick.prevented(), false, 'cancelled gesture leaves no stale click suppression');

process.stdout.write(JSON.stringify({before, moved, edgeBefore, edgeMoved: readEdge()}));
