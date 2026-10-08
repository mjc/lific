'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {handlerFixture} = require('../handler_fixture.cjs');
const {emittedShard} = require('./activity_shard_fixture.cjs');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const fixture = handlerFixture(input.signals, () => { throw new Error('preview must not fetch'); }, input.browser_source);
const {cx, context, controller, handler} = fixture;

class FakeElement {
  constructor(attributes = {}, parent = null) {
    this.attributes = new Map(Object.entries(attributes));
    this.parentElement = parent;
    this.children = [];
    this.listeners = new Map();
    if (parent) parent.children.push(this);
  }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  matches(selector) {
    if (selector.startsWith('img[')) return this instanceof FakeImage &&
      this.getAttribute('data-native-attachment-image') !== null &&
      this.getAttribute('data-native-original-src') !== null;
    if (selector === '[data-native-markdown-preview]') return this.getAttribute('data-native-markdown-preview') !== null;
    return false;
  }
  closest(selector) {
    for (let current = this; current; current = current.parentElement) {
      if (current.matches(selector)) return current;
    }
    return null;
  }
  contains(node) {
    for (let current = node; current; current = current.parentElement) if (current === this) return true;
    return false;
  }
  querySelectorAll(selector) {
    const found = [];
    const visit = node => {
      for (const child of node.children) {
        if (child.matches(selector)) found.push(child);
        visit(child);
      }
    };
    visit(this);
    return found;
  }
  addEventListener(type, callback, options) {
    this.listeners.set(type, {callback, options});
  }
}
class FakeImage extends FakeElement {
  constructor(attributes, parent, {complete = false, naturalWidth = 0} = {}) {
    super(attributes, parent);
    this.complete = complete;
    this.naturalWidth = naturalWidth;
    this.sourceWrites = 0;
  }
  setAttribute(name, value) {
    if (name === 'src') this.sourceWrites += 1;
    super.setAttribute(name, value);
  }
}
class FakeEvent {
  constructor(type, target, currentTarget = target, key = '') {
    this.type = type;
    this.target = target;
    this.currentTarget = currentTarget;
    this.key = key;
    this.defaultPrevented = false;
  }
  preventDefault() { this.defaultPrevented = true; }
  stopPropagation() {}
  stopImmediatePropagation() {}
}
context.Element = FakeElement;
context.HTMLImageElement = FakeImage;
const windowListeners = [];
context.window.addEventListener = (type, callback, options) => windowListeners.push({type, callback, options});
const unbox = value => {
  while (value && typeof value === 'object' && Object.hasOwn(value, 'v')) value = value.v;
  return value;
};
const evaluate = (source, owner = cx) => {
  let value = vm.runInNewContext(`cx => (${source})`, context)(owner);
  if (value && typeof value.dehydrate === 'function') value = value.dehydrate();
  return unbox(value);
};
const values = owner => Object.fromEntries(Object.keys(input.signals).map(id => [
  id,
  unbox(owner.signal(id).get().dehydrate()),
]));
const signalSnapshot = owner => Object.fromEntries(Object.keys(input.signals).map(id => [
  id, owner.signal(id).get().dehydrate(),
]));
const plain = value => {
  if (value && typeof value.dehydrate === 'function') value = value.dehydrate();
  return JSON.parse(JSON.stringify(value));
};
const click = handler(input.click_handler);
const mount = handler(input.mount_handler);

