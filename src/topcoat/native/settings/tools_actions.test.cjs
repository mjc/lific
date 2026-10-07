'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(runtime.split(bootstrap).length - 1, 1);
const eventClass = runtime.match(/event\(\w+\)\{return new (\w+)\(\w+\)\}/)?.[1];
assert.ok(eventClass, 'packaged Event surrogate');
const settle = async () => {for (let i = 0; i < 30; i++) await Promise.resolve();};

async function run(first) {
  const pending = new Map();
  const controller = new AbortController();
  class Element {}
  const context = {
    TextEncoder, TextDecoder, queueMicrotask, Element,
    Event: class {constructor(type) {this.type = type;}},
    document: {querySelector: () => ({dataset: {topcoatUsizeBits: '64'}}), documentElement: {getAttribute: () => '/app'}},
    window: {dispatchEvent() {}},
    fetch(url, options) {
      if (url.endsWith('/profile_session')) {
        return Promise.resolve({ok: true, json: async () => input.responses.authority});
      }
      assert.match(url, /\/__native_settings\/(?:connect|bot_action)$/,
        'tool actions use native procedures');
      const action = url.endsWith('/connect') ? 'connect' : 'bot';
      assert.equal(options.method, 'POST');
      assert.ok(!pending.has(action), 'one request per action');
      return new Promise(resolve => pending.set(action, {resolve, body: JSON.parse(options.body)}));
    },
  };
  vm.runInNewContext(runtime.replace(bootstrap,
    `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass}};`), context);
  const registry = new context.fixture.Registry();
  const cx = Object.assign(new context.fixture.Context(registry), {
    abortSignal: controller.signal,
    event: event => new context.fixture.Event(event),
  });
  for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
  const handler = source => vm.runInNewContext(`cx => (${source})`, context)(cx);
  const snapshot = () => Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, cx.signal(id).dehydrate().v]));
  const baseline = snapshot();
  const parent = {contains: () => true};
  const click = target => cx.event({type: 'click', target, currentTarget: parent,
    preventDefault() {}, stopPropagation() {}});
  const botHandler = handler(input.bot_handler);
  botHandler(click(Object.assign(new Element(), {closest: () => null})));
  await settle();
  assert.equal(pending.size, 0, 'ordinary Tools clicks do not dispatch bot mutations');
  handler(input.connect_handler)(click({closest: () => null}));
  const button = Object.assign(new Element(), {
    dataset: {nativeBotAction: input.wire},
    getAttribute: name => name === 'data-native-bot-action' ? input.wire : null,
    closest() {return this;},
  });
  botHandler(click(button));
  await settle();
  assert.equal(pending.size, 2, 'connecting and updating another connection can overlap');
  const during = snapshot();
  const botBusy = Object.keys(during).find(id => during[id]?.v === input.bot_id);
  assert.ok(botBusy, 'bot mutation records its pending identity');
  assert.ok(Object.keys(during).some(id => baseline[id] === false && during[id] === true),
    'the connection is pending alongside the bot mutation');
  assert.equal(pending.get('bot').body[1].v, input.bot_id,
    'delegated bot IDs remain exact above JavaScript safe-integer range');
  assert.deepEqual(pending.get('bot').body.slice(1), JSON.parse(input.wire),
    'bot identity and action retain the rendered lossless wire values');
  const finish = async action => {
    pending.get(action).resolve({ok: true, json: async () => input.responses[action]});
    await settle();
  };
  await finish(first);
  await finish(first === 'connect' ? 'bot' : 'connect');
  const final = snapshot();
  assert.deepEqual(final[botBusy], baseline[botBusy], 'bot mutation clears its pending identity');
  assert.ok(Object.values(final).includes(input.key), 'one-time key survives both completion orders');
  assert.ok(Object.keys(final).some(id =>
    (baseline[id] === 0 && final[id] === 2) ||
    (baseline[id]?.v === '0' && final[id]?.v === '2')),
    'both completed mutations refresh the connection list');
  assert.ok(Object.keys(during).some(id => baseline[id] === false && during[id] === true && final[id] === false),
    'connection busy settles while its setup dialog remains open');
}

(async () => {
  await run('connect');
  await run('bot');
  process.stdout.write(JSON.stringify({both_orders: true}));
})().catch(error => {console.error(error); process.exitCode = 1;});
