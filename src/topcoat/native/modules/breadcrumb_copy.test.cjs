'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const notifications = [];
const unbox = value => {
  while (value !== null && typeof value === 'object') {
    if (typeof value.dehydrate === 'function') value = value.dehydrate();
    else if ('v' in value) value = value.v;
    else break;
  }
  return value;
};
const fixture = handlerFixture(input.signals, async () => {
  throw new Error('copy does not make network requests');
}, input.browser_source);
const {context, cx} = fixture;
context.CustomEvent = class extends context.Event {
  constructor(type, options = {}) { super(type); this.detail = options.detail; }
};
context.window.dispatchEvent = event => { notifications.push(event); return true; };
context.navigator = {clipboard: {writeText: async () => { throw new Error('clipboard denied'); }}};
context.document.createElement = () => ({
  value: '', style: {}, setAttribute() {}, select() {}, remove() {},
});
context.document.body = {appendChild() {}};
context.document.execCommand = () => false;

async function main() {
  const event = new context.Event('click');
  let prevented = false;
  let stopped = false;
  Object.assign(event, {
    preventDefault() { prevented = true; },
    stopPropagation() { stopped = true; },
  });
  fixture.handler(input.handler)(cx.event(event));
  for (let index = 0; index < 12; index++) await Promise.resolve();

  assert.equal(prevented, true);
  assert.equal(stopped, true);
  assert.equal(notifications.length, 1);
  const notification = notifications[0];
  assert.equal(notification.type, 'lific:native-toast-error');
  const detail = unbox(notification.detail);
  assert.equal(String(unbox(detail.account_id)), String(input.account_id));
  assert.equal(unbox(detail.message), "Couldn't copy to clipboard");
  process.stdout.write(JSON.stringify({passed: true}));
}

main().catch(error => { console.error(error); process.exitCode = 1; });
