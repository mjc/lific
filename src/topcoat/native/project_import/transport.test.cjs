'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const {fixtureRuntime} = require('../runtime_fixture.cjs');

const listeners = {};
const documentListeners = {};
const xhrs = [];
const storage = new Map();
let owner;
const selectedFileInput = {value: ''};
class FakeFile {
  constructor(name, size) { this.name = name; this.size = size; }
}
class FakeFormData {
  constructor() { this.values = new Map([['archive', new FakeFile('team.tar.gz', 12)], ['confirm', 'on']]); }
  keys() { return this.values.keys(); }
  get(key) { return this.values.get(key); }
  delete(key) { this.values.delete(key); }
}
class FakeXhr {
  constructor() { this.upload = {addEventListener: (name, fn) => { (this.uploadListeners ??= {})[name] = fn; }}; this.listeners = {}; this.headers = {}; xhrs.push(this); }
  open(method, url) { this.method = method; this.url = url; }
  setRequestHeader(name, value) { this.headers[name] = value; }
  addEventListener(name, fn) { this.listeners[name] = fn; }
  send(data) { this.data = data; }
}
const context = {
  TextEncoder, TextDecoder, queueMicrotask, File: FakeFile, FormData: FakeFormData,
  XMLHttpRequest: FakeXhr,
  fetch: async (url, options) => {
    assert.equal(new URL(url, 'http://localhost').pathname, '/app/__native_project_import/current_session');
    assert.equal(options.method, 'POST');
    return {ok: true, json: async () => input.session_reply};
  },
  sessionStorage: {getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value), removeItem: key => storage.delete(key)},
  document: {
    documentElement: {getAttribute: () => '/app'},
    getElementById: () => owner,
    querySelector: selector => selector === 'input[data-native-project-import-file]' ? selectedFileInput : null,
    addEventListener: (name, fn) => { documentListeners[name] = fn; },
  },
};
vm.runInNewContext(fixtureRuntime(['Context', 'Registry', 'Event']), context);
function makeCx(controller, registry) {
  return Object.assign(new context.fixture.Context(registry), {
    abortSignal: controller.signal,
    event: event => new context.fixture.Event(event),
  });
}
const emptyArchiveHandles = (fileName = '') => [
  fileName, '', false, 'idle', {t: 'Option', v: null}, '', {t: 'Option', v: null},
];
const setSignal = (signal, value) => signal.set(context.cx.hydrate(value));
const resetRegistry = new context.fixture.Registry();
context.cx = makeCx(new AbortController(), resetRegistry);
for (const [id, value] of Object.entries(input.reset_signals)) {
  resetRegistry.insert(id, context.cx.hydrate(value));
}
selectedFileInput.value = 'team.tar.gz';
const resetHandler = vm.runInNewContext(`cx => (${input.reset_handler})`, context)(context.cx);
resetHandler(context.cx.event({type: 'click', inner: {}}));
assert.equal(selectedFileInput.value, '', 'reset clears the browser file selection before a same-file retry');

const selectionRegistry = new context.fixture.Registry();
context.cx = makeCx(new AbortController(), selectionRegistry);
for (const [id, value] of Object.entries(input.page_signals)) {
  selectionRegistry.insert(id, context.cx.hydrate(value));
}
const selectionHandler = vm.runInNewContext(`cx => (${input.file_change_handler})`, context)(context.cx);
const unbox = value => {
  if (value !== null && typeof value === 'object') {
    if (value.t === 'usize' || value.t === 'i64') return Number(value.v);
    if (value.t === 'Vec') return Array.from(value.v, unbox);
    if (value.t === 'Record') {
      return Object.fromEntries(Object.entries(value.v).map(([key, nested]) => [key, unbox(nested)]));
    }
    if (Object.hasOwn(value, 'v')) return unbox(value.v);
    if (Object.hasOwn(value, 'n')) return unbox(value.n);
  }
  return value;
};
const read = signal => unbox(signal.dehydrate());
const pageValues = () => Object.keys(input.page_signals).map(id =>
  read(context.cx.signal(id)));
