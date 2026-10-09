'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const events = [];
const fixture = handlerFixture(input.signals, () => {
  assert.fail('assignee edits are sent through the durable native owner');
}, input.browser_source);
fixture.context.CustomEvent = class extends Event {
  constructor(type, options) {
    super(type, options);
    this.detail = options.detail;
  }
};
fixture.context.window.dispatchEvent = event => {
  events.push(event);
  event.preventDefault();
  return false;
};

const click = () => fixture.cx.event(new Event('click', {cancelable: true}));
fixture.handler(input.toggle_handler)(click());
fixture.handler(input.clear_handler)(click());
fixture.handler(input.human_handler)(click());
assert.equal(events.length, 2);
assert.equal(events[0].type, 'lific:native-issue-assignee-request');
assert.deepEqual(serialized(events[0]), input.clear_request);
assert.deepEqual(serialized(events[1]), input.human_request);

fixture.handler(input.search_handler)(fixture.cx.event({type: 'input', target: {value: 'ALPHA'}}));
assert.equal(events.length, 2, 'search remains a local picker operation');
fixture.handler(input.first_person_handler)(click());
fixture.handler(input.second_person_handler)(click());
fixture.handler(input.first_person_handler)(click());
fixture.handler(input.cancel_handler)(click());
assert.equal(events.length, 2, 'Cancel drops the local draft');
fixture.handler(input.toggle_handler)(click());
fixture.handler(input.first_person_handler)(click());
fixture.handler(input.second_person_handler)(click());
fixture.handler(input.apply_handler)(click());
assert.equal(events.length, 3);
const request = serialized(events[2]);
assert.deepEqual(request, input.named_request);
process.stdout.write(JSON.stringify({request}));

function serialized(event) {
  return JSON.parse(JSON.stringify(event.detail.dehydrate()));
}
