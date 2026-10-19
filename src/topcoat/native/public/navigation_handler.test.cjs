'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const publicKey = `lific:public:list:layout:${input.project}`;
const privateKey = `lific:list:layout:${input.project}`;
const otherProjectKey = 'lific:public:list:layout:OTHER';
const run = ({saved, denied = false, disposed = false}) => {
  const runtime = handlerFixture(input.signals, async () => {}, input.browser_source);
  const storage = new Map([
    [publicKey, saved],
    [privateKey, 'board'],
    [otherProjectKey, 'board'],
  ]);
  const readKeys = [];
  runtime.context.localStorage = {
    getItem: key => {
      readKeys.push(key);
      if (denied) throw new Error('storage denied');
      return storage.get(key) ?? null;
    },
    setItem: (key, value) => storage.set(key, String(value)),
    removeItem: key => storage.delete(key),
  };
  const read = id => {
    let value = runtime.cx.signal(id).dehydrate();
    while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
    return value;
  };
  if (disposed) runtime.controller.abort();
  runtime.handler(input.mount_handler)(runtime.cx.event({type: 'mount', target: {}, currentTarget: {}}));
  const result = {
    href: read(input.href_signal),
    label: read(input.label_signal),
    private_key_untouched: storage.get(privateKey) === 'board',
    other_project_ignored: storage.get(otherProjectKey) === 'board',
    private_key_unread: !readKeys.includes(privateKey),
    other_project_unread: !readKeys.includes(otherProjectKey),
    public_key_read: readKeys.includes(publicKey),
  };
  runtime.controller.abort();
  return result;
};

assert.ok(input.link_kind, 'the Back link remains a generated native navigation link');
const board = run({saved: 'board'});
assert.equal(board.href, `${input.mount}/public/${input.project}/board`);
assert.equal(board.label, 'Back to board');
const invalid = run({saved: 'kanban'});
const denied = run({saved: 'board', denied: true});
const disposed = run({saved: 'board', disposed: true});
assert.equal(invalid.href, `${input.mount}/public/${input.project}/issues`);
assert.equal(invalid.label, 'Back to issues');
assert.equal(denied.href, `${input.mount}/public/${input.project}/issues`);
assert.equal(disposed.href, `${input.mount}/public/${input.project}/issues`);
assert.ok([board, invalid, denied, disposed].every(item => item.private_key_untouched));
assert.ok([board, invalid, denied, disposed].every(item => item.other_project_ignored));
assert.ok([board, invalid, denied, disposed].every(item => item.private_key_unread));
assert.ok([board, invalid, denied, disposed].every(item => item.other_project_unread));
assert.ok([board, invalid, denied].every(item => item.public_key_read));
process.stdout.write(JSON.stringify({board, invalid, denied, disposed}));
