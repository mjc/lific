'use strict';

// Replay the actual native Files confirmation handlers and observe the account toast request.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
const notifications = [];
const fixture = handlerFixture(input.signals, async (url, options) => {
  requests.push({path: new URL(url, 'http://localhost').pathname, body: JSON.parse(options.body)});
  assert.equal(options.method, 'POST');
  return {ok: !input.fail, status: input.fail ? 403 : 200, json: async () => input.reply};
}, input.browser_source);
fixture.context.CustomEvent = class extends Event {
  constructor(type, options={}) {super(type, options);this.detail=options.detail;}
};
fixture.context.window.dispatchEvent = event => {
  notifications.push({type:event.type, detail:event.detail.dehydrate()});
  return true;
};

async function run() {
  fixture.handler(input.handler)(fixture.cx.event({type: 'click', target: {}}));
  for (let index = 0; index < 80; index += 1) await Promise.resolve();

  if (input.phase === 'open') {
    assert.equal(requests.length, 0, 'opening inline confirmation does not delete the file');
    assert.equal(notifications.length, 0, 'opening confirmation does not show success');
  } else {
    assert.equal(requests.length, 1, 'confirming deletion uses one emitted procedure call');
    assert.ok(requests[0].path.endsWith('/__native_files/delete'), requests[0].path);
    if (input.phase === 'failure') {
      assert.deepEqual(notifications, [], 'failed deletion never emits a success toast');
    } else {
      assert.deepEqual(notifications, [{type:'lific:native-toast-success',detail:input.success_request}],
        'only a successful emitted delete dispatches the account-owned toast request');
    }
  }
  const signals = Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, fixture.cx.signal(id).get().dehydrate()]));
  process.stdout.write(JSON.stringify({requests, notifications, signals}));
}

run().catch(error => {
  process.stderr.write(`${error.stack}\n`);
  process.exitCode = 1;
});
