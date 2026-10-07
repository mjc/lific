'use strict';

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

const requests = [];
const delayed = [];
const bridges = [];
const redirects = [];
const ownerController = new AbortController();
const cardController = new AbortController();
const launchController = new AbortController();
let launch;
let cx;
const parent = {contains: () => true};
class Element {
  closest(selector) {
    if (selector === '[data-native-tool-connect]') return this;
    if (selector === '[data-native-tools-actions]') return parent;
    return null;
  }
  getAttribute(name) { return name === 'data-native-tool-connect' ? input.tool_id : null; }
}
const context = {
  TextEncoder,
  TextDecoder,
  queueMicrotask,
  Element,
  Event: class { constructor(type) { this.type = type; } },
  navigator: {platform: input.platform ?? ''},
  window: {dispatchEvent() {}},
  document: {
    documentElement: {getAttribute: () => '/app'},
    querySelector: selector => selector === '[data-native-tool-launch]'
      ? {click: () => launch(launchCx.event({type: 'click', target: {}, currentTarget: parent,
        preventDefault() {}, stopPropagation() {}}))}
      : ({dataset: {topcoatUsizeBits: '64'}}),
  },
  fetch: async (url, options) => {
    const path = new URL(url, 'http://localhost').pathname;
    const route = path.slice(path.lastIndexOf('/') + 1);
    requests.push({url, options, route});
    if (!input.responses) return new Promise(() => {});
    const entries = input.responses[route];
    assert.ok(entries, `missing ${route} response`);
    const response = Array.isArray(entries) ? entries.shift() : entries;
    if (['disposed', 'card_disposed'].includes(input.scenario) && route === 'connect') {
      return new Promise(resolve => delayed.push(() => resolve({ok: true, json: async () => response})));
    }
    return Promise.resolve({ok: true, json: async () => response});
  },
};
vm.runInNewContext(runtime.replace(bootstrap,
  `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass}};`), context);
const registry = new context.fixture.Registry();
const launchCx = Object.assign(new context.fixture.Context(registry), {
  abortSignal: launchController.signal,
  withSessionChange(owner, task) {
    bridges.push({owner});
    if (owner.aborted) return Promise.reject(new DOMException('disposed', 'AbortError'));
    return Promise.resolve().then(task);
  },
  redirect: url => redirects.push(url),
  event: event => new context.fixture.Event(event),
});
cx = Object.assign(new context.fixture.Context(registry), {
  abortSignal: cardController.signal,
  withSessionChange: launchCx.withSessionChange,
  redirect: launchCx.redirect,
  event: event => new context.fixture.Event(event),
});
for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
const handler = vm.runInNewContext(`cx => (${input.handler})`, context)(cx);
launch = vm.runInNewContext(`cx => (${input.launch_handler})`, context)(launchCx);
const customIdHandler = input.custom_id_handler &&
  vm.runInNewContext(`cx => (${input.custom_id_handler})`, context)(cx);
const customNameHandler = input.custom_name_handler &&
  vm.runInNewContext(`cx => (${input.custom_name_handler})`, context)(cx);
const confirmHandler = input.confirm_handler &&
  vm.runInNewContext(`cx => (${input.confirm_handler})`, context)(launchCx);
const passwordInputHandler = input.password_input_handler &&
  vm.runInNewContext(`cx => (${input.password_input_handler})`, context)(launchCx);
let draftIds;
if (input.scenario === 'captured_identity' && customIdHandler && customNameHandler) {
  const signalId = source => source.match(/"id":"([^"]+)"/)?.[1];
  draftIds = [signalId(input.custom_id_handler), signalId(input.custom_name_handler)];
  customIdHandler(cx.event({type: 'input', target: {value: 'codex-laptop'}}));
  customNameHandler(cx.event({type: 'input', target: {value: 'Codex laptop draft'}}));
}
handler(cx.event({
  type: 'click',
  target: new Element(),
  currentTarget: new Element(),
  preventDefault() {},
  stopPropagation() {},
}));
if (draftIds) {
  assert.equal(cx.signal(draftIds[0]).dehydrate().v, 'codex-laptop',
    'catalog selection preserves the in-progress custom connection ID draft');
  assert.equal(cx.signal(draftIds[1]).dehydrate().v, 'Codex laptop draft',
    'catalog selection preserves the in-progress custom display name draft');
}

async function settle() {
  for (let index = 0; index < 80; index++) await Promise.resolve();
}