const root = new FakeElement();
const image = new FakeImage({
  src: `${input.mount}/api/attachments/9007199254740999/thumbnail`,
  'data-native-attachment-image': '',
  'data-native-original-src': `${input.mount}/api/attachments/9007199254740999`,
  alt: 'Page image',
}, root);
const before = values(cx);
assert.equal(evaluate(input.hidden_binding), true, 'the SSR preview dialog starts hidden');
const clickEvent = new FakeEvent('click', image, root);
click(cx.event(clickEvent));
const opened = values(cx);
const retainedOpenSignals = signalSnapshot(cx);
assert.equal(evaluate(input.source_binding), `${input.mount}/api/attachments/9007199254740999`, 'the opened dialog uses the original URL');
assert.equal(evaluate(input.alt_binding), 'Page image', 'the opened dialog preserves authored alt text');
assert.match(evaluate(input.class_binding), /fixed inset-0 z-\[1200\].*bg-black/, 'the open binding preserves the complete overlay class');
const openId = Object.keys(before).find(id => before[id] === false && opened[id] === true);
assert.ok(openId, 'the emitted image click opens its Rust-owned preview signal');
assert.equal(evaluate(input.hidden_binding), false, 'the emitted binding makes the preview visible');
assert.ok(Object.values(opened).includes(`${input.mount}/api/attachments/9007199254740999`), 'the preview stores the original mounted URL');
assert.ok(Object.values(opened).includes('Page image'), 'the preview stores the authored alt text');
assert.equal(clickEvent.defaultPrevented, true, 'opening an attachment nested in a link prevents navigation');

const outside = new FakeImage({
  src: '/outside/thumbnail',
  'data-native-attachment-image': '',
  'data-native-original-src': '/outside/original',
  alt: 'outside',
});
click(cx.event(new FakeEvent('click', outside, root)));
assert.deepEqual(values(cx), opened, 'targets outside the root cannot change this preview');
const siblingRoot = new FakeElement();
const siblingImage = new FakeImage({
  src: '/app/api/attachments/9007199254741000/thumbnail',
  'data-native-attachment-image': '',
  'data-native-original-src': '/app/api/attachments/9007199254741000',
  alt: 'sibling',
}, siblingRoot);
click(cx.event(new FakeEvent('click', siblingImage, root)));
assert.deepEqual(values(cx), opened, 'an image owned by a sibling Markdown root cannot cross-open this preview');
const foreign = new FakeImage({src: 'https://foreign.test/image.png', alt: 'foreign'}, root);
click(cx.event(new FakeEvent('click', foreign, root)));
assert.deepEqual(values(cx), opened, 'ordinary foreign images do not open previews');

const overlay = new FakeElement({'data-native-markdown-preview': ''}, root);
const overlayImage = new FakeImage({src: '/app/api/attachments/9007199254740999', alt: 'Page image'}, overlay);
click(cx.event(new FakeEvent('click', overlayImage, root)));
assert.equal(values(cx)[openId], false, 'the overlay click closes the preview');
assert.equal(evaluate(input.hidden_binding), true, 'the preview binding hides it again');
const failed = new FakeImage({
  src: '/app/api/attachments/11/thumbnail',
  'data-native-attachment-image': '',
  'data-native-original-src': '/app/api/attachments/11',
}, root);
const succeeded = new FakeImage({
  src: '/app/api/attachments/12/thumbnail',
  'data-native-attachment-image': '',
  'data-native-original-src': '/app/api/attachments/12',
}, root, {complete: true, naturalWidth: 24});
const failedBeforeMount = new FakeImage({
  src: '/app/api/attachments/13/thumbnail',
  'data-native-attachment-image': '',
  'data-native-original-src': '/app/api/attachments/13',
}, root, {complete: true, naturalWidth: 0});
mount(cx.event(new FakeEvent('mount', root, root)));
const keydown = windowListeners.find(listener => listener.type === 'keydown');
assert.ok(keydown, 'mount installs its lifetime-scoped Escape listener');
assert.equal(keydown.options.signal, controller.signal);
const closedEscape = new FakeEvent('keydown', root, root, 'Escape');
keydown.callback(closedEscape);
assert.equal(closedEscape.defaultPrevented, false, 'Escape is untouched while this preview is closed');
click(cx.event(new FakeEvent('click', image, root)));
const escape = new FakeEvent('keydown', root, root, 'Escape');
keydown.callback(escape);
assert.equal(escape.defaultPrevented, true, 'Escape is consumed only while this preview is open');
assert.equal(values(cx)[openId], false, 'Escape closes this component-owned preview');

