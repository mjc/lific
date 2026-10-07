'use strict';

// Execute the handlers emitted by the Settings document in the packaged runtime.
// Rust supplies the rendered attributes and serialized replies from the real
// authenticated procedures; this fixture only bridges those replies to fetch.
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

const settle = async () => { for (let i = 0; i < 50; i++) await Promise.resolve(); };
class Element {}

function assertDraftOnly(snapshot, baseline) {
  const changedText = Object.entries(snapshot)
    .filter(([id, value]) => typeof value === 'string' && value !== baseline[id])
    .map(([, value]) => value);
  assert.deepEqual(changedText, [input.edits.display_name],
    'post-check failure leaves the draft and does not publish an error string');
}

function fixture(delayed = false, responses = input.responses) {
  const controller = new AbortController();
  const requests = [];
  const pending = [];
  let now = 0;
  let timerId = 0;
  const timers = new Map();
  const context = {
    TextEncoder, TextDecoder, queueMicrotask,
    setTimeout(callback, delay = 0) {
      const id = ++timerId;
      timers.set(id, {callback, due: now + delay});
      return id;
    },
    clearTimeout(id) { timers.delete(id); },
    Event: class { constructor(type) { this.type = type; } },
    document: {documentElement: {getAttribute: () => input.prefix ?? ''}},
    history: {state: null, pushState(value) { this.state = value; }, replaceState(value) { this.state = value; }},
    Element,
    window: {dispatchEvent() {}},
    fetch(url, options) {
      const path = new URL(url, 'http://localhost').pathname;
      const kind = path.endsWith('/save_profile') ? 'save' :
        path.endsWith('/profile_session') ? 'session' : null;
      assert.ok(kind, `unexpected profile procedure ${path}`);
      assert.equal(options.method, 'POST');
      const request = {kind, path, body: JSON.parse(options.body)};
      requests.push(request);
      const response = responses[kind];
      assert.ok(response, `missing serialized ${kind} procedure reply`);
      if (delayed) return new Promise((resolve, reject) => pending.push({kind, resolve, reject, response}));
      return Promise.resolve({ok: true, json: async () => response});
    },
  };
  vm.runInNewContext(runtime.replace(bootstrap,
    `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass},Element};`), context);
  const registry = new context.fixture.Registry();
  const cx = Object.assign(new context.fixture.Context(registry), {
    abortSignal: controller.signal,
    event: event => new context.fixture.Event(event),
  });
  for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
  const snapshot = () => Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, cx.signal(id).dehydrate().v]));
  const baseline = snapshot();
  const handler = source => vm.runInNewContext(`cx => (${source})`, context)(cx);
  const fields = Object.fromEntries(Object.entries(input.fields).map(([key, value]) =>
    [key, handler(value.handler)]));
  const save = handler(input.save_handler);
  return {
    controller, requests, baseline, snapshot, context, cx,
    binding(source) { return vm.runInNewContext(`cx => (${source})`, context)(cx).dehydrate(); },
    async advance(ms) {
      now += ms;
      for (const [id, timer] of [...timers].sort((a, b) => a[1].due - b[1].due)) {
        if (timer.due > now) continue;
        timers.delete(id);
        timer.callback();
      }
      await settle();
    },
    release(kind, fail = false) {
      const request = pending.find(item => item.kind === kind);
      assert.ok(request, `no pending ${kind} response`);
      pending.splice(pending.indexOf(request), 1);
      if (fail) {
        request.reject(new Error('delayed profile transport failure'));
        return;
      }
      request.resolve({ok: true, json: async () => request.response});
    },
    input(field, value) {
      fields[field](cx.event({type: 'input', target: {value}}));
    },
    click() {
      save(cx.event({type: 'click', preventDefault() {}, stopPropagation() {}}));
    },
    changed() {
      const current = snapshot();
      return Object.fromEntries(Object.entries(current).filter(([id, value]) =>
        JSON.stringify(value) !== JSON.stringify(baseline[id])));
    },
  };
}

