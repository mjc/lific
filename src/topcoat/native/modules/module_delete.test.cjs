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
const [toggle, openConfirm, confirm, cancel] = input.handlers.map(source => fixture.handler(source));
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

  const failedRequests = [];
  const failed = handlerFixture(input.signals, async (url, options) => {
    failedRequests.push({url: String(url), body: JSON.parse(options.body)});
    throw new Error('offline');
  }, input.browser_source);
  const failedNavigations = [];
  failed.cx.navigate = destination => failedNavigations.push(String(destination));
  const [failedToggle, failedOpen, failedConfirm] = input.handlers.map(source => failed.handler(source));
  const failedClick = handler => handler(failed.cx.event({
    type: 'click', target: {}, currentTarget: {}, stopPropagation() {},
  }));
  failedClick(failedToggle);
  failedClick(failedOpen);
  failedClick(failedConfirm);
  await flush();
  assert.equal(failedRequests.length, 1, 'a confirmed failure sends one request');
  assert.deepEqual(failedNavigations, [], 'failed deletion never navigates away');
  const failedSignals = Object.keys(input.signals).map(id => failed.cx.signal(id).dehydrate().v);
  const errorVisible = failedSignals.includes("Couldn't delete module. Try again.") &&
    input.error_slot_available_outside_panels && failedSignals.includes(true);
  assert.equal(errorVisible, true,
    'failure feedback remains visible in the owner while confirmation stays open for retry');

  const disposed = handlerFixture(input.signals, async () => {
    assert.fail('a disposed module owner must not delete');
  }, input.browser_source);
  const beforeDispose = Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, disposed.cx.signal(id).dehydrate().v]));
  disposed.controller.abort();
  for (const source of input.handlers) {
    disposed.handler(source)(disposed.cx.event({type: 'click', target: {}, currentTarget: {}, stopPropagation() {}}));
  }
  await flush();
  assert.deepEqual(Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, disposed.cx.signal(id).dehydrate().v])), beforeDispose,
  'disposed delete controls cannot mutate their old owner');

  process.stdout.write(JSON.stringify({
    cancel_requests: 0,
    delete_requests: requests.length,
    destination: navigations[0],
    error_visible: errorVisible,
  }));
})().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
