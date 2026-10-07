'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const source = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(source.split(bootstrap).length - 1, 1);
const settle = async () => {for (let i = 0; i < 40; i++) await Promise.resolve();};

async function run(authorityLost, check = input.check) {
  const requests = [], sockets = [], redirects = [], timers = [];
  let runtime, finish, startedPaused = false;
  const context = {
    TextEncoder, TextDecoder, URL, queueMicrotask,
    setTimeout: (fn, delay) => {timers.push({fn, delay}); return timers.length;}, clearTimeout() {},
    AbortController, DOMException, Event, EventTarget, Response, Headers,
    document: Object.assign(new EventTarget(), {
      querySelector: () => ({dataset: {topcoatUsizeBits: '64'}}),
      documentElement: {getAttribute: () => '/app'}, activeElement: null, readyState: 'complete',
    }),
    location: {href: 'https://example.test/app/settings', assign(value) {redirects.push(String(value));}},
    window: Object.assign(new EventTarget(), {}),
    fetch(url, options) {
      requests.push(url);
      if (url.endsWith('/password')) {
        startedPaused = runtime.sessionChange != null && runtime.connection.suspended;
        return new Promise(resolve => {finish = () => resolve({ok: true, json: async () => input.saved});});
      }
      assert.ok(url.endsWith('/profile_session'), 'only native password and authority procedures are used');
      if (authorityLost === 'network') return Promise.reject(new Error('offline'));
      return Promise.resolve({ok: true, json: async () => authorityLost === 'missing' ? input.absent_authority : authorityLost ? {t: 'Result', err: 'Your account changed.'} : input.authority});
    },
  };
  vm.runInNewContext(source.replace(bootstrap, 'globalThis.fixture={Runtime:ye};'), context);
  runtime = new context.fixture.Runtime();
  runtime.connection.open = () => {
    const listeners = {};
    const socket = {readyState: 0, addEventListener: (name, listener) => {listeners[name] = listener;},
      send() {}, close() {}, opened() {this.readyState = 1; listeners.open();}};
    sockets.push(socket);
    return socket;
  };
  const owner = new AbortController();
  const cx = Object.assign(Object.create(runtime.context), {abortSignal: owner.signal});
  for (const [id, value] of Object.entries(input.signals)) runtime.registry.insert(id, cx.hydrate(value));
  const handler = source => vm.runInNewContext(`cx => (${source})`, context)(cx);
  handler(input.current)(cx.event({target: {value: 'testpassword1'}}));
  handler(input.next)(cx.event({target: {value: 'replacement-password'}}));
  const member = {label: 'Shard', rerunRequest: () => ({url: '/__native_tools', headers: {}, body: '{}'}),
    connectionOpened() {runtime.connection.run(this);}, reportError(error) {throw error;}};
  runtime.connection.join(member);
  sockets[0].opened();
  const scope = runtime.page.contentScope;
  handler(input.submit)(cx.event({type: 'click'}));
  await settle();
  assert.equal(startedPaused, true, 'password rotation pauses the old-cookie transport before the procedure');
  assert.equal(sockets.length, 1);
  finish();
  await settle();
  assert.ok(requests.some(url => url.endsWith('/profile_session')), 'a successful reply needs fresh account authority');
  assert.equal(runtime.page.contentScope, scope, 'canonical state is published without replacing the owner');
  assert.equal(sockets.length, authorityLost ? 1 : 2, 'only a current owner resumes its connection');
  const values = Object.keys(input.signals).map(id => cx.signal(id).dehydrate().v);
  assert.ok(!values.includes('testpassword1') && !values.includes('replacement-password'), 'success clears both password drafts');
  if (authorityLost) {
    assert.deepEqual(redirects, ['/app/'], 'a replaced account returns to fresh cookie authority');
  } else {
    assert.deepEqual(redirects, [], 'success keeps the current page');
    if (check === 'validation_reset') {
      const success = Object.keys(input.signals).find(id => input.signals[id] === false && cx.signal(id).dehydrate().v === true);
      assert.ok(success, 'success is initially visible');
      const count = requests.length;
      handler(input.next)(cx.event({target: {value: 'short'}}));
      handler(input.submit)(cx.event({type: 'click'}));
      await settle();
      assert.equal(requests.length, count, 'invalid passwords never start a request');
      assert.equal(cx.signal(success).dehydrate().v, false, 'a new invalid attempt clears the previous success');
    }
    if (check === 'expiry') {
      const success = Object.keys(input.signals).find(id => input.signals[id] === false && cx.signal(id).dehydrate().v === true);
      assert.ok(success, 'success is visible');
      const expiry = timers.find(timer => timer.delay === 6000);
      assert.ok(expiry, 'Main expires password success after six seconds');
      expiry.fn();
      await settle();
      assert.equal(cx.signal(success).dehydrate().v, false);
    }
  }
  runtime.connection.reset();
}
(async () => {
  await run(false);
  await run(false, 'validation_reset');
  await run(true);
  await run('missing');
  await run('network');
  process.stdout.write(JSON.stringify({safe_rotation: true}));
})().catch(error => {console.error(error); process.exitCode = 1;});
