'use strict';

// Replay emitted Pages menu clicks through the packaged Topcoat runtime.
const fs = require('node:fs');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
let finishRequest;
const fixture = handlerFixture(input.signals, async (url, options) => {
  const path = new URL(url, 'http://localhost').pathname;
  if (!path.endsWith('/__native_pages/create-folder')) {
    throw new Error(`unexpected folder creation procedure ${path}`);
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
const animationFrames = [];
let focusCount = 0;
let composerHidden = true;
context.requestAnimationFrame = callback => animationFrames.push(callback);
const unbox = value => {
  while (value !== null && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
context.document.querySelector = selector => {
  if (selector === "input[placeholder='Folder name']" && input.composer_hidden_binding) {
    return {focus() {
      assert.equal(composerHidden, false,
        'folder input is visible before the deferred focus runs');
      focusCount += 1;
    }};
  }
  return null;
};

async function run() {
  const change = (handler, value) => fixture.handler(handler)(cx.event({
    type: 'change', target: {value}, currentTarget: {},
  }));
  if (input.change_handler) change(input.change_handler, input.folder_value);
  for (const handlerSource of input.handlers) {
    fixture.handler(handlerSource)(cx.event({
      type: 'click',
      target: {},
      currentTarget: {},
      stopPropagation() {},
      preventDefault() {},
    }));
  }
  if (input.after_change_handler) {
    change(input.after_change_handler, input.after_folder_value);
  }

async function flush() {
  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
}

if (input.create_handler || input.key_handler) {
  const create = input.create_handler ? fixture.handler(input.create_handler) : null;
  const previousQuerySelector = context.document.querySelector;
  context.document.querySelector = selector => {
    if (selector === '#native-pages-create-folder-button' && create) {
      return {click: () => create(cx.event({type: 'click', target: {}, currentTarget: {}}))};
    }
    return previousQuerySelector(selector);
  };
  if (input.name_handler) {
    fixture.handler(input.name_handler)(cx.event({
      type: 'input', target: {value: input.name}, currentTarget: {},
    }));
  }
  if (input.key_handler) {
    fixture.handler(input.key_handler)(cx.event({
      type: 'keydown', key: input.key || 'Enter', target: {}, currentTarget: {},
      preventDefault() {}, stopPropagation() {},
    }));
  } else if (create) {
    create(cx.event({type: 'click', target: {}, currentTarget: {}}));
  }
  if (input.scenario === 'disposed') fixture.controller.abort();
  await flush();
  if (input.scenario === 'pending') {
    if (typeof finishRequest !== 'function') throw new Error('folder procedure did not start');
    finishRequest();
    await flush();
  }
}
if (input.focus_handler) {
  fixture.handler(input.focus_handler)(cx.event({
    type: 'click', target: {}, currentTarget: {},
  }));
  if (input.scenario === 'focus_closed') {
    fixture.handler(input.close_handler)(cx.event({
      type: 'keydown', key: 'Escape', target: {}, currentTarget: {},
      preventDefault() {}, stopPropagation() {},
    }));
  } else if (input.scenario === 'focus_disposed') {
    fixture.controller.abort();
  }
  await flush();
  if (input.composer_hidden_binding) {
    const readHidden = vm.runInNewContext(
      `cx => (${input.composer_hidden_binding})`, context,
    );
    composerHidden = unbox(readHidden(cx));
  }
  while (animationFrames.length) animationFrames.shift()();
  await flush();
}
if (input.blur_handler) {
  fixture.handler(input.blur_handler)(cx.event({type: 'blur', target: {}, currentTarget: {}}));
}

const signals = Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, cx.signal(id).get().dehydrate()]));
const filterValue = input.folder_value_binding
  ? unbox(vm.runInNewContext(`cx => (${input.folder_value_binding})`, context)(cx))
  : null;
process.stdout.write(JSON.stringify({signals, requests, focus_count: focusCount, filter_value: filterValue}));
}

run().catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