async function run() {
  if (input.mode === 'open_phone') {
    const phone = fixture();
    vm.runInNewContext(input.source.replace(/export const (\w+)=/g, 'globalThis.$1='), phone.context);
    phone.context.__lificNativeMounts = {
      [`${input.mount_url}#mobile-dispatch`]: phone.context.mobileDispatch,
    };
    const owner = {closest: selector => selector === '.native-home-shell' ? root : null};
    const root = new EventTarget();
    root.contains = node => node === action;
    phone.context.document.getElementById = id =>
      id === 'native-mobile-action-owner' ? owner : null;
    phone.context.document.querySelector = () => null;
    const mount = vm.runInNewContext(`cx => (${input.mount_handler})`, phone.context)(phone.cx);
    mount(phone.cx.event({type: 'mount'}));
    const action = new Element();
    action.getAttribute = name => name === 'data-native-mobile-action' ? input.action : null;
    action.closest = selector => selector === '[data-native-mobile-action]' ? action :
      selector === '.native-home-shell' ? root : null;
    const click = new Event('click');
    Object.defineProperty(click, 'target', {value: action});
    root.dispatchEvent(click);
    await settle();
    process.stdout.write(JSON.stringify({changed_signals: phone.changed()}));
    return;
  }

  if (input.mode === 'edit_only') {
    const draft = fixture();
    draft.input('display_name', input.edited_name);
    process.stdout.write(JSON.stringify({changed_signals: draft.changed()}));
    return;
  }

  if (input.mode === 'email_only') {
    const emailOnly = fixture();
    assert.equal(emailOnly.binding(input.disabled_handler), true);
    emailOnly.input('email', input.edits.email);
    assert.equal(emailOnly.binding(input.disabled_handler), false);
    emailOnly.click();
    await settle();
    assert.deepEqual(emailOnly.requests.map(request => request.kind), ['save', 'session']);
    assert.deepEqual(emailOnly.requests[0].body, input.expected_save_arguments,
      'email-only edits send only the trimmed email field');
    const changed = emailOnly.changed();
    const profileEntry = Object.entries(changed).find(([, wire]) =>
      wire?.t === 'Record' && typeof wire?.v?.username === 'string');
    assert.ok(profileEntry, 'email-only success publishes its canonical profile');
    assert.equal(profileEntry[1].v.email, input.canonical.email);
    process.stdout.write(JSON.stringify({email_only_sparse: true}));
    return;
  }

  const unchanged = fixture();
  assert.equal(unchanged.binding(input.disabled_handler), true,
    'the rendered disabled binding starts disabled for an unchanged profile');
  unchanged.input('display_name', `  ${input.initial_fields.display_name}  `);
  unchanged.input('email', `  ${input.initial_fields.email.toUpperCase()}  `);
  assert.equal(unchanged.binding(input.disabled_handler), true,
    'trimming and email case normalization preserve a no-op state');
  unchanged.click();
  await settle();
  assert.equal(unchanged.requests.length, 0,
    'normalized no-op edits do not issue a profile mutation');

  const nextLineEmail = fixture();
  nextLineEmail.input('email', `\u0085${input.initial_fields.email}\u0085`);
  assert.equal(nextLineEmail.binding(input.disabled_handler), false,
    'Main does not trim NEXT LINE as ECMAScript whitespace');
  nextLineEmail.click();
  await settle();
  assert.equal(nextLineEmail.requests[0]?.kind, 'save',
    'the enabled email edit and emitted save handler agree about whitespace');

  const javascriptWhitespace = fixture();
  javascriptWhitespace.input('display_name', `\uFEFF${input.initial_fields.display_name}\uFEFF`);
  assert.equal(javascriptWhitespace.binding(input.disabled_handler), true,
    'Main treats ECMAScript BOM whitespace around a name as a no-op');
  javascriptWhitespace.click();
  await settle();
  assert.equal(javascriptWhitespace.requests.length, 0);

  const disposedBeforeMutation = fixture();
  disposedBeforeMutation.input('display_name', input.edits.display_name);
  disposedBeforeMutation.click();
  disposedBeforeMutation.controller.abort();
  await settle();
  assert.equal(disposedBeforeMutation.requests.length, 0,
    'an owner disposed before the queued mutation starts sends no request');

  const current = fixture();
  current.input('display_name', input.edits.display_name);
  current.input('email', input.edits.email);
  assert.equal(current.binding(input.disabled_handler), false,
    'the rendered disabled binding enables a normalized dirty profile');
  current.click();
  // A duplicate click while the first mutation is in flight must be ignored.
  current.click();
  await settle();

  assert.deepEqual(current.requests.map(request => request.kind), ['save', 'session'],
    'one profile mutation is followed by one fresh session check');
  assert.deepEqual(current.requests[0].body, input.expected_save_arguments,
    'the rendered handler sends only changed normalized fields and preserves integer wire values');
  assert.deepEqual(current.requests[1].body, input.expected_session_arguments,
    'fresh session check is scoped to the captured account');
  assert.equal(current.binding(input.disabled_handler), true,
    'canonical publication resets both fields and disables Save again');

  const changed = current.changed();
  const profileEntry = Object.entries(changed).find(([, wire]) =>
    wire?.t === 'Record' && typeof wire?.v?.username === 'string');
  assert.ok(profileEntry, 'successful mutation publishes the canonical Profile signal');
  const profile = profileEntry[1].v;
  assert.equal(profile.display_name, input.canonical.display_name);
  assert.equal(profile.email, input.canonical.email);

  // The first success timer must not clear feedback from a newer save.
  await current.advance(1000);
  current.input('display_name', 'Second Viewer');
  current.click();
  await settle();
  assert.equal(current.requests.length, 4, 'a later edit can be saved after completion');
  await current.advance(1000);
  const afterOldTimer = current.snapshot();
  assert.ok(Object.entries(afterOldTimer).some(([id, value]) =>
    input.signals[id] === false && value === true),
  'the earlier Saved timer cannot clear feedback from the newer save');
  await current.advance(1000);
  assert.equal(Object.entries(current.snapshot()).some(([id, value]) =>
    input.signals[id] === false && value === true), false,
  'the current Saved timer clears its own feedback after two seconds');

  const disposed = fixture(true);
  disposed.input('display_name', input.edits.display_name);
  assert.equal(disposed.binding(input.disabled_handler), false);
  disposed.click();
  await settle();
  assert.equal(disposed.binding(input.disabled_handler), true,
    'the rendered disabled binding enters its busy state');
  assert.equal(disposed.requests[0]?.kind, 'save', 'the delayed mutation has started');
  const profileBeforeDispose = Object.entries(disposed.snapshot()).find(([, wire]) =>
    wire?.t === 'Record' && typeof wire?.v?.username === 'string');
  assert.ok(profileBeforeDispose, 'shared profile signal exists before delayed completion');
  disposed.controller.abort();
  const disposedSnapshot = disposed.snapshot();
  disposed.release('save');
  await settle();
  assert.deepEqual(disposed.snapshot(), disposedSnapshot,
    'a disposed owner cannot write any signal after delayed success');
  assert.equal(disposed.requests.some(request => request.kind === 'session'), false,
    'a disposed owner does not start a fresh session check');

  const disposedCheck = fixture(true);
  disposedCheck.input('display_name', input.edits.display_name);
  disposedCheck.click();
  await settle();
  disposedCheck.release('save');
  await settle();
  assert.equal(disposedCheck.requests[1]?.kind, 'session',
    'successful mutation reaches its delayed fresh identity check');
  const profileBeforeCheck = Object.entries(disposedCheck.snapshot()).find(([, wire]) =>
    wire?.t === 'Record' && typeof wire?.v?.username === 'string');
  assert.ok(profileBeforeCheck, 'profile signal exists while fresh check is pending');
  disposedCheck.controller.abort();
  const disposedCheckSnapshot = disposedCheck.snapshot();
  disposedCheck.release('session');
  await settle();
  assert.deepEqual(disposedCheck.snapshot(), disposedCheckSnapshot,
    'disposing during the post-write session check prevents all late writes');

  const disposedFailure = fixture(true);
  disposedFailure.input('display_name', input.edits.display_name);
  disposedFailure.click();
  await settle();
  const profileBeforeFailure = Object.entries(disposedFailure.snapshot()).find(([, wire]) =>
    wire?.t === 'Record' && typeof wire?.v?.username === 'string');
  assert.ok(profileBeforeFailure, 'profile signal exists before delayed error');
  disposedFailure.controller.abort();
  const disposedFailureSnapshot = disposedFailure.snapshot();
  disposedFailure.release('save', true);
  await settle();
  assert.deepEqual(disposedFailure.snapshot(), disposedFailureSnapshot,
    'a late transport error from a disposed owner cannot write any signal');

  if (input.mismatch_responses) {
    const mismatch = fixture(false, input.mismatch_responses);
    mismatch.input('display_name', input.edits.display_name);
    mismatch.click();
    await settle();
    assert.deepEqual(mismatch.requests.map(request => request.kind), ['save', 'session'],
      'a successful write still performs the post-write identity check');
    const beforeMismatch = Object.entries(mismatch.baseline).find(([, wire]) =>
      wire?.t === 'Record' && typeof wire?.v?.username === 'string');
    assert.ok(beforeMismatch, 'profile signal exists before the post-write check');
    assert.deepEqual(mismatch.snapshot()[beforeMismatch[0]], beforeMismatch[1],
      'a changed session identity prevents stale canonical profile publication');
    assertDraftOnly(mismatch.snapshot(), mismatch.baseline);
  }

  if (input.failure_responses) {
    const failure = fixture(false, input.failure_responses);
    failure.input('display_name', input.edits.display_name);
    failure.click();
    await settle();
    assert.deepEqual(failure.requests.map(request => request.kind), ['save', 'session'],
      'even an authorized inner error is published only after the fresh identity check');
    const beforeFailure = Object.entries(failure.baseline).find(([, wire]) =>
      wire?.t === 'Record' && typeof wire?.v?.username === 'string');
    assert.ok(beforeFailure, 'profile signal exists before rejected mutation');
    assert.deepEqual(failure.snapshot()[beforeFailure[0]], beforeFailure[1],
      'an inner procedure error under a replacement session does not publish profile state');
    assertDraftOnly(failure.snapshot(), failure.baseline);
  }

  const checkTransportFailure = fixture(true);
  checkTransportFailure.input('display_name', input.edits.display_name);
  checkTransportFailure.click();
  await settle();
  const pendingSignals = checkTransportFailure.snapshot();
  const savingId = Object.keys(pendingSignals).find(id =>
    checkTransportFailure.baseline[id] === false && pendingSignals[id] === true);
  assert.ok(savingId, 'profile mutation enters its busy state');
  checkTransportFailure.release('save');
  await settle();
  assert.equal(checkTransportFailure.requests[1]?.kind, 'session',
    'successful write reaches its fresh session check');
  checkTransportFailure.release('session', true);
  await settle();
  assert.deepEqual(checkTransportFailure.requests.map(request => request.kind), ['save', 'session']);
  assert.equal(checkTransportFailure.snapshot()[savingId], false,
    'fresh-check transport failure clears the live form busy state');
  const beforeCheckFailure = Object.entries(checkTransportFailure.baseline).find(([, wire]) =>
    wire?.t === 'Record' && typeof wire?.v?.username === 'string');
  assert.ok(beforeCheckFailure, 'profile signal exists before failed fresh check');
  assert.deepEqual(checkTransportFailure.snapshot()[beforeCheckFailure[0]], beforeCheckFailure[1],
    'a fresh-check transport failure cannot publish a profile');
  assertDraftOnly(checkTransportFailure.snapshot(), checkTransportFailure.baseline);

  const serializedChanged = current.changed();
  process.stdout.write(JSON.stringify({
    unchanged_no_request: true,
    disposed_before_mutation: true,
    disposed_success: true,
    disposed_during_check: true,
    fresh_check_transport_failure: true,
    disposed_late_error: true,
    session_mismatch_rejected: Boolean(input.mismatch_responses),
    procedure_error_rejected: Boolean(input.failure_responses),
    changed_signals: serializedChanged,
    requests: current.requests,
  }));
}

run().catch(error => { console.error(error); process.exitCode = 1; });
