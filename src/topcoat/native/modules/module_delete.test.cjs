'use strict';

const assert = require('node:assert/strict');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const requests = [];
const fixture = handlerFixture(input.signals, async (url, options) => {
  requests.push({url: String(url), body: JSON.parse(options.body)});
  return {ok: true, json: async () => null};
}, input.browser_source);
const navigations = [];
fixture.cx.navigate = destination => navigations.push(String(destination));
const [toggle, openConfirm, confirm, cancel] = input.handlers.map(fixture.handler);
const click = handler => handler(fixture.cx.event({
  type: 'click', target: {}, currentTarget: {},
  preventDefault() {}, stopPropagation() {},
}));
const flush = async () => { for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve(); };

(async () => {
  click(toggle);
  click(openConfirm);
  click(cancel);
  await flush();
  assert.equal(requests.length, 0, 'cancel leaves the module untouched');

  click(toggle);
  click(openConfirm);
  click(confirm);
  await flush();
  assert.equal(requests.length, 1, 'only the final inline confirmation deletes');
  assert.ok(requests[0].url.endsWith('/__native_modules/delete'));
  assert.deepEqual(navigations, [input.destination], 'success returns to the mounted module list');
  process.stdout.write(JSON.stringify({
    cancel_requests: 0,
    delete_requests: requests.length,
    destination: navigations[0],
  }));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
