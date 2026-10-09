'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
if (Array.isArray(input.stages)) {
  const storage = new Map([
    [`lific:public:list:state:${input.project}`, input.initial_value],
  ]);
  const restoredQueries = [];
  input.stages.forEach((stage, index) => {
    const runtime = handlerFixture(stage.signals, async () => {}, input.browser_source);
    runtime.context.localStorage = {
      getItem: key => storage.get(key) ?? null,
      setItem: (key, value) => storage.set(key, String(value)),
      removeItem: key => storage.delete(key),
    };
    const signal = runtime.cx.signal(stage.signal_id);
    const dispatch = (handler, type, value = '') => runtime.handler(handler)(
      runtime.cx.event({type, target: {value}, currentTarget: {}}),
    );
    dispatch(stage.mount, 'mount');
    const unbox = value => {
      while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
      return value;
    };
    restoredQueries.push(unbox(signal.dehydrate()));
    if (index === 0) dispatch(stage.query, 'input', 'changed in list');
    if (index === 1) dispatch(stage.query, 'input', 'changed in board');
    runtime.controller.abort();
  });
  process.stdout.write(JSON.stringify({restored_queries: restoredQueries}));
} else {
const runtime = handlerFixture(input.signals, async () => {}, input.browser_source);
const publicKey = input.kind === 'pages'
  ? `lific:public:subtab:pages:${input.project_id}`
  : `lific:public:list:state:${input.project}`;
const privateKey = input.kind === 'pages'
  ? `lific:subtab:pages:${input.project_id}`
  : `lific:list:state:${input.project}`;
const storage = new Map([
  [publicKey, input.initial_value ?? (input.kind === 'pages'
    ? 'browse'
    : JSON.stringify({searchQuery: 'saved public search'}))],
  [privateKey, 'private preference sentinel'],
]);
runtime.context.localStorage = {
  getItem: key => {
    if (input.storage_denied) throw new Error('storage denied');
    return storage.get(key) ?? null;
  },
  setItem: (key, value) => {
    if (input.storage_denied) throw new Error('storage denied');
    storage.set(key, String(value));
  },
  removeItem: key => storage.delete(key),
};
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const read = name => unbox(runtime.cx.signal(input.signal_ids[name]).dehydrate());
const dispatch = (name, type, value = '') => runtime.handler(input.handlers[name])(
  runtime.cx.event({type, target: {value}, currentTarget: {}}),
);

if (input.dispose_before_mount) runtime.controller.abort();
if (input.handlers.mount) dispatch('mount', 'mount');
const restoredQuery = input.kind === 'pages' ? undefined : read('query');
const restoredTab = input.kind === 'pages' ? read('tab') : undefined;
if (input.handlers.query) dispatch('query', 'input', 'new public search');
for (const [name, value] of Object.entries({
  status: 'active', priority: 'urgent', label: 'Roadmap', module: 'Core',
  sort: 'updated', direction: 'desc', group: 'module',
})) {
  if (input.handlers[name]) dispatch(name, 'change', value);
}
if (input.handlers.tab) dispatch('tab', 'click');
if (input.handlers.recent) dispatch('recent', 'click');
if (input.handlers.archived) dispatch('archived', 'click');
const savedQuery = input.kind === 'pages'
  || input.storage_denied
  ? undefined
  : JSON.parse(storage.get(publicKey)).searchQuery;
const savedFields = input.kind === 'pages' || input.storage_denied
  ? undefined
  : JSON.parse(storage.get(publicKey));
assert.equal(storage.get(privateKey), 'private preference sentinel');
process.stdout.write(JSON.stringify({
  restored_query: restoredQuery,
  saved_query: savedQuery,
  saved_fields: savedFields,
  saved_tab: input.kind === 'pages' ? read('tab') : undefined,
  restored_tab: restoredTab,
  persisted_tab: input.kind === 'pages' ? storage.get(publicKey) : undefined,
  private_key_untouched: storage.get(privateKey) === 'private preference sentinel',
  disposed: runtime.controller.signal.aborted,
}));
}
