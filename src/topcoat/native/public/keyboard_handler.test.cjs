'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = handlerFixture(input.signals, async () => {
  throw new Error('keyboard navigation must remain local');
}, input.browser_source);
let searchButtonClicks = 0;
const activatedDestinations = [];
const defaultDestinations = [];
const searchButton = {
  click() {
    searchButtonClicks += 1;
    if (input.search_open) {
      runtime.handler(input.search_open)(runtime.cx.event({type: 'click', target: {}, currentTarget: {}}));
    }
  },
};
const binding = source => vm.runInNewContext('cx => (' + source + ')', runtime.context)(runtime.cx);
const readBinding = source => {
  let value = binding(source);
  while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const rows = input.rows.map(({id, href, attributes}) => {
  const row = {
    id,
    hidden: false,
    focused: false,
    clicks: 0,
    getAttribute(name) {
      if (name === 'data-native-issue-row') return this.id;
      if (name === 'data-native-public-focused') return String(readBinding(input.focus_bindings[this.id]));
      return null;
    },
    closest(selector) { return selector === '[hidden]' && this.hidden ? this : null; },
    getClientRects() { return this.hidden ? [] : [{}]; },
    focus() {
      this.focused = true;
      activeElement = this.anchor;
    },
    scrollIntoView() { this.scrolled = true; },
  };
  row.anchor = {
    tagName: 'A',
    href,
    attributes,
    click() {
      row.clicks += 1;
      activatedDestinations.push(this.href);
    },
    focus: () => { row.focus(); },
  };
  return row;
});
let activeElement = {tagName: 'BODY'};
let overlayActive = false;
const overlay = {getClientRects: () => [{}], closest: () => null};
const blurredSearchInput = {getClientRects: () => [{}], closest: () => null};
const listeners = new Set();
runtime.context.getComputedStyle = () => ({visibility: 'visible'});
runtime.context.document = {
  get activeElement() { return activeElement; },
  getElementById() { return null; },
  querySelectorAll(selector) {
    if (selector === 'li[data-native-issue-row]') return rows;
    if (
      selector.includes('[data-native-public-mobile]') ||
      selector.includes('[data-native-markdown-preview]')
    ) return overlayActive ? [overlay] : [];
    if (selector.includes("[data-native-search='issues']")) return [blurredSearchInput];
    return [];
  },
  querySelector(selector) {
    if (selector === "li[data-native-public-focused='true'] a") {
      return rows.find(row => row.getAttribute('data-native-public-focused') === 'true')?.anchor ?? null;
    }
    if (selector === "li[data-native-public-focused='true']") {
      return rows.find(row => row.getAttribute('data-native-public-focused') === 'true') ?? null;
    }
    if (selector === "[data-native-search-open='issues']") return searchButton;
    return null;
  },
};
runtime.context.window.addEventListener = (name, listener, options = {}) => {
  if (options.signal?.aborted) return;
  if (name !== 'keydown') return;
  listeners.add(listener);
  options.signal?.addEventListener('abort', () => listeners.delete(listener), {once: true});
};
let nextFrameId = 0;
const frames = new Map();
runtime.context.requestAnimationFrame = callback => {
  const id = ++nextFrameId;
  frames.set(id, callback);
  return id;
};
runtime.context.cancelAnimationFrame = id => frames.delete(id);
const flushFrames = () => {
  while (frames.size > 0) {
    const pending = Array.from(frames.entries());
    frames.clear();
    for (const [, callback] of pending) callback();
  }
};

const readSignal = id => {
  let value = runtime.cx.signal(id).dehydrate();
  while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const readFocused = () => readSignal(input.focused_signal);
let activeController = runtime.controller;
let activeOwner = runtime.cx;
let mount = runtime.handler(input.mount, activeOwner);
const mountOwner = () => mount(activeOwner.event({type: 'mount', target: {}, currentTarget: {}}));
const remountWithFreshOwner = () => {
  activeController.abort();
  activeController = new AbortController();
  activeOwner = Object.assign(new runtime.context.fixture.Context(runtime.registry), {
    abortSignal: activeController.signal,
    event: event => new runtime.context.fixture.Event(event),
  });
  mount = runtime.handler(input.mount, activeOwner);
  mountOwner();
};
mountOwner();
const dispatch = (key, timeStamp, options = {}) => {
  const state = {prevented: false};
  assert.equal(listeners.size, 1, 'there is one live owner-scoped key listener');
  const event = {
    type: 'keydown', key, timeStamp,
    preventDefault() { state.prevented = true; },
    stopPropagation() {},
    ...options,
  };
  for (const listener of [...listeners]) listener(event);
  if (key === 'Enter' && !state.prevented && activeElement?.tagName === 'A') {
    defaultDestinations.push(activeElement.href);
  }
  return state;
};

if (input.board) {
  const movement = dispatch('j', 100);
  assert.equal(movement.prevented, false, 'board cards have no list row-key actions');
  const search = dispatch('/', 200);
  assert.equal(search.prevented, true);
  assert.equal(searchButtonClicks, 1, 'slash activates the shared board search control');
  flushFrames();
  assert.equal(readSignal(input.expanded_signal), true, 'slash opens the shared search overlay');
  runtime.controller.abort();
  assert.equal(listeners.size, 0);
  process.stdout.write(JSON.stringify({opened: readSignal(input.expanded_signal), searchButtonClicks}));
  process.exit(0);
}

const first = rows[0].id;
const second = rows[1].id;
const listSearch = dispatch('/', 50);
assert.equal(listSearch.prevented, true, 'slash opens issue search from the list');
assert.equal(searchButtonClicks, 1);
assert.equal(readSignal(input.expanded_signal), true);
flushFrames();
dispatch('j', 100);
assert.equal(readFocused(), first, 'initial j focuses the first visible row');
flushFrames();
dispatch('j', 200);
assert.equal(readFocused(), first, 'movement repeats are throttled for 150 ms');
dispatch('j', 251);
assert.equal(readFocused(), second, 'j advances after the repeat window');
flushFrames();

rows.reverse();
dispatch('ArrowDown', 401);
assert.equal(readFocused(), first, 'reordered visible rows retain focus by issue id');
flushFrames();
dispatch('Home', 402);
assert.equal(readFocused(), second, 'Home moves to the first row without movement throttling');
const immediateEnter = dispatch('Enter', 403);
assert.equal(immediateEnter.prevented, true);
assert.equal(rows.find(row => row.id === second).clicks, 1, 'Enter uses the updated focused binding before a frame');
assert.equal(
  activatedDestinations.at(-1),
  rows.find(row => row.id === second).anchor.href,
  'Enter activates the href emitted for the focused issue row',
);
flushFrames();
dispatch('End', 404);
assert.equal(readFocused(), first, 'End moves to the last row without movement throttling');
flushFrames();
dispatch('ArrowUp', 600);
assert.equal(readFocused(), second, 'ArrowUp moves toward the first visible row');
flushFrames();
dispatch('K', 751);
assert.equal(readFocused(), second, 'uppercase K is supported for read-only row navigation');
flushFrames();
const enter = dispatch('Enter', 752);
assert.equal(enter.prevented, true);
assert.equal(rows.find(row => row.id === second).clicks, 2, 'Enter activates the current mounted row link');
assert.deepEqual(
  activatedDestinations,
  [rows.find(row => row.id === second).anchor.href, rows.find(row => row.id === second).anchor.href],
  'Enter follows the mounted href of the issue selected by the current row order',
);
for (const [index, key] of ['c', 'x', 's', 'p', 'm'].entries()) {
  const result = dispatch(key, 410 + index);
  assert.equal(result.prevented, false, `${key} remains inert for published viewers`);
  assert.equal(readFocused(), second);
}
const selectAll = dispatch('a', 416, {metaKey: true});
assert.equal(selectAll.prevented, false, 'Cmd+A does not activate private bulk selection');
assert.equal(readFocused(), second);

overlayActive = true;
const overlayKey = dispatch('j', 600);
assert.equal(overlayKey.prevented, false, 'visible dialogs and the mobile drawer own their keys');
assert.equal(readFocused(), second);
overlayActive = false;

activeElement = {tagName: 'INPUT'};
const typed = dispatch('ArrowDown', 800);
assert.equal(typed.prevented, false, 'typing contexts retain their own keyboard input');
assert.equal(readFocused(), second);
for (const [index, target] of [
  {tagName: 'TEXTAREA'},
  {tagName: 'SELECT'},
  {tagName: 'DIV', isContentEditable: true},
].entries()) {
  activeElement = target;
  const typing = dispatch('k', 820 + index * 160);
  assert.equal(typing.prevented, false, 'all native text-entry contexts keep their keys');
  assert.equal(readFocused(), second);
}

activeElement = {tagName: 'BODY'};
const blurredSearch = dispatch('ArrowUp', 1320);
assert.equal(blurredSearch.prevented, true, 'a nonempty search query does not suppress list navigation after blur');
assert.equal(readFocused(), second);

const focusedRow = rows.find(row => row.id === second);
focusedRow.hidden = true;
remountWithFreshOwner();
assert.equal(readFocused(), '', 'remount reconciliation clears an issue hidden by collapse/filter');
focusedRow.hidden = false;
remountWithFreshOwner();
assert.equal(readFocused(), '', 'expanding again does not resurrect stale row focus');
dispatch('Escape', 1321);
assert.equal(readFocused(), '', 'Escape clears visible focus');
for (const row of rows) row.hidden = true;
remountWithFreshOwner();
assert.equal(dispatch('j', 1400).prevented, true, 'empty lists consume row movement without selecting anything');
assert.equal(dispatch('Home', 1401).prevented, false);
assert.equal(dispatch('End', 1402).prevented, false);
assert.equal(dispatch('Enter', 1403).prevented, false);
assert.equal(readFocused(), '', 'empty-list keys cannot activate a stale issue');
for (const row of rows) row.hidden = false;
rows.reverse();
remountWithFreshOwner();
assert.equal(readFocused(), '', 'rows returning after an empty filter do not restore old focus');
dispatch('Home', 1450);
assert.equal(readFocused(), first, 'Home selects the first row after returning to the original order');
flushFrames();
dispatch('Escape', 1451);
assert.equal(readFocused(), '', 'Escape clears the virtual row focus');
const enterAfterEscape = dispatch('Enter', 1452);
assert.equal(enterAfterEscape.prevented, false);
assert.equal(
  defaultDestinations.length,
  0,
  'Escape leaves no focused old link for the browser to activate on Enter',
);
for (const row of rows) {
  row.focused = false;
  row.scrolled = false;
}
dispatch('j', 1600);
assert.equal(readFocused(), first, 'the final keypress records virtual focus before disposal');
assert.ok(frames.size > 0, 'keyboard focus schedules DOM work for the next frame');
activeController.abort();
assert.equal(frames.size, 0, 'disposing the owner cancels its queued focus frame');
assert.equal(readFocused(), first, 'disposing the owner preserves the selected issue ID');
flushFrames();
assert.equal(rows.some(row => row.scrolled), false, 'a disposed owner cannot scroll a stale row');
assert.equal(listeners.size, 0, 'the global listener is removed with its owner');
process.stdout.write(JSON.stringify({
  focused: readFocused(),
  searchButtonClicks,
  disposed: activeController.signal.aborted,
}));