async function run() {
  if (input.scenario === 'captured_identity') {
    customIdHandler(cx.event({type: 'input', target: {value: 'other-client'}}));
    customNameHandler(cx.event({type: 'input', target: {value: 'Other Client'}}));
    await settle();
    assert.equal(requests.length, 1);
    assert.deepEqual(JSON.parse(requests[0].options.body), input.expected_arguments,
      'editing the custom form draft cannot retarget a pending named connection');
    process.stdout.write(JSON.stringify({identity_captured: true}));
    return;
  }
  if (input.mount_handler) {
    const mount = vm.runInNewContext(`cx => (${input.mount_handler})`, context)(launchCx);
    mount(cx.event({type: 'mount'}));
  }
  if (input.scenario === 'setup_before_key') {
    await settle();
    const binding = vm.runInNewContext(`cx => (${input.setup_binding})`, context)(cx).dehydrate();
    assert.equal(binding, true, 'setup remains hidden until a one-time key exists');
    process.stdout.write(JSON.stringify({setup_hidden: true}));
    return;
  }
  if (input.scenario === 'platform_detection') {
    await settle();
    const values = Object.keys(input.signals).map(id => cx.signal(id).dehydrate().v);
    assert.ok(values.includes(input.expected_platform), 'mount detection selects the host operating system');
    process.stdout.write(JSON.stringify({platform_detected: input.expected_platform}));
    return;
  }
  if (input.scenario === 'manual_confirmation') {
    await settle();
    const confirmationVisible = vm.runInNewContext(`cx => (${input.confirmation_binding})`, context)(cx).dehydrate();
    assert.equal(confirmationVisible, false, 'recent-auth failures reveal password confirmation');
    passwordInputHandler(cx.event({type: 'input', target: {value: 'current-password'}}));
    confirmHandler(cx.event({type: 'click', target: {}, currentTarget: parent,
      preventDefault() {}, stopPropagation() {}}));
    await settle();
    const values = Object.keys(input.signals).map(id => cx.signal(id).dehydrate().v);
    assert.deepEqual(requests.map(request => request.route), ['connect', 'confirm_connect', 'profile_session']);
    assert.equal(bridges.length, 1, 'manual confirmation uses the session-rotation bridge once');
    assert.equal(values.includes(input.key), true, 'fresh authority permits the one-time key');
    assert.equal(values.includes('recent authentication required'), false,
      'successful confirmation clears the initial recent-authentication error');
    process.stdout.write(JSON.stringify({requests: requests.length, key_published: true, bridges: bridges.length}));
    return;
  }
  if (input.scenario === 'disposed') {
    await settle();
    assert.equal(delayed.length, 1, 'the request is pending before the owner is disposed');
    launchController.abort();
    delayed.shift()();
    await settle();
    const values = Object.keys(input.signals).map(id => cx.signal(id).dehydrate().v);
    assert.equal(values.includes(input.key), false, 'a disposed owner cannot publish a key');
    assert.deepEqual(requests.map(request => request.route), ['connect']);
    process.stdout.write(JSON.stringify({requests: requests.length, key_published: false}));
    return;
  }
  if (input.scenario === 'card_disposed') {
    await settle();
    assert.equal(delayed.length, 1, 'the parent connect request is pending after card dispatch');
    cardController.abort();
    delayed.shift()();
    await settle();
    const values = Object.keys(input.signals).map(id => launchCx.signal(id).dehydrate().v);
    assert.equal(values.includes(input.key), true,
      'retiring the cards cannot cancel the static parent owner or discard its one-time key');
    process.stdout.write(JSON.stringify({requests: requests.length, key_published: true}));
    return;
  }
  if (input.scenario) {
    await settle();
    const values = Object.keys(input.signals).map(id => cx.signal(id).dehydrate().v);
    const published = values.includes(input.key);
    if (input.scenario === 'automatic_confirmation') {
      assert.deepEqual(requests.map(request => request.route), ['connect', 'confirm_connect', 'profile_session']);
      assert.equal(bridges.length, 1, 'automatic recovery performs exactly one session rotation');
      assert.equal(published, true, 'a one-time key appears after fresh account verification');
      assert.equal(values.includes('recent authentication required'), false,
        'successful automatic recovery clears the earlier recent-authentication error');
    } else if (input.scenario === 'terminal_failure') {
      assert.deepEqual(requests.map(request => request.route), ['connect']);
      assert.equal(bridges.length, 0, 'terminal failures do not rotate sessions');
      assert.equal(published, false, 'terminal failures never publish a key');
    }
    process.stdout.write(JSON.stringify({requests: requests.length, key_published: published, bridges: bridges.length, redirects}));
    return;
  }
  await settle();
  assert.equal(requests.length, 1, 'one click starts one connection attempt');
  assert.match(requests[0].url, /\/__native_settings\/connect$/);
  assert.equal(requests[0].options.method, 'POST');
  assert.deepEqual(JSON.parse(requests[0].options.body), input.expected_arguments,
    'the emitted handler captures the selected connection identity and display name');
  process.stdout.write(JSON.stringify({requests: requests.length, identity_captured: true}));
}

run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
