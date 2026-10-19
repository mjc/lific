'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const {fixtureRuntime} = require('../runtime_fixture.cjs');
const {emittedShard} = require('../pages/activity_shard_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const context = {TextEncoder, TextDecoder, queueMicrotask};
vm.runInNewContext(fixtureRuntime(['Context', 'Registry', 'Event']), context);
const registry = new context.fixture.Registry();
const cx = new context.fixture.Context(registry);
for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
const plain = value => {
  if (value && typeof value.dehydrate === 'function') value = value.dehydrate();
  return JSON.parse(JSON.stringify(value));
};
const shard = emittedShard(input.shard_marker, context, cx, plain);
process.stdout.write(JSON.stringify(shard));
