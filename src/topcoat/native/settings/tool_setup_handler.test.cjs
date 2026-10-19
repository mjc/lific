'use strict';

const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const {fixtureRuntime} = require('../runtime_fixture.cjs');
const copied = [];
const timers = [];
let releaseClipboard;
let rejectClipboardPromise;
let delayClipboard = false;
const context = {
  TextEncoder,
  TextDecoder,
  queueMicrotask,
  window: {location: {origin: 'https://example.test'}},
  navigator: {platform: 'MacIntel', clipboard: {writeText: value => {
    copied.push(value);
    if (delayClipboard) return new Promise((resolve, reject) => {
      releaseClipboard = resolve;
      rejectClipboardPromise = reject;
    });
    return Promise.resolve();
  }}},
  document: {documentElement: {getAttribute: () => '/app'}},
  setTimeout: callback => { timers.push(callback); return timers.length; },
};
vm.runInNewContext(fixtureRuntime(['Context', 'Registry', 'Event']), context);
const registry = new context.fixture.Registry();
const cx = Object.assign(new context.fixture.Context(registry), {
  event: event => new context.fixture.Event(event),
  abortSignal: new AbortController().signal,
});
for (const [id, value] of Object.entries(input.signals)) registry.insert(id, cx.hydrate(value));
const event = {type: 'click', target: {}, currentTarget: {}, preventDefault() {}, stopPropagation() {}};
const invoke = (source, target) => vm.runInNewContext(`cx => (${source})`, context)(target)(target.event(event));
(async () => {
  if (input.run_mount !== false) {
    invoke(input.mount, cx);
    await new Promise(resolve => setImmediate(resolve));
    const mountedOs = Object.keys(input.signals)
      .map(id => readValue(registry.read(id))).find(value => value === 'mac');
    const detectorOs = mountedOs ?? Object.keys(input.signals)
      .map(id => readValue(registry.read(id))).find(value => value === 'macos');
    if (detectorOs !== 'macos' && detectorOs !== 'mac') throw new Error('macOS detector did not select a macOS value');
    if (!input.codex_group_oses.includes('mac')) throw new Error('Codex Linux/macOS group omits mac');
    if (!input.mac_group_binding) throw new Error('missing emitted aria-pressed binding');
    const macGroupPressed = vm.runInNewContext(`cx => (${input.mac_group_binding})`, context)(cx);
    if (macGroupPressed.toString() !== 'true') throw new Error('detected macOS did not select Codex Linux/macOS group');
  }
  invoke(input.select_windows, cx);
  invoke(input.copy, cx);
  await new Promise(resolve => setImmediate(resolve));
  if (copied.length !== 1) throw new Error(`expected one clipboard write, got ${copied.length}`);
  if (!copied[0].startsWith('setx LIFIC_API_KEY "')) {
    throw new Error(`copy used stale OS command: ${copied[0]}`);
  }
  const firstStatus = Object.keys(input.signals).some(id =>
    readValue(registry.read(id)) === true);
  if (!firstStatus) throw new Error('successful copy did not publish copied status');
  const label = vm.runInNewContext(`cx => (${input.copy_label_expression})`, context)(cx);
  if (label.toString() !== 'Copied') throw new Error(`copy label did not update: ${label}`);

  async function attemptAbortedCopy(handler, reject = false) {
    const aborter = new AbortController();
    const currentRegistry = new context.fixture.Registry();
    const currentCx = Object.assign(new context.fixture.Context(currentRegistry), {
      event: received => new context.fixture.Event(received),
      abortSignal: aborter.signal,
    });
    for (const [id, value] of Object.entries(input.signals)) {
      currentRegistry.insert(id, currentCx.hydrate(value));
    }
    const prior = Object.fromEntries(Object.keys(input.signals)
      .map(id => [id, readValue(currentRegistry.read(id))]));
    delayClipboard = true;
    const action = vm.runInNewContext(`cx => (${handler})`, context)(currentCx);
    action(currentCx.event(event));
    await new Promise(resolve => setImmediate(resolve));
    aborter.abort();
    if (reject) rejectClipboardPromise();
    else releaseClipboard();
    await new Promise(resolve => setImmediate(resolve));
    delayClipboard = false;
    const published = Object.keys(input.signals).some(id =>
      prior[id] !== true && readValue(currentRegistry.read(id)) === true);
    return !published;
  }

  const abortSuccessSafe = await attemptAbortedCopy(input.config_copy);
  const abortFailureSafe = await attemptAbortedCopy(input.config_copy, true);
  if (!abortSuccessSafe || !abortFailureSafe) {
    throw new Error(`retired config copy published feedback: success=${abortSuccessSafe}, failure=${abortFailureSafe}`);
  }
  process.stdout.write(JSON.stringify({clipboard: copied[0], copied_status: true, abort_safe: true}));
})().catch(error => { setImmediate(() => { throw error; }); });

function readValue(value) {
  return value !== null && typeof value === 'object' && 'v' in value ? value.v : value;
}
