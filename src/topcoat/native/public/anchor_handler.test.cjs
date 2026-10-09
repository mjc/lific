'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture(input.signals, async () => {}, input.browser_source);
runtime.context.URL = URL;
runtime.context.window.location = {href: input.href};
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const ownerListeners = new Map();
const windowListeners = new Map();
const add = (name, callback) => {
  const current = ownerListeners.get(name) || [];
  current.push(callback);
  ownerListeners.set(name, current);
};
const owner = {addEventListener: (name, callback) => add(name, callback)};
runtime.context.window.addEventListener = (name, callback) => {
  const current = windowListeners.get(name) || [];
  current.push(callback);
  windowListeners.set(name, current);
};
const event = type => runtime.cx.event({type, target: owner, currentTarget: owner});
const args = () => input.shard_expressions.map(source => runtime.handler(source).dehydrate());
const read = id => unbox(runtime.cx.signal(id).dehydrate());
const fireHash = href => {
  runtime.context.window.location.href = href;
  for (const listener of windowListeners.get('hashchange') || [])
    listener({type: 'hashchange'});
};

runtime.handler(input.mount_handler)(event('mount'));
if (input.hashchange_href) fireHash(input.hashchange_href);
const beforeClick = args();
if (input.click_handler) runtime.handler(input.click_handler)(event('click'));

let request = args();
if (input.expected_idle_revision !== undefined)
  assert.equal(Number(unbox(beforeClick[6])), input.expected_idle_revision);
if (input.expected_revision !== undefined) {
  assert.equal(Number(unbox(request[6])), input.expected_revision);
  assert.equal(Number(unbox(request[8][4])), input.expected_attempts);
}
if (input.expected_loaded_pages !== undefined)
  assert.equal(Number(unbox(request[8][3])), input.expected_loaded_pages);
if (input.expected_has_more !== undefined)
  assert.equal(Boolean(unbox(request[8][8])), input.expected_has_more);
if (input.fail) {
  for (const listener of ownerListeners.get('topcoat:render-error') || [])
    listener({detail: {path: input.render_path}});
  const failed = args();
  assert.equal(Number(unbox(failed[6])), input.expected_revision);
  fireHash(input.hashchange_href || input.href);
  request = args();
  assert.equal(Number(unbox(request[6])), input.expected_revision,
    'a failed automatic request does not retry at the unchanged loaded count');
}

const count = read(input.count_signal);
if (input.expected_count !== undefined) assert.equal(Number(count), input.expected_count);
const beforeDispose = request;
if (input.dispose) {
  runtime.controller.abort();
  fireHash(input.disposed_href);
  if (input.click_handler) runtime.handler(input.click_handler)(event('click'));
  assert.deepEqual(args(), beforeDispose, 'disposed owners cannot schedule another request');
}

const signals = {};
for (const id of runtime.registry.signals.keys()) signals[id] = runtime.registry.read(id).dehydrate();
process.stdout.write(JSON.stringify({args: request, before_click: beforeClick, count: Number(count), signals}));
