'use strict';

const assert = require('node:assert/strict');
const {handlerFixture} = require('../handler_fixture.cjs');
const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const key = `lific:subtab:plans:${input.project_id}`;
const otherKey = `lific:subtab:plans:${input.project_id + 1}`;
const unwrap = value => {
  if (value && typeof value.dehydrate === 'function') value = value.dehydrate();
  while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};

function fixture(seed, {getFails = false, setFails = false, disposed = false} = {}) {
  const values = new Map(Object.entries(seed));
  const writes = [];
  const runtime = handlerFixture(input.signals,
    () => assert.fail('tab selection performs no service request'), input.browser_source);
  runtime.context.localStorage = {
    getItem(name) {
      if (getFails) throw new Error('storage read unavailable');
      return values.get(String(name)) ?? null;
    },
    setItem(name, value) {
      if (setFails) throw new Error('storage write unavailable');
      values.set(String(name), String(value));
      writes.push([String(name), String(value)]);
    },
  };
  if (disposed) runtime.controller.abort();
  return {runtime, values, writes};
}
function selected(tab, runtime) {
  const read = vmBinding(tab.selected);
  return unwrap(read(runtime.cx)) === true;
}
function vmBinding(source) {
  return require('node:vm').runInNewContext(`cx => (${source})`, inputContext);
}
let inputContext;
function selectedId(runtime) {
  inputContext = runtime.context;
  return input.tabs.find(tab => selected(tab, runtime))?.id ?? null;
}
function signalValues(runtime) {
  return Object.fromEntries(Object.keys(input.signals).map(id => [
    id,
    runtime.cx.signal(id).get().dehydrate(),
  ]));
}
function invoke(runtime, source, type) {
  runtime.handler(source)(runtime.cx.event({type}));
}

let signalSnapshot = input.signals;
const restored = fixture({[key]: 'done', [otherKey]: 'active'});
invoke(restored.runtime, input.mount_handler, 'mount');
assert.equal(selectedId(restored.runtime), 'done', 'mount restores the saved tab');
const archived = input.tabs.find(tab => tab.id === 'archived');
invoke(restored.runtime, archived.handler, 'click');
assert.equal(selectedId(restored.runtime), 'archived', 'click changes the selected tab');
assert.equal(restored.values.get(key), 'archived', 'click persists under the numeric project id');
assert.equal(restored.values.get(otherKey), 'active', 'persistence is isolated by project');
signalSnapshot = signalValues(restored.runtime);

const invalid = fixture({[key]: 'future-tab'});
invoke(invalid.runtime, input.mount_handler, 'mount');
assert.equal(selectedId(invalid.runtime), input.empty_active_fallback ? 'all' : 'active',
  'invalid storage applies the correct project-specific fallback');

const disposed = fixture({[key]: 'done'}, {disposed: true});
const beforeDisposed = selectedId(disposed.runtime);
invoke(disposed.runtime, archived.handler, 'click');
assert.equal(selectedId(disposed.runtime), beforeDisposed, 'retired owners ignore detached clicks');
assert.equal(disposed.writes.length, 0, 'retired owners do not persist clicks');

const asymmetric = fixture({[key]: 'done'}, {setFails: true});
invoke(asymmetric.runtime, input.mount_handler, 'mount');
invoke(asymmetric.runtime, archived.handler, 'click');
assert.equal(selectedId(asymmetric.runtime), 'archived', 'storage failure does not block live selection');
invoke(asymmetric.runtime, input.mount_handler, 'mount');
assert.equal(selectedId(asymmetric.runtime), 'archived',
  'a remounted shard does not reread stale storage after the live owner already restored');

let initialFallback = 'active';
if (input.empty_active_fallback) {
  const fallback = fixture({});
  invoke(fallback.runtime, input.mount_handler, 'mount');
  initialFallback = selectedId(fallback.runtime);
  assert.equal(initialFallback, 'all', 'no saved tab plus no active plans shows All');
  signalSnapshot = signalValues(fallback.runtime);
}

process.stdout.write(JSON.stringify({
  restored: 'done',
  selected_after_click: 'archived',
  stored_for_project: restored.values.get(key),
  other_project_value: restored.values.get(otherKey),
  invalid_storage_fallback: selectedId(invalid.runtime),
  disposed_writes: disposed.writes.length,
  initial_fallback: initialFallback,
  signals: signalSnapshot,
}));
