'use strict';
// DOM-free execution of the production Rust-emitted mount expression.
// Input: {source, signals, closeId}; run from the repository root.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const {source, signals, closeId} = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(runtime.split(bootstrap).length - 1, 1, 'packaged bootstrap');
const eventClass = runtime.match(/event\(\w+\)\{return new (\w+)\(\w+\)\}/)?.[1];
assert.ok(eventClass, 'packaged Event surrogate');
assert.equal(typeof source, 'string');
assert.ok(Object.hasOwn(signals, closeId), 'close signal is in page signals');

class Target {
  constructor() { this.listeners = []; }
  addEventListener(type, callback, options = {}) {
    if (typeof options === 'boolean') options = {capture: options};
    const entry = {type, callback, options};
    if (options.signal?.aborted) return;
    this.listeners.push(entry);
    options.signal?.addEventListener('abort', () => {
      this.listeners = this.listeners.filter(item => item !== entry);
    }, {once: true});
  }
  removeEventListener(type, callback) {
    this.listeners = this.listeners.filter(item => item.type !== type || item.callback !== callback);
  }
  dispatch(type, input = {}) {
    const event = {
      type, pointerId: 1, pointerType: 'touch', isPrimary: true,
      clientY: 100, timeStamp: 0, target: this, currentTarget: this,
      defaultPrevented: false,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() {}, stopImmediatePropagation() {},
      ...input,
    };
    for (const entry of [...this.listeners]) {
      if (entry.type === type) entry.callback(event);
    }
    return event;
  }
}

function setup({height = 1000, width = 767} = {}) {
  let now = 0, serial = 0;
  const timers = new Map(), window = new Target(), grab = new Target();
  const sheet = {style: {transition: '', transform: '', visibility: ''},
    getBoundingClientRect: () => ({height})};
  grab.closest = selector => selector === '[data-native-issue-peek]' ? sheet : null;
  const abort = new AbortController();
  const context = {
    TextEncoder, TextDecoder, queueMicrotask, window, innerWidth: width,
    document: {documentElement: {getAttribute: () => ''}},
    performance: {now: () => now},
    setTimeout(callback, delay) { const id = ++serial; timers.set(id, {at: now + delay, callback}); return id; },
    clearTimeout(id) { timers.delete(id); },
  };
  window.setTimeout = context.setTimeout;
  window.clearTimeout = context.clearTimeout;
  vm.runInNewContext(runtime.replace(bootstrap,
    `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass}};`), context);
  const registry = new context.fixture.Registry();
  const cx = Object.assign(new context.fixture.Context(registry), {
    event: event => new context.fixture.Event(event), abortSignal: abort.signal,
  });
  for (const [id, value] of Object.entries(signals)) registry.insert(id, cx.hydrate(value));
  const closes = [];
  const handle = registry.handle(closeId), originalSet = handle.set;
  handle.set = value => {
    closes.push({value: value.toString(), visibility: sheet.style.visibility, at: now});
    return originalSet(value);
  };
  const mount = vm.runInNewContext(`cx => (${source})`, context)(cx);
  mount(cx.event({target: grab, type: 'mount'}));
  function advance(ms) {
    const until = now + ms;
    for (;;) {
      const next = [...timers].filter(([, timer]) => timer.at <= until)
        .sort((a, b) => a[1].at - b[1].at || a[0] - b[0])[0];
      if (!next) break;
      now = next[1].at; timers.delete(next[0]); next[1].callback();
    }
    now = until;
  }
  return {
    sheet, grab, window, abort, timers, closes, advance,
    setHeight(value) { height = value; },
    setNow(value) { now = value; },
    down(input) { return grab.dispatch('pointerdown', {timeStamp: now, ...input}); },
    move(dy, input) { return window.dispatch('pointermove', {clientY: 100 + dy, timeStamp: now, ...input}); },
    up(input) { return window.dispatch('pointerup', {timeStamp: now, ...input}); },
    cancel(input) { return window.dispatch('pointercancel', {timeStamp: now, ...input}); },
  };
}

const cases = [];
function test(name, body) { cases.push([name, body]); }
function springback(t) {
  assert.match(t.sheet.style.transform, /^translateY\(0(?:px)?\)$/);
  assert.match(t.sheet.style.transition, /170ms/);
  t.advance(199); assert.notEqual(t.sheet.style.transition, '');
  t.advance(1); assert.equal(t.sheet.style.transform, '');
  assert.equal(t.sheet.style.transition, ''); assert.equal(t.closes.length, 0);
}
function dismiss(t, height) {
  assert.equal(t.sheet.style.transform, `translateY(${height + 40}px)`);
  assert.match(t.sheet.style.transition, /170ms/);
  assert.match(t.sheet.style.transition.replaceAll(' ', ''), /cubic-bezier\((?:0)?\.2,(?:0)?\.8,(?:0)?\.3,1\)/);
  t.advance(169); assert.equal(t.closes.length, 0);
  t.advance(1); assert.equal(t.closes.length, 1);
  assert.deepEqual(t.closes[0], {value: '', visibility: 'hidden', at: 170 + t.endAt});
  assert.equal(t.sheet.style.visibility, 'hidden');
  assert.equal(t.sheet.style.transform, `translateY(${height + 40}px)`);
}
function drag(t, dy, duration, end = 'up') {
  t.down(); t.advance(duration); t.move(dy); t.endAt = duration; t[end]();
}

