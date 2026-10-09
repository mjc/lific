'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const flush = async () => {
  for (let attempt = 0; attempt < 60; attempt += 1) await Promise.resolve();
};

function snapshot(cx, signals) {
  return Object.fromEntries(Object.keys(signals).map(id => [id, cx.signal(id).dehydrate().v]));
}

async function run() {
  const clickFixture = handlerFixture(input.signals, async () => {
    throw new Error('starting title edit must not perform network I/O');
  });
  const beforeClick = snapshot(clickFixture.cx, input.signals);
  clickFixture.handler(input.click_handler)(clickFixture.cx.event({
    type: 'click', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {},
  }));
  await flush();
  const afterClick = snapshot(clickFixture.cx, input.signals);
  const clickChanges = Object.keys(input.signals).filter(id =>
    JSON.stringify(beforeClick[id]) !== JSON.stringify(afterClick[id]));
  assert.ok(clickChanges.some(id => afterClick[id].v === true),
    'click enters edit mode (other focus or draft signals may also update)');

  const keyboardFixture = handlerFixture(input.signals, async () => {
    throw new Error('starting title edit must not perform network I/O');
  });
  const beforeKey = snapshot(keyboardFixture.cx, input.signals);
  const keyEvent = {
    type: 'keydown', key: 'Enter', target: {}, currentTarget: {},
    preventDefault() { this.prevented = true; }, stopPropagation() {},
  };
  keyboardFixture.handler(input.keydown_handler)(keyboardFixture.cx.event(keyEvent));
  await flush();
  const afterKey = snapshot(keyboardFixture.cx, input.signals);
  const keyChanges = Object.keys(input.signals).filter(id =>
    JSON.stringify(beforeKey[id]) !== JSON.stringify(afterKey[id]));
  assert.equal(keyEvent.prevented, true, 'Enter prevents its default action');
  assert.ok(keyChanges.some(id => afterKey[id].v === true),
    'Enter enters edit mode (other focus or draft signals may also update)');
  return {click_editing: true, keyboard_editing: true};
}

run().then(result => process.stdout.write(JSON.stringify(result))).catch(error => {
  process.stderr.write(`${error.stack}\n`);
  process.exitCode = 1;
});