const selectFile = (name, size) => selectionHandler(context.cx.event({
  type: 'change',
  target: {files: [new FakeFile(name, size)]},
}));
selectFile('workspace.TAR.GZ', 1);
assert.ok(pageValues().includes('workspace.TAR.GZ'), 'the emitted handler retains uppercase archive names');
assert.ok(pageValues().includes(1));
assert.ok(!pageValues().includes('Choose a Lific project archive ending in .tar.gz.'));
selectFile('workspace.tar.gz', input.max_upload_bytes);
assert.ok(pageValues().includes(input.max_upload_bytes), 'the exact compressed limit is accepted');
assert.ok(!pageValues().includes('This archive exceeds the 128 MiB web upload limit. Use the CLI for larger archives.'));
selectFile('workspace.zip', 1);
assert.ok(pageValues().includes('Choose a Lific project archive ending in .tar.gz.'));
selectFile('workspace.tar.gz', 0);
assert.ok(pageValues().includes('This file is empty. Choose a Lific project archive.'), JSON.stringify(pageValues()));
selectFile('workspace.tar.gz', input.max_upload_bytes + 1);
assert.ok(pageValues().includes('This archive exceeds the 128 MiB web upload limit. Use the CLI for larger archives.'));

const firstController = new AbortController();
let registry = new context.fixture.Registry();
context.cx = makeCx(firstController, registry);
for (let index = 0; index < 7; index++) {
  registry.insert(`archive-${index}`, context.cx.hydrate(emptyArchiveHandles('team.tar.gz')[index]));
}
const [fileName, fileError, confirmed, phase, progress, error, result] =
  Array.from({length: 7}, (_, index) => context.cx.signal(`archive-${index}`));
const account = context.cx.hydrate(input.account);
const accountKey = String(input.account_value);
const fingerprint = context.cx.hydrate(input.session_reply).fingerprint.toString();
const handler = vm.runInNewContext(`cx => (${input.factory})`, context)(context.cx);
const ownerKey = `${accountKey}:${fingerprint}`;
owner = {dataset: {nativeArchiveOwner: ownerKey}};
const makeMountEvent = () => context.cx.event({type: 'mount', inner: {}});
handler(makeMountEvent(), [fileName, fileError, confirmed, phase, progress, error, result],
  account, context.cx.hydrate(fingerprint), context.cx.hydrate(`/app/__native_project_import/upload/${accountKey}`));
setSignal(confirmed, true);
const form = {closest: () => form, querySelector: () => ({files: [new FakeFile('team.tar.gz', 12)]})};
const submit = documentListeners.submit;
assert.ok(submit, 'the emitted mount handler installs submit handling');
const submitEvent = {target: form, preventDefault() { this.prevented = true; }};
submit(submitEvent);
assert.equal(xhrs.length, 1);
assert.equal(xhrs[0].method, 'POST');
assert.equal(xhrs[0].headers['X-Lific-Import-Session'], fingerprint);
assert.equal([...xhrs[0].data.keys()].join(','), 'archive', 'transport sends only the archive field');
assert.equal(read(phase), 'uploading');

xhrs[0].uploadListeners.progress({lengthComputable: true, loaded: 49, total: 100});
const nextOwner = {dataset: {nativeArchiveOwner: ownerKey}};
documentListeners['topcoat:before-page-replace']({detail: {nextDocument: {
  getElementById: () => nextOwner,
  querySelector: () => ({})
}}});
firstController.abort();
owner = nextOwner;
const nextController = new AbortController();
const oldSignals = [fileName, fileError, confirmed, phase, progress, error, result];
registry = new context.fixture.Registry();
context.cx = makeCx(nextController, registry);
for (let index = 0; index < 7; index++) {
  registry.insert(`archive-${index}`, context.cx.hydrate(emptyArchiveHandles()[index]));
}
const nextSignals = Array.from({length: 7}, (_, index) => context.cx.signal(`archive-${index}`));
const remounted = vm.runInNewContext(`cx => (${input.factory})`, context)(context.cx);
remounted(makeMountEvent(), nextSignals, account, context.cx.hydrate(fingerprint),
  context.cx.hydrate(`/app/__native_project_import/upload/${accountKey}`));
