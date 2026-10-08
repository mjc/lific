'use strict';

// Replay emitted folder tree controls through the packaged Topcoat runtime.
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
let finishRequest;
const fixture = handlerFixture(input.signals, async (url, options) => {
  const path = new URL(url, 'http://localhost').pathname;
  if (!path.endsWith('/__native_pages/delete-folder')) {
    throw new Error(`unexpected folder tree procedure ${path}`);
  }
  requests.push({path, arguments: JSON.parse(options.body)});
  if (input.scenario === 'pending') {
    return new Promise(resolve => {
      finishRequest = () => resolve({ok: true, json: async () => input.reply});
    });
  }
  return {ok: true, json: async () => input.reply};
}, input.browser_source);
const {cx, context} = fixture;
context.document.documentElement.getAttribute = () => input.mount || '';

const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const readExpanded = input.expanded_binding
  ? vm.runInNewContext(`cx => (${input.expanded_binding})`, context)
  : null;
const readRevision = input.revision_binding
  ? vm.runInNewContext(`cx => (${input.revision_binding})`, context)
  : null;
let deleteStopped = false;
const event = (type, target = {}, currentTarget = target) => {
  const nativeEvent = new context.Event(type);
  Object.assign(nativeEvent, {
    key: 'Enter',
    target,
    currentTarget,
    stopped: false,
    prevented: false,
    stopPropagation() { this.stopped = true; },
    preventDefault() { this.prevented = true; },
  });
  return nativeEvent;
};
const flush = async () => {
  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
};

async function run() {
  const expandedBefore = readExpanded ? unbox(readExpanded(cx)) : null;
  const revisionBefore = readRevision ? unbox(readRevision(cx)) : null;
  if (input.toggle_handler) {
    const toggleEvent = event('click');
    await fixture.handler(input.toggle_handler)(cx.event(toggleEvent));
  }
  if (input.keydown_handler) {
    const keyEvent = event('keydown');
    await fixture.handler(input.keydown_handler)(cx.event(keyEvent));
  }
  if (input.delete_handler) {
    const deleteEvent = event('click');
    await fixture.handler(input.delete_handler)(cx.event(deleteEvent));
    deleteStopped = deleteEvent.stopped;
    await flush();
    if (input.scenario === 'pending') {
      if (typeof finishRequest !== 'function') throw new Error('delete procedure did not start');
      finishRequest();
      await flush();
    }
  }
  const expandedAfter = readExpanded ? unbox(readExpanded(cx)) : null;
  const revisionAfter = readRevision ? unbox(readRevision(cx)) : null;
  const signals = Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, cx.signal(id).get().dehydrate()]));
  process.stdout.write(JSON.stringify({
    expanded_before: expandedBefore,
    expanded_after: expandedAfter,
    revision_before: revisionBefore,
    revision_after: revisionAfter,
    stopped: deleteStopped,
    requests,
    signals,
  }));
}

run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
