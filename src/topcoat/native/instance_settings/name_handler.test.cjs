'use strict';

// Execute the emitted input and blur handlers with replies from the real
// authenticated native save_text procedure.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const {fixtureRuntime} = require('../runtime_fixture.cjs');
const settle = async () => { for (let i = 0; i < 60; i++) await Promise.resolve(); };

function makeFixture(replies, delayedRequests = [], rejectedRequests = []) {
  const requests = [];
  const pending = new Map();
  const controller = new AbortController();
  let sessionChanges = 0;
  let invokeBlur;
  const context = {
    TextEncoder, TextDecoder, queueMicrotask,
    Event: class { constructor(type) { this.type = type; } },
    document: {
      documentElement: {getAttribute: () => ''},
      querySelector: selector => ({
        dataset: {topcoatUsizeBits: '64'},
        dispatchEvent: () => {
          if (selector === 'input[data-native-instance-name]') invokeBlur?.();
          return true;
        },
      }),
    },
    Element: class {}, window: {dispatchEvent() {}},
    fetch: (url, options) => {
      const path = new URL(url, 'http://localhost').pathname;
      assert.ok(
        path.endsWith('/__native_instance_settings/save_text') ||
        path.endsWith('/__native_instance_settings/confirm_name'),
        `unexpected route ${path}`,
      );
      assert.equal(options.method, 'POST');
      const body = JSON.parse(options.body);
      requests.push(body);
      const rejected = rejectedRequests.includes(requests.length);
      const reply = replies[requests.length - 1];
      if (delayedRequests.includes(requests.length)) {
        return new Promise((resolve, reject) => pending.set(requests.length, () => {
          if (rejected) {
            reject(new Error('simulated connection loss'));
          } else {
            resolve({ok: true, json: async () => reply});
          }
        }));
      }
      if (rejected) {
        return Promise.reject(new Error('simulated connection loss'));
      }
      assert.ok(reply, 'fixture has a serialized reply for each save');
      return Promise.resolve({ok: true, json: async () => reply});
    },
  };
  vm.runInNewContext(fixtureRuntime(['Context', 'Registry', 'Event']), context);
  const registry = new context.fixture.Registry();
  const cx = Object.assign(new context.fixture.Context(registry), {
    abortSignal: controller.signal,
    event: event => new context.fixture.Event(event),
    withSessionChange: (_owner, task) => { sessionChanges++; return task(); },
  });
  for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
  const handler = source => vm.runInNewContext(`cx => (${source})`, context)(cx);
  const inputHandler = handler(input.input_handler);
  const blurHandler = handler(input.blur_handler);
  const passwordHandler = handler(input.reauth_password_handler);
  const confirmHandler = handler(input.confirm_handler);
  const cancelHandler = handler(input.cancel_handler);
  const valueId = input.value_binding.match(/"id":"([^"]+)"/)?.[1];
  assert.ok(valueId, 'the emitted value binding identifies the draft signal');
  const confirmationId = input.confirmation_binding.match(/"id":"([^"]+)"/)?.[1];
  assert.ok(confirmationId, 'the emitted confirmation binding identifies its state signal');
  const unbox = value => {
    if (value !== null && typeof value === 'object') {
      if (Object.hasOwn(value, 'v')) return unbox(value.v);
      if (Object.hasOwn(value, 'n')) return unbox(value.n);
    }
    return value;
  };
  const fireInput = value => inputHandler(cx.event({type: 'input', target: {value}}));
  const firePassword = value => passwordHandler(cx.event({type: 'input', target: {value}}));
  const blur = () => blurHandler(cx.event({type: 'blur', target: {}, currentTarget: {}}));
  invokeBlur = blur;
  const current = () => {
    return unbox(cx.signal(valueId).dehydrate());
  };
  const needsConfirmation = () => unbox(cx.signal(confirmationId).get());
  return {
    requests, controller, fireInput, firePassword, blur, current, needsConfirmation,
    confirm: () => confirmHandler(cx.event({type: 'click'})),
    cancel: () => cancelHandler(cx.event({type: 'click'})),
    sessionChanges: () => sessionChanges,
    values: () => Object.keys(input.signals).map(id => {
      return unbox(cx.signal(id).dehydrate());
    }),
    release: requestNumber => {
      const release = pending.get(requestNumber);
      assert.ok(release, `request ${requestNumber} is pending`);
      pending.delete(requestNumber);
      release();
    },
  };
}

