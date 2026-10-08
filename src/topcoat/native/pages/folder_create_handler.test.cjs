'use strict';

// Replay emitted Pages menu clicks through the packaged Topcoat runtime.
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const fixture = handlerFixture(input.signals, async () => {
  throw new Error('opening the folder composer must not send a request');
}, input.browser_source);
const {cx} = fixture;

for (const handlerSource of input.handlers) {
  fixture.handler(handlerSource)(cx.event({
    type: 'click',
    target: {},
    currentTarget: {},
    stopPropagation() {},
    preventDefault() {},
  }));
}

const signals = Object.fromEntries(Object.keys(input.signals)
  .map(id => [id, cx.signal(id).get().dehydrate()]));
process.stdout.write(JSON.stringify({signals}));
