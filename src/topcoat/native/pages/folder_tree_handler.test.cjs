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
const readErrorHidden = input.error_binding
  ? vm.runInNewContext(`cx => (${input.error_binding})`, context)
  : null;
let deleteStopped = false;
const element = () => ({
  getAttribute(name) {
    if (name === 'data-folder-id') return String(input.folder_id || '');
    if (name === 'data-folder-name') return input.folder_name || '';
    if (name === 'data-folder-revision') return String(input.folder_revision || 0);
    return null;
  },
  closest(selector) {
    if (input.target_kind === 'delete' && selector === '[data-native-page-folder-delete]') return this;
    if (input.target_kind === 'toggle' && selector === '[data-native-page-folder-toggle]') return this;
    return null;
  },
});
const event = (type, target = element(), currentTarget = target) => {
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
  if (input.dispose_before) fixture.controller.abort();
  const expandedBefore = readExpanded ? unbox(readExpanded(cx).dehydrate()) === 'true' : null;
  const revisionBefore = readRevision ? Number(unbox(readRevision(cx).dehydrate())) : null;
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
    if (input.repeat_delete) {
      await fixture.handler(input.delete_handler)(cx.event(event('click')));
    }
    await flush();
    if (input.scenario === 'pending') {
      if (typeof finishRequest !== 'function') throw new Error('delete procedure did not start');
      finishRequest();
      await flush();
    }
  }
  const expandedAfter = readExpanded ? unbox(readExpanded(cx).dehydrate()) === 'true' : null;
  const revisionAfter = readRevision ? Number(unbox(readRevision(cx).dehydrate())) : null;
  const signals = Object.fromEntries(Object.keys(input.signals)
    .map(id => [id, cx.signal(id).get().dehydrate()]));

  const delegateHandler = input.toggle_handler || input.delete_handler || input.keydown_handler;
  if (delegateHandler) {
    const diagnostic = handlerFixture(input.signals, async () => ({
      ok: true,
      json: async () => input.reply,
    }), input.browser_source);
    const largeFolderId = '9007199254740993';
    const largeTarget = {
      getAttribute(name) {
        if (name === 'data-folder-id') return largeFolderId;
        if (name === 'data-folder-revision') return String(input.folder_revision || 0);
        return null;
      },
      closest(selector) {
        return selector === '[data-native-page-folder-toggle]' ? this : null;
      },
    };
    const largeEvent = new diagnostic.context.Event('click');
    Object.assign(largeEvent, {
      target: largeTarget,
      currentTarget: largeTarget,
      stopPropagation() {},
      preventDefault() {},
    });
    await diagnostic.handler(delegateHandler)(diagnostic.cx.event(largeEvent));
    const largeSignals = Object.fromEntries(Object.keys(input.signals)
      .map(id => [id, diagnostic.cx.signal(id).get().dehydrate()]));
    const largeState = JSON.stringify(largeSignals);
    if (!largeState.includes('"v":"9007199254740993"')) {
      throw new Error('folder row IDs must cross the DOM boundary without JavaScript Number rounding');
    }
    if (largeState.includes('"v":"9007199254740992"')) {
      throw new Error('large folder row IDs were rounded before updating the expanded state');
    }
  }

  process.stdout.write(JSON.stringify({
    expanded_before: expandedBefore,
    expanded_after: expandedAfter,
    revision_before: revisionBefore,
    revision_after: revisionAfter,
    error_hidden: readErrorHidden ? unbox(readErrorHidden(cx).dehydrate()) : null,
    stopped: deleteStopped,
    requests,
    signals,
  }));
}

run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
