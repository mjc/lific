'use strict';

// Run the emitted in-row confirmation and member mutation handlers against
// replies produced by the real native procedures.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const {cx, handler} = handlerFixture(input.signals, (url, options) => {
    const path = new URL(url, 'http://localhost').pathname;
    assert.equal(path, '/__native_instance_settings/member_action');
    assert.equal(options.method, 'POST');
    input.requests.push(JSON.parse(options.body));
    const reply = input.replies[input.requests.length - 1];
    assert.ok(reply, 'one real procedure reply is provided for every emitted mutation');
    return Promise.resolve({ok: true, json: async () => reply});
}, input.browser_source);
cx.withSessionChange = (_owner, task) => task();
const click = source => handler(source)(cx.event({type: 'click'}));
const signalValue = id => {
  const unbox = value => {
    if (Array.isArray(value)) return value.map(unbox);
    while (value !== null && typeof value === 'object') {
      if (Object.hasOwn(value, 'v')) value = value.v;
      else if (Object.hasOwn(value, 'n')) value = Number(value.n);
      else break;
    }
    return value;
  };
  return unbox(cx.signal(id).dehydrate());
};
const settle = async () => { for (let i = 0; i < 80; i++) await Promise.resolve(); };

async function run() {
  click(input.demote_handler);
  click(input.confirm_handler);
  await settle();
  assert.deepEqual(input.requests[0], input.expected_demote_args,
    'the in-row Demote confirmation submits the captured demote action');
  assert.equal(signalValue(input.admin_signal_id), input.admin_after_demote,
    'a successful demotion updates the visible role signal');
  assert.equal(signalValue(input.active_signal_id), input.active_after_demote,
    'a role change preserves the member active state');

  click(input.deactivate_handler);
  click(input.confirm_handler);
  await settle();
  assert.deepEqual(input.requests[1], input.expected_deactivate_args,
    'the in-row Deactivate confirmation submits the captured deactivate action');
  assert.equal(signalValue(input.active_signal_id), input.active_after_deactivate,
    'a successful deactivation updates the visible active-state signal');
  process.stdout.write(JSON.stringify({requests: input.requests}));
}

run().catch(error => { process.stderr.write(String(error.stack || error)); process.exitCode = 1; });