async function run() {
  const queued = makeFixture([input.save_reply, input.queued_reply], [1]);
  queued.fireInput('  New name  ');
  queued.blur();
  await settle();
  assert.equal(queued.requests.length, 1);
  assert.deepEqual(queued.requests[0], input.expected_save_args,
    'the delayed first request matches its real procedure reply');
  queued.fireInput('  Latest name  ');
  queued.blur();
  queued.fireInput('Unblurred C');
  await settle();
  assert.equal(queued.requests.length, 1, 'blur while the save is pending queues the latest value');
  queued.release(1);
  await settle();
  assert.equal(queued.requests.length, 2, 'the queued blur sends a second serialized mutation');
  assert.deepEqual(queued.requests[1], input.expected_queued_args,
    'the queued save sends the latest trimmed draft');
  assert.equal(queued.current(), 'Unblurred C',
    'saving the queued blur snapshot does not erase a newer unblurred draft');

  const queuedAfterTransportFailure = makeFixture(
    [null, input.queued_reply], [1], [1],
  );
  queuedAfterTransportFailure.fireInput('New name');
  queuedAfterTransportFailure.blur();
  await settle();
  queuedAfterTransportFailure.fireInput('Latest name');
  queuedAfterTransportFailure.blur();
  queuedAfterTransportFailure.release(1);
  await settle();
  assert.equal(queuedAfterTransportFailure.requests.length, 2,
    'a queued blur is still submitted after the earlier transport request rejects');
  assert.deepEqual(queuedAfterTransportFailure.requests[1], input.expected_queued_args,
    'transport failure does not strand the exact queued blur snapshot');

  const reverted = makeFixture([input.save_reply, input.revert_reply], [1]);
  reverted.fireInput('New name');
  reverted.blur();
  await settle();
  reverted.fireInput('Old name');
  reverted.blur();
  reverted.release(1);
  await settle();
  assert.equal(reverted.requests.length, 2,
    'a blur back to the old baseline is queued while the first save is in flight');
  assert.deepEqual(reverted.requests[1], input.expected_revert_args,
    'the queued revert is compared with the updated baseline after the first save');
  assert.equal(reverted.current(), 'Old name');

  const changed = makeFixture([input.save_reply, input.clear_reply]);
  changed.fireInput('  New name  ');
  changed.blur();
  changed.blur();
  await settle();
  assert.equal(changed.requests.length, 1, 'duplicate blur during save does not duplicate mutation');
  assert.deepEqual(changed.requests[0], input.expected_save_args,
    'blur sends the trimmed name through the actual native save_text procedure');
  assert.equal(changed.current(), 'New name');

  changed.fireInput('  New name  ');
  changed.blur();
  await settle();
  assert.equal(changed.requests.length, 1, 'normalized unchanged values are a no-op');
  assert.equal(changed.current(), 'New name', 'unchanged blur still normalizes the visible field');

  changed.fireInput('');
  changed.blur();
  await settle();
  assert.deepEqual(changed.requests[1], input.expected_clear_args,
    'blank is sent as the request to restore the host-name fallback');
  assert.equal(changed.current(), '');

  // A server refusal keeps the typed value visible for correction/retry.
  const disposed = makeFixture([input.save_reply]);
  disposed.controller.abort();
  disposed.fireInput('No request after unmount');
  disposed.blur();
  await settle();
  assert.equal(disposed.requests.length, 0, 'a disposed field owner sends no late mutation');

  const disposedPending = makeFixture([input.save_reply], [1]);
  disposedPending.fireInput('Pending draft');
  disposedPending.blur();
  await settle();
  assert.equal(disposedPending.requests.length, 1);
  disposedPending.controller.abort();
  const stateBeforeLateReply = disposedPending.values();
  disposedPending.release(1);
  await settle();
  assert.deepEqual(disposedPending.values(), stateBeforeLateReply,
    'a late response after owner disposal publishes no status or saved value');

  const failed = makeFixture([input.error_reply]);
  const rejectedValue = 'Draft survives';
  failed.fireInput(rejectedValue);
  failed.blur();
  await settle();
  assert.equal(failed.requests.length, 1);
  assert.equal(failed.current(), rejectedValue);
  assert.equal(failed.needsConfirmation(), true,
    'the exact recent-auth refusal opens the password confirmation controls');

  const recovery = makeFixture([input.error_reply, input.wrong_password_reply]);
  recovery.fireInput('Draft survives');
  recovery.blur();
  await settle();
  assert.equal(recovery.needsConfirmation(), true);
  recovery.fireInput('Coalesced name');
  recovery.blur();
  await settle();
  assert.equal(recovery.requests.length, 1,
    'blurred edits merge into the parked name without another stale save');
  recovery.firePassword('incorrect-password');
  recovery.confirm();
  await settle();
  assert.deepEqual(recovery.requests[1], input.expected_confirm_args,
    'confirmation submits the latest parked name with the entered password');
  assert.equal(recovery.needsConfirmation(), true,
    'an incorrect password leaves the confirmation open');
  assert.equal(recovery.current(), 'Coalesced name',
    'wrong-password recovery preserves the latest parked draft');
  assert.ok(recovery.values().includes('incorrect password'),
    'the wrong-password message is visible in the confirmation error');
  assert.equal(recovery.sessionChanges(), 2,
    'both the stale save and password confirmation use the runtime session-change bridge');
  recovery.cancel();
  assert.equal(recovery.needsConfirmation(), false);
  assert.equal(recovery.current(), input.name_signal_value,
    'Cancel restores the last saved canonical name');
  assert.ok(!recovery.values().includes('incorrect password'),
    'Cancel clears the confirmation error');

  const retry = makeFixture([
    input.error_reply, input.wrong_password_reply, input.confirm_success_reply,
  ]);
  retry.fireInput('Draft survives');
  retry.blur();
  await settle();
  retry.fireInput('Coalesced name');
  retry.blur();
  retry.firePassword('incorrect-password');
  retry.confirm();
  await settle();
  retry.firePassword('testpassword1');
  retry.confirm();
  await settle();
  assert.deepEqual(retry.requests[2], input.expected_confirm_success_args,
    'a corrected password retries the exact parked name');
  assert.equal(retry.needsConfirmation(), false,
    'successful confirmation closes the prompt');
  assert.equal(retry.current(), 'Coalesced name',
    'successful confirmation publishes the canonical server response');

  const confirming = makeFixture([
    input.error_reply, input.confirm_success_reply, input.confirm_queued_reply,
  ], [2]);
  confirming.fireInput('Draft survives');
  confirming.blur();
  await settle();
  confirming.fireInput('Coalesced name');
  confirming.blur();
  confirming.firePassword('testpassword1');
  confirming.confirm();
  await settle();
  assert.equal(confirming.requests.length, 2);
  confirming.fireInput('Queued after confirm');
  confirming.blur();
  confirming.fireInput('Unblurred after confirm');
  confirming.release(2);
  await settle();
  assert.deepEqual(confirming.requests[1], input.expected_confirm_success_args,
    'confirmation sends the parked snapshot that was displayed at submit time');
  assert.equal(confirming.requests.length, 3,
    'a blur during confirmation is saved after the confirmation response');
  assert.deepEqual(confirming.requests[2], input.expected_confirm_queued_args,
    'post-confirmation save uses the exact blurred snapshot');
  assert.equal(confirming.current(), 'Unblurred after confirm',
    'the post-confirm save does not overwrite a newer unblurred draft');

  const failedFollowup = makeFixture([
    input.error_reply, input.confirm_success_reply, input.ordinary_reply,
    input.followup_retry_reply,
  ], [2, 3]);
  failedFollowup.fireInput('Draft survives');
  failedFollowup.blur();
  await settle();
  failedFollowup.fireInput('Coalesced name');
  failedFollowup.blur();
  failedFollowup.firePassword('testpassword1');
  failedFollowup.confirm();
  await settle();
  failedFollowup.fireInput('Queued after confirm');
  failedFollowup.blur();
  failedFollowup.release(2);
  await settle();
  assert.equal(failedFollowup.requests.length, 3,
    'the blurred confirmation-time edit starts its own follow-up save');
  failedFollowup.fireInput('Newest after failed follow-up');
  failedFollowup.blur();
  failedFollowup.release(3);
  await settle();
  assert.equal(failedFollowup.current(), 'Newest after failed follow-up',
    'a failed replay does not replace a newer draft');
  assert.equal(
    failedFollowup.values().filter(value => value === 'Newest after failed follow-up').length,
    2,
    'a blur accumulated during a failed follow-up remains queued for a later attempt',
  );
  assert.equal(failedFollowup.requests.length, 3,
    'an ordinary replay refusal does not immediately flood the server with retries');
  failedFollowup.blur();
  await settle();
  assert.deepEqual(failedFollowup.requests[3], input.expected_followup_retry_args,
    'the next explicit blur retries the preserved draft');
  assert.equal(failedFollowup.current(), 'Newest after failed follow-up');

  const confirmationRetry = makeFixture(
    [input.error_reply, null, input.confirm_success_reply], [], [2],
  );
  confirmationRetry.fireInput('Draft survives');
  confirmationRetry.blur();
  await settle();
  confirmationRetry.fireInput('Coalesced name');
  confirmationRetry.blur();
  confirmationRetry.firePassword('testpassword1');
  confirmationRetry.confirm();
  await settle();
  assert.equal(confirmationRetry.current(), 'Coalesced name');
  assert.ok(confirmationRetry.values().includes('Couldn\'t confirm your password. Try again.'));
  confirmationRetry.confirm();
  await settle();
  assert.deepEqual(confirmationRetry.requests[2], input.expected_confirm_success_args,
    'a transport failure retries the exact submitted confirmation snapshot');
  assert.equal(confirmationRetry.needsConfirmation(), false);

  const disposedConfirmation = makeFixture([
    input.error_reply, input.confirm_success_reply,
  ], [2]);
  disposedConfirmation.fireInput('Draft survives');
  disposedConfirmation.blur();
  await settle();
  disposedConfirmation.firePassword('testpassword1');
  disposedConfirmation.confirm();
  await settle();
  assert.equal(disposedConfirmation.requests.length, 2);
  disposedConfirmation.controller.abort();
  const stateBeforeConfirmReply = disposedConfirmation.values();
  disposedConfirmation.release(2);
  await settle();
  assert.deepEqual(disposedConfirmation.values(), stateBeforeConfirmReply,
    'a late confirmation reply after disposal publishes no state');

  const ordinaryFailure = makeFixture([input.ordinary_reply]);
  ordinaryFailure.fireInput('Draft resets');
  ordinaryFailure.blur();
  await settle();
  assert.equal(ordinaryFailure.current(), input.name_signal_value,
    'ordinary refusals restore the authoritative saved value');
  assert.ok(ordinaryFailure.values().includes('only an admin can do this'),
    'ordinary refusals remain visible as an error');
  process.stdout.write(JSON.stringify({
    trimmed_save: true, unchanged_noop: true, blank_clears: true, queued_latest: true,
    queued_revert: true,
    disposed_no_request: true, disposed_pending_unchanged: true,
    draft_kept_on_error: true, ordinary_failure_restores: true,
    confirmation_opened: true, wrong_password_retains: true, cancel_restores: true,
    confirmation_retry: true, confirmation_queues_blur: true,
    confirmation_transport_retry: true, failed_followup_keeps_newer_blur: true,
    confirmation_disposal: true,
  }));
}
run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
