'use strict';

// Runs the rendered sign-out handler with delayed procedure responses. Each
// scenario gets a fresh signal registry and the scope's real AbortSignal.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const {fixtureRuntime} = require('../runtime_fixture.cjs');
assert.equal(typeof input.handler, 'string', 'rendered sign-out click handler');
assert.equal(typeof input.destination, 'string', 'mounted sign-out destination');
assert.ok(input.success_response, 'actual serialized procedure success response');

const settle = async () => {
  for (let i = 0; i < 30; i++) await Promise.resolve();
};

function fixture() {
  const controller = new AbortController();
  const requests = [];
  const navigations = [];
  let resolveResponse;
  let rejectResponse;
  const response = new Promise((resolve, reject) => {
    resolveResponse = resolve;
    rejectResponse = reject;
  });
  const assign = destination => navigations.push(destination);
  const context = {
    TextEncoder, TextDecoder, queueMicrotask,
    Event: class {constructor(type) {this.type = type;}},
    document: {documentElement: {getAttribute: () => input.prefix ?? ''}},
    window: {location: {assign}, dispatchEvent() {}},
    location: {assign},
    fetch: (url, options) => {
      assert.ok(url.endsWith('/__native_settings/sign_out'),
        'sign-out uses its native procedure');
      requests.push({url, options});
      return response;
    },
  };
  vm.runInNewContext(fixtureRuntime(['Context', 'Registry', 'Event']), context);
  const registry = new context.fixture.Registry();
  const cx = Object.assign(new context.fixture.Context(registry), {
    abortSignal: controller.signal,
    event: event => new context.fixture.Event(event),
  });
  for (const [id, value] of Object.entries(input.signals)) {
    registry.insert(id, cx.hydrate(value));
  }
  const snapshot = () => Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, cx.signal(id).dehydrate().v]));
  const handler = vm.runInNewContext(`cx => (${input.handler})`, context)(cx);
  return {
    controller, requests, navigations, snapshot,
    click() {
      handler(cx.event({type: 'click', preventDefault() {}, stopPropagation() {}}));
    },
    succeed(body = input.success_response) {
      resolveResponse({ok: true, status: 200, json: async () => body});
    },
    fail() {rejectResponse(new Error('delayed transport failure'));},
  };
}

async function pending() {
  const current = fixture();
  current.click();
  await settle();
  assert.equal(current.requests.length, 1, 'one procedure request starts before disposal');
  assert.equal(current.requests[0].options.method, 'POST');
  assert.deepEqual(current.navigations, [], 'sign-out waits for its procedure result');
  return current;
}

async function run() {
  const disposedSuccess = await pending();
  disposedSuccess.controller.abort();
  const beforeSuccess = disposedSuccess.snapshot();
  disposedSuccess.succeed();
  await settle();
  assert.deepEqual(disposedSuccess.navigations, [],
    'a disposed Settings handler must not redirect after delayed sign-out success');
  assert.deepEqual(disposedSuccess.snapshot(), beforeSuccess,
    'delayed sign-out success must not change disposed Settings state');

  const liveSuccess = await pending();
  liveSuccess.succeed();
  await settle();
  assert.deepEqual(liveSuccess.navigations, [input.destination],
    'a live Settings handler redirects once to the mounted login destination');

  const disposedFailure = await pending();
  disposedFailure.controller.abort();
  const beforeFailure = disposedFailure.snapshot();
  disposedFailure.fail();
  await settle();
  assert.deepEqual(disposedFailure.navigations, [],
    'a disposed Settings handler must not navigate after transport failure');
  assert.deepEqual(disposedFailure.snapshot(), beforeFailure,
    'delayed transport failure must not change disposed Settings state');

  if (input.error_response) {
    const disposedError = await pending();
    disposedError.controller.abort();
    const beforeError = disposedError.snapshot();
    disposedError.succeed(input.error_response);
    await settle();
    assert.deepEqual(disposedError.navigations, [],
      'a disposed Settings handler must not navigate after procedure rejection');
    assert.deepEqual(disposedError.snapshot(), beforeError,
      'delayed procedure rejection must not change disposed Settings state');
  }
  process.stdout.write(JSON.stringify({
    disposed_success: true,
    live_success: true,
    disposed_transport_failure: true,
    disposed_procedure_error: Boolean(input.error_response),
  }));
}

run().catch(error => {console.error(error); process.exitCode = 1;});