const errorListener = root.listeners.get('error');
assert.ok(errorListener, 'fallback uses one root-scoped capture listener');
assert.equal(errorListener.options.capture, true);
assert.equal(errorListener.options.signal, controller.signal);
assert.equal(failedBeforeMount.getAttribute('src'), '/app/api/attachments/13', 'a completed failed thumbnail is recovered during mount');
assert.equal(succeeded.getAttribute('src'), '/app/api/attachments/12/thumbnail', 'a successful thumbnail is unchanged');
const preMountWrites = failedBeforeMount.sourceWrites;
errorListener.callback(new FakeEvent('error', failedBeforeMount, root));
assert.equal(failedBeforeMount.sourceWrites, preMountWrites, 'the mount-time recovery is also one-shot');
errorListener.callback(new FakeEvent('error', failed, root));
assert.equal(failed.getAttribute('src'), '/app/api/attachments/11', 'the first thumbnail error falls back to original');
const writes = failed.sourceWrites;
errorListener.callback(new FakeEvent('error', failed, root));
assert.equal(failed.sourceWrites, writes, 'a repeated error does not loop or rewrite the original');
const outsideFailure = new FakeImage({
  src: '/app/api/attachments/16/thumbnail',
  'data-native-attachment-image': '',
  'data-native-original-src': '/app/api/attachments/16',
});
errorListener.callback(new FakeEvent('error', outsideFailure, root));
assert.equal(outsideFailure.getAttribute('src'), '/app/api/attachments/16/thumbnail', 'an image outside this root is ignored');
const retained = new FakeImage({
  src: '/app/api/attachments/14/thumbnail',
  'data-native-attachment-image': '',
  'data-native-original-src': '/app/api/attachments/14',
}, root);
click(cx.event(new FakeEvent('click', image, root)));
assert.equal(values(cx)[openId], true, 'the preview can be open before owner retirement');
click(cx.event(new FakeEvent('click', overlayImage, root)));
const beforeDispose = values(cx);
controller.abort();
const retainedEscape = new FakeEvent('keydown', root, root, 'Escape');
keydown.callback(retainedEscape);
assert.equal(retainedEscape.defaultPrevented, false, 'a retained Escape callback is inert after owner retirement');
assert.equal(values(cx)[openId], beforeDispose[openId], 'retained callbacks cannot write modal state after retirement');
errorListener.callback(new FakeEvent('error', retained, root));
click(cx.event(new FakeEvent('click', image, root)));
assert.deepEqual(values(cx), beforeDispose, 'retained click and Escape callbacks cannot write after disposal');
assert.equal(retained.getAttribute('src'), '/app/api/attachments/14/thumbnail', 'retained fallback callbacks are inert after disposal');

const remounted = handlerFixture(input.signals, () => { throw new Error('preview must not fetch'); }, input.browser_source);
remounted.context.Element = FakeElement;
remounted.context.HTMLImageElement = FakeImage;
remounted.context.window.addEventListener = () => {};
const remountRoot = new FakeElement();
const remountFailed = new FakeImage({
  src: '/ACC/api/attachments/15/thumbnail',
  'data-native-attachment-image': '',
  'data-native-original-src': '/ACC/api/attachments/15',
}, remountRoot, {complete: true, naturalWidth: 0});
vm.runInNewContext(`cx => (${input.mount_handler})`, remounted.context)(remounted.cx)(
  remounted.cx.event(new FakeEvent('mount', remountRoot, remountRoot)),
);
assert.equal(remountFailed.getAttribute('src'), '/ACC/api/attachments/15', 'a replacement owner mounts its own fallback listener');

const markdownShard = emittedShard(input.markdown_marker, context, cx, plain);
process.stdout.write(JSON.stringify({passed: true, markdown_shard: markdownShard, signals: retainedOpenSignals}));
