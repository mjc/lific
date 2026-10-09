'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const flush = async () => {
  for (let attempt = 0; attempt < 80; attempt += 1) await Promise.resolve();
};
const event = type => ({type, target: {value: ''}, currentTarget: {}, preventDefault() {}, stopPropagation() {}});
const signals = runtime => Object.fromEntries(Object.keys(input.signals).map(id => [id, runtime.cx.signal(id).dehydrate()]));
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};

async function main() {
  let rejectedRequests = 0;
  const listeners = [];
  const runtime = handlerFixture(input.signals, async () => {
    rejectedRequests += 1;
    throw new Error('saved view service unavailable');
  }, input.browser_source);
  runtime.context.window.addEventListener = (name, handler) => listeners.push([name, handler]);
  runtime.context.document.getElementById = () => ({contains: () => false});
  runtime.context.sessionStorage = {getItem: () => null};
  runtime.handler(input.mount_handler)(runtime.cx.event(event('mount')));
  await flush();
  assert.equal(rejectedRequests, 1, 'the owner attempts to load its saved views');
  assert.equal(listeners.length, 2,
    'outside-click and Escape dismissal remain installed when the list request rejects');
  assert.deepEqual(listeners.map(([name]) => name).sort(), ['click', 'keydown']);

  const mutation = handlerFixture(input.signals, async () => {
    throw new Error('saved view mutation unavailable');
  }, input.browser_source);
  mutation.context.document.getElementById = () => ({focus() {}});
  mutation.context.sessionStorage = {getItem: () => null, setItem() {}, removeItem() {}};
  mutation.handler(input.create_handler)(mutation.cx.event(event('click')));
  mutation.handler(input.name_handler)(mutation.cx.event({...event('input'), target: {value: 'Daily'}}));
  mutation.handler(input.submit_handler)(mutation.cx.event(event('click')));
  await flush();
  assert.equal(unbox(mutation.cx.signal(input.busy_signal_id).dehydrate()), false,
    'rejected mutations release the form busy signal');
  const mutationStrings = Object.values(signals(mutation)).map(unbox).filter(value => typeof value === 'string');
  assert(mutationStrings.includes('The saved view could not be updated. Try again.'),
    'rejected mutations expose the inline error instead of leaving a busy form');

  let disposedRequests = 0;
  const disposedListeners = [];
  let rejectPending;
  const disposed = handlerFixture(input.signals, () => {
    disposedRequests += 1;
    return new Promise((_resolve, reject) => { rejectPending = reject; });
  }, input.browser_source);
  disposed.context.window.addEventListener = (name, handler, options = {}) => {
    const entry = {name, handler, active: true};
    disposedListeners.push(entry);
    options.signal?.addEventListener('abort', () => { entry.active = false; }, {once: true});
  };
  disposed.context.document.getElementById = () => ({contains: () => false});
  disposed.context.sessionStorage = {getItem: () => null};
  disposed.handler(input.mount_handler)(disposed.cx.event(event('mount')));
  await flush();
  assert.equal(typeof rejectPending, 'function', 'the owner started its asynchronous collection load');
  const beforeSettlement = JSON.stringify(signals(disposed));
  disposed.controller.abort();
  rejectPending(new Error('owner retired while loading'));
  await flush();
  assert.equal(disposedRequests, 1, 'the request started before its owner was retired');
  assert.equal(JSON.stringify(signals(disposed)), beforeSettlement,
    'a late rejection cannot update state owned by a disposed collection');
  assert.equal(disposedListeners.filter(entry => entry.active).length, 0,
    'owner disposal removes outside-click and Escape listeners');

  process.stdout.write(JSON.stringify({
    rejected_requests: rejectedRequests,
    listeners_after_rejection: listeners.length,
    disposed_requests: disposedRequests,
    disposed_listeners: disposedListeners.filter(entry => entry.active).length,
  }));
}

main().catch(error => {
  process.stderr.write(`${error.stack || error}\n`);
  process.exitCode = 1;
});