for (const [dy, duration, commit] of [
  [280, 1000, false], [280.001, 1000, true],
  [24, 10, false], [24.001, 10, true],
  [90, 200, false], [90.001, 200, true],
]) test(`strict threshold dy=${dy} elapsed=${duration}`, () => {
  const t = setup(); drag(t, dy, duration); commit ? dismiss(t, 1000) : springback(t);
});
test('zero measured height uses 480px and slides 520px', () => {
  const t = setup({height: 0}); drag(t, 135, 1000); dismiss(t, 480);
});
test('current height is measured at release', () => {
  const t = setup(); t.down(); t.setHeight(400); t.advance(1000); t.move(113);
  t.endAt = 1000; t.up(); dismiss(t, 400);
});
test('elapsed follows event timestamps independently of performance clock', () => {
  const t = setup(); t.down({timeStamp: 10}); t.advance(1000); t.move(25);
  t.endAt = 1000; t.up({timeStamp: 20}); dismiss(t, 1000);
});
test('pointer cancel springs back even above threshold', () => {
  const t = setup(); drag(t, 500, 1000, 'cancel'); springback(t);
});
test('upward move is clamped to zero and never dismisses', () => {
  const t = setup(); t.down(); t.advance(100); const event = t.move(-50);
  assert.equal(event.defaultPrevented, true);
  assert.ok(t.sheet.style.transform === '' || /^translateY\(0(?:px)?\)$/.test(t.sheet.style.transform));
  t.up(); springback(t);
});
for (const [label, options, down] of [
  ['desktop boundary', {width: 768}, {}], ['mouse', {}, {pointerType: 'mouse'}],
  ['non-primary', {}, {isPrimary: false}],
  ...['button', 'a', 'input', 'textarea', 'select'].map(tag =>
    [tag, {}, {target: {closest: selector => selector.split(',').map(s => s.trim()).includes(tag) ? {} : null}}]),
]) test(`ignores ${label} origin`, () => {
  const t = setup(options); t.down(down); t.advance(1000);
  assert.equal(t.move(500).defaultPrevented, false); t.up(); t.advance(1000);
  assert.equal(t.closes.length, 0); assert.equal(t.sheet.style.transform, '');
});
test('pen can dismiss and moves prevent default', () => {
  const t = setup(); t.down({pointerType: 'pen'}); t.advance(1000);
  assert.equal(t.move(500).defaultPrevented, true); t.endAt = 1000; t.up(); dismiss(t, 1000);
});
test('external pointer move/up/cancel leaves matching drag active', () => {
  const t = setup(); t.down(); t.advance(1000);
  assert.equal(t.move(500, {pointerId: 2}).defaultPrevented, false);
  assert.equal(t.sheet.style.transform, ''); t.up({pointerId: 2}); t.cancel({pointerId: 2});
  assert.equal(t.move(500).defaultPrevented, true); t.endAt = 1000; t.up(); dismiss(t, 1000);
});
test('window end listeners capture and move is non-passive', () => {
  const t = setup(); t.down();
  for (const type of ['pointerup', 'pointercancel']) {
    const listeners = t.window.listeners.filter(entry => entry.type === type);
    assert.ok(listeners.length); assert.ok(listeners.every(entry => entry.options.capture === true));
  }
  const moves = t.window.listeners.filter(entry => entry.type === 'pointermove');
  assert.ok(moves.length); assert.ok(moves.every(entry => entry.options.passive === false));
});
test('scope abort removes listeners and cancels pending dismiss', () => {
  const t = setup(); drag(t, 500, 1000); assert.equal(t.timers.size, 1);
  t.abort.abort(); assert.equal(t.grab.listeners.length, 0); assert.equal(t.window.listeners.length, 0);
  assert.equal(t.timers.size, 0); t.advance(1000); assert.equal(t.closes.length, 0);
});
test('scope abort cancels pending springback cleanup', () => {
  const t = setup(); drag(t, 20, 1000); assert.equal(t.timers.size, 1);
  const before = {...t.sheet.style}; t.abort.abort(); assert.equal(t.timers.size, 0);
  t.advance(1000); assert.deepEqual(t.sheet.style, before); assert.equal(t.closes.length, 0);
});
test('scope abort during drag prevents further pointer updates', () => {
  const t = setup(); t.down(); t.move(50); t.abort.abort();
  assert.equal(t.move(500).defaultPrevented, false); t.up(); t.advance(1000);
  assert.equal(t.sheet.style.transform, 'translateY(50px)'); assert.equal(t.closes.length, 0);
});

let failures = 0;
for (const [name, body] of cases) {
  try { body(); }
  catch (error) { failures++; console.error(`FAIL ${name}\n${error.stack}`); }
}
if (failures) process.exitCode = 1;
else console.log(`${cases.length} emitted peek gesture cases passed`);
