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
const listeners = new Map();
const ownerListeners = new Map();
const scrolled = [];
const microtasks = [];
const rows = new Set(input.rows || []);
const add = (listeners, name, callback) => {
  const current = listeners.get(name) || [];
  current.push(callback);
  listeners.set(name, current);
};
const owner = {addEventListener: (name, callback) => add(ownerListeners, name, callback)};
runtime.context.window.location = {href: input.href};
runtime.context.queueMicrotask = callback => microtasks.push(callback);
runtime.context.window.addEventListener = (name, listener) => add(listeners, name, listener);
runtime.context.document.querySelector = selector => rows.has(selector) ? {
  scrollIntoView() { scrolled.push(selector); },
} : null;
const event = type => runtime.cx.event({type, target: owner, currentTarget: owner});
const argumentsNow = () => input.shard_expressions.map(source => runtime.handler(source).dehydrate());
const read = id => unbox(runtime.cx.signal(id).get().dehydrate());
const flush = () => { while (microtasks.length > 0) microtasks.shift()(); };
const fireHash = href => {
  runtime.context.window.location.href = href;
  for (const listener of listeners.get('hashchange') || []) listener({type: 'hashchange'});
};

runtime.handler(input.mount_handler)(event('mount'));
if (input.repeat_mount) runtime.handler(input.mount_handler)(event('mount'));
if (input.manual) runtime.handler(input.click_handler)(event('click'));
let args = input.shard_expressions ? argumentsNow() : [];
if (input.expected_revision !== undefined) {
  assert.equal(Number(unbox(args[6])), input.expected_revision);
  assert.equal(Number(unbox(args[8][4])), input.expected_attempts);
  assert.equal(unbox(args[7][1]), true, 'activation marks loading busy');
}
if (input.click_handler && input.shard_expressions) {
  runtime.handler(input.click_handler)(event('click'));
  assert.deepEqual(argumentsNow(), args, 'a busy boundary ignores repeated clicks');
}
if (input.fail) {
  for (const listener of ownerListeners.get('topcoat:render-error') || [])
    listener({detail: {path: input.render_path}});
  args = argumentsNow();
  assert.equal(unbox(args[7][1]), false, 'failure clears busy');
  assert.ok(unbox(args[7][3]).length > 0, 'failure supplies retry text');
  fireHash(input.href);
  assert.equal(Number(unbox(argumentsNow()[6])), Number(unbox(args[6])),
    'the unchanged row count suppresses automatic retries after failure');
  runtime.handler(input.click_handler)(event('click'));
  args = argumentsNow();
  assert.equal(Number(unbox(args[6])), input.expected_revision + 1, 'manual retry activates same cursor');
  assert.equal(Number(unbox(args[8][4])), input.expected_attempts, 'manual retry does not reset automatic budget');
}
if (!input.cancel_scroll) flush();
if (input.hashchange_href) fireHash(input.hashchange_href);
if (input.expected_count !== undefined) assert.equal(Number(read(input.count_signal)), input.expected_count);
flush();
if (input.expected_scroll) assert.ok(scrolled.includes(input.expected_scroll), JSON.stringify(scrolled));
if (input.forbidden_scroll) assert.ok(!scrolled.includes(input.forbidden_scroll), JSON.stringify(scrolled));
const beforeDispose = input.shard_expressions ? argumentsNow() : [];
runtime.controller.abort();
if (input.disposed_href) fireHash(input.disposed_href);
if (input.click_handler) runtime.handler(input.click_handler)(event('click'));
if (input.shard_expressions) assert.deepEqual(argumentsNow(), beforeDispose, 'disposed owners cannot activate requests');
const signals = {};
for (const id of runtime.registry.signals.keys()) signals[id] = runtime.registry.read(id).dehydrate();
process.stdout.write(JSON.stringify({args: input.shard_expressions ? argumentsNow() : [], signals, scrolled}));