assert.equal(read(nextSignals[4]), 49, 'the new owner replays the latest transfer progress');
assert.equal(read(nextSignals[3]), 'uploading', 'the new owner restores the in-flight phase');
assert.equal(read(nextSignals[5]), '', 'the new owner clears the reload warning when restoring the active upload');

xhrs[0].status = 201;
xhrs[0].responseText = 'malformed success response';
xhrs[0].listeners.load();
setImmediate(async () => {
  assert.equal(read(nextSignals[3]), 'unknown', 'the new owner receives the terminal response');
  assert.equal(read(nextSignals[5]), 'The import outcome could not be checked. Check the project list before importing again.');
  assert.equal(storage.get(`lific:native-archive-import-pending:${accountKey}`), '1',
    'malformed success keeps the reload fence until acknowledgement');
  assert.equal(read(oldSignals[3]), 'uploading', 'terminal response does not update disposed signals');

  const resetController = new AbortController();
  registry = new context.fixture.Registry();
  context.cx = makeCx(resetController, registry);
  for (const [signalId, value] of Object.entries(input.reset_signals)) {
    registry.insert(signalId, context.cx.hydrate(value));
  }
  const resetButton = vm.runInNewContext(`cx => (${input.reset_handler})`, context)(context.cx);
  selectedFileInput.value = 'team.tar.gz';
  resetButton(context.cx.event({type: 'click', inner: {}}));
  assert.equal(storage.has(`lific:native-archive-import-pending:${accountKey}`), false);
  assert.equal(context.document.nativeArchiveImportTransfer.terminal, null);
  assert.equal(selectedFileInput.value, '', 'reset clears the browser file selection so the same archive can be picked again');

  const finalOwner = {dataset: {nativeArchiveOwner: ownerKey}};
  documentListeners['topcoat:before-page-replace']({detail: {nextDocument: {
    getElementById: () => finalOwner,
    querySelector: () => ({})
  }}});
  resetController.abort();
  owner = finalOwner;
  registry = new context.fixture.Registry();
  const finalController = new AbortController();
  context.cx = makeCx(finalController, registry);
  for (let index = 0; index < 7; index++) {
    registry.insert(`archive-${index}`, context.cx.hydrate(emptyArchiveHandles()[index]));
  }
  const finalSignals = Array.from({length: 7}, (_, index) => context.cx.signal(`archive-${index}`));
  const finalHandler = vm.runInNewContext(`cx => (${input.factory})`, context)(context.cx);
  finalHandler(makeMountEvent(), finalSignals, account, context.cx.hydrate(fingerprint),
    context.cx.hydrate(`/app/__native_project_import/upload/${accountKey}`));
  assert.equal(read(finalSignals[3]), 'idle');
  assert.equal(read(finalSignals[5]), '');

  setSignal(finalSignals[0], 'team.tar.gz');
  setSignal(finalSignals[1], '');
  setSignal(finalSignals[2], true);
  const successXhrIndex = xhrs.length;
  const successForm = {closest: () => successForm, querySelector: () => ({files: [new FakeFile('team.tar.gz', 12)]})};
  documentListeners.submit({target: successForm, preventDefault() {}});
  assert.equal(xhrs.length, successXhrIndex + 1);
  const successXhr = xhrs[successXhrIndex];
  successXhr.status = 201;
  successXhr.responseText = JSON.stringify(input.success_reply);
  successXhr.listeners.load();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(read(finalSignals[3]), 'success', 'typed HTTP 201 data produces a successful import state');
  const imported = read(finalSignals[6]);
  assert.equal(imported.project.identifier, 'ARCIM');
  assert.equal(imported.project.is_public, false);
  assert.deepEqual(imported.report.rows, [{table: 'issues', count: 3}]);
  assert.equal(imported.report.blobs, 2);
  assert.deepEqual(imported.report.external_references, ['archived-author']);
  assert.equal(imported.report.external_reference_count, 1);
  assert.equal(storage.has(`lific:native-archive-import-pending:${accountKey}`), false);

  resetHandler(context.cx.event({type: 'click', inner: {}}));
  setSignal(finalSignals[3], 'idle');
  setSignal(finalSignals[5], '');
  setSignal(finalSignals[0], '');
  setSignal(finalSignals[1], '');
  setSignal(finalSignals[2], false);
  const gapSignals = finalSignals;
  setSignal(gapSignals[2], true);
  setSignal(gapSignals[0], 'team.tar.gz');
  setSignal(gapSignals[1], '');
  const xhrIndex = xhrs.length;
  const gapForm = {closest: () => gapForm, querySelector: () => ({files: [new FakeFile('team.tar.gz', 12)]})};
  documentListeners.submit({target: gapForm, preventDefault() {}});
  assert.equal(xhrs.length, xhrIndex + 1, 'the no-owner scenario starts only one upload');
  const gapXhr = xhrs[xhrIndex];
  documentListeners['topcoat:before-page-replace']({detail: {nextDocument: {
    getElementById: () => null,
    querySelector: () => null,
  }}});
  finalController.abort();
  const disposedGapSignals = gapSignals.slice();
  gapXhr.status = 409;
  gapXhr.responseText = 'This project identifier is already in use.';
  gapXhr.listeners.load();
  setImmediate(() => {
    const transfer = context.document.nativeArchiveImportTransfer;
    assert.equal(transfer.terminal.status, 409, 'a terminal reply survives while the route has no import owner');
    assert.equal(read(disposedGapSignals[3]), 'uploading', 'the no-owner completion never writes disposed signals');
    assert.equal(storage.get(`lific:native-archive-import-pending:${accountKey}`), '1');

    const returnedOwner = {dataset: {nativeArchiveOwner: ownerKey}};
    owner = returnedOwner;
    registry = new context.fixture.Registry();
    context.cx = makeCx(new AbortController(), registry);
    for (let index = 0; index < 7; index++) {
      registry.insert(`archive-${index}`, context.cx.hydrate(emptyArchiveHandles()[index]));
    }
    const returnedSignals = Array.from({length: 7}, (_, index) => context.cx.signal(`archive-${index}`));
    const returnedHandler = vm.runInNewContext(`cx => (${input.factory})`, context)(context.cx);
    returnedHandler(makeMountEvent(), returnedSignals, account, context.cx.hydrate(fingerprint),
      context.cx.hydrate(`/app/__native_project_import/upload/${accountKey}`));
    setImmediate(() => {
      assert.equal(read(returnedSignals[3]), 'error', 'the matching session receives a retained definite response');
      assert.equal(read(returnedSignals[5]), 'This project identifier is already in use.');
      assert.equal(xhrs.length, xhrIndex + 1, 'returning to the route does not start a duplicate upload');
      assert.equal(read(disposedGapSignals[3]), 'uploading');
      process.stdout.write(JSON.stringify({
        upload_started: true,
        progress_retained_after_handoff: true,
        late_terminal_uses_new_owner: true,
        reset_clears_transfer_terminal: true,
        reset_owner_does_not_replay_old_terminal: true,
        selection_validation_uses_production_handler: true,
        absent_owner_gap_retains_one_terminal: true,
        typed_201_result_hydrates_successfully: true,
      }));
    });
  });
});
