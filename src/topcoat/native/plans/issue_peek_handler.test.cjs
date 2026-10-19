'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const events = [];
const {cx, context, handler: emit} = handlerFixture(input.signals, async () => {
  throw new Error('Issue Peek must not perform network I/O');
}, input.browser_source);
context.CustomEvent = class CustomEvent {
  constructor(type, options) {
    this.type = type;
    this.detail = options?.detail;
  }
};
context.document.dispatchEvent = event => {
  events.push({type: event.type, identifier: event.detail.identifier});
  return true;
};

let normalClicks = 0;
for (const link of input.links) {
  assert.match(link.href, new RegExp(`/ACC/issues/${input.identifier}$`),
    'ordinary route link remains available');
  const handler = emit(link.handler);
  const event = shiftKey => ({
    type: 'click', shiftKey, target: {}, currentTarget: {},
    defaultPrevented: false,
    preventDefault() { this.prevented = true; this.defaultPrevented = true; },
    stopPropagation() { this.stopped = true; },
  });
  const regular = event(false);
  handler(cx.event(regular));
  assert.equal(regular.prevented, undefined, 'normal navigation is untouched');
  assert.equal(regular.defaultPrevented, false, 'normal click is not canceled');
  assert.equal(regular.stopped, undefined, 'normal click propagation is untouched');
  normalClicks += 1;
  const modified = event(true);
  handler(cx.event(modified));
  assert.equal(modified.prevented, true, 'Shift-click prevents route navigation');
  assert.equal(modified.defaultPrevented, true, 'Shift-click default is canceled');
  assert.equal(modified.stopped, true, 'Shift-click does not bubble to other controls');
}

assert.equal(events.length, input.links.length);
for (const event of events) {
  assert.equal(event.type, 'lific:native-issue-peek-request');
  assert.equal(event.identifier, input.identifier);
}
process.stdout.write(JSON.stringify({peek_events: events, normal_clicks: normalClicks}));
