'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture(input.signals, async () => {}, input.browser_source);
runtime.context.URL = URL;
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const readVisible = () => unbox(runtime.cx.signal(input.visible_signal).dehydrate());
const listeners = new Map();
const scrolled = [];
const microtasks = [];
let domVisible = input.initial_visible;
runtime.context.window.location = {href: input.href};
runtime.context.queueMicrotask = callback => microtasks.push(callback);
runtime.context.window.addEventListener = (name, listener) => {
  const current = listeners.get(name) || [];
  current.push(listener);
  listeners.set(name, current);
};
runtime.context.document.querySelector = selector => {
  return {
    scrollIntoView() {
      assert.ok(domVisible >= input.required_visible[selector],
        `${selector} must be visible before scrolling (visible ${domVisible})`);
      scrolled.push(selector);
    },
  };
};
const flushMicrotasks = () => {
  while (microtasks.length > 0) {
    domVisible = Number(readVisible());
    microtasks.shift()();
  }
};

const mount = runtime.handler(input.mount_handler);
mount(runtime.cx.event({type: 'mount', target: {}, currentTarget: {}}));
const afterMount = readVisible();
flushMicrotasks();
runtime.context.window.location.href = input.hashchange_href;
for (const listener of listeners.get('hashchange') || []) listener(runtime.cx.event({type: 'hashchange'}));
const afterHashChange = readVisible();
flushMicrotasks();
runtime.controller.abort();
runtime.context.window.location.href = input.disposed_href;
for (const listener of listeners.get('hashchange') || []) listener(runtime.cx.event({type: 'hashchange'}));

assert.ok(afterMount > input.initial_visible, 'mount reveals the linked comment');
assert.ok(afterHashChange >= afterMount, 'hashchange can reveal an older comment');
assert.ok(scrolled.includes(`#comment-${input.mount_comment}`));
assert.ok(scrolled.includes(`#comment-${input.hashchange_comment}`));
assert.equal(readVisible(), afterHashChange, 'disposing the owner prevents later state changes');
process.stdout.write(JSON.stringify({after_mount: afterMount, after_hashchange: afterHashChange, scrolled}));
