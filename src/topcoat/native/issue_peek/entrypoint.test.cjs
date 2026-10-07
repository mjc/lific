'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder, isDeepStrictEqual} = require('node:util');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js', 'utf8');
const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
assert.equal(runtime.split(bootstrap).length - 1, 1);
const eventClass = runtime.match(/event\(\w+\)\{return new (\w+)\(\w+\)\}/)?.[1];
const listeners = new Map();
const controller = new AbortController();
let prevented = 0, stopped = 0;
const context = {
  TextEncoder, TextDecoder, queueMicrotask,
  CustomEvent: class {constructor(type, options) {this.type=type;this.detail=options.detail;}},
  document: {
    documentElement: {getAttribute: () => '/app'},
    addEventListener(type, handler, options) {
      listeners.set(type, handler);
      options?.signal.addEventListener('abort',()=>listeners.delete(type),{once:true});
    },
    dispatchEvent(event) {listeners.get(event.type)?.(event);},
  },
  window: {addEventListener(type, handler, options) {
    listeners.set(`window:${type}`,handler);
    options?.signal.addEventListener('abort',()=>listeners.delete(`window:${type}`),{once:true});
  }},
  innerWidth: 1024,
  matchMedia: () => ({matches:true}),
  fetch() {throw new Error('Peek entrypoint must not use the API');},
};
vm.runInNewContext(runtime.replace(bootstrap,
  `globalThis.fixture={Context:fe,Registry:ve,Event:${eventClass}};`), context);
const registry = new context.fixture.Registry();
const cx = Object.assign(new context.fixture.Context(registry), {
  abortSignal: controller.signal,
  event: event => new context.fixture.Event(event),
});
for (const [id, value] of Object.entries(input.signals)) registry.insert(id,cx.hydrate(value));
const handler = source => vm.runInNewContext(`cx => (${source})`,context)(cx);
const click = () => cx.event({type:'click',preventDefault(){prevented++;},stopPropagation(){stopped++;}});
const snapshot = () => Object.fromEntries(Object.keys(input.signals).map(id=>[id,cx.signal(id).dehydrate().v]));
const baseline = snapshot();
// Rust merges only changed signals, preserving unrelated wire values exactly.
const changes = () => Object.fromEntries(Object.entries(snapshot())
  .filter(([id, value]) => !isDeepStrictEqual(value, baseline[id])));
if(input.dismiss) {
  const element = name => ({name, disabled:false, hidden:false, isConnected:true,
    getClientRects:()=>[{}],closest:()=>null,
    focus(){context.document.activeElement=this;},
  });
  const trigger=element('trigger'), first=element('close'), last=element('open-full-view');
  const controls=[first,last];
  const panel={
    querySelector:()=>first, querySelectorAll:()=>controls,
    contains:node=>controls.includes(node), animate(){},
  };
  context.document.activeElement=trigger;
  handler(input.dismiss)(cx.event({type:'mount',target:panel}));
  assert.equal(context.document.activeElement,first,'sheet takes focus from the trigger');
  const key=(key,shiftKey=false)=>listeners.get('window:keydown')({key,shiftKey,preventDefault(){prevented++;}});
  last.focus(); key('Tab');
  assert.equal(context.document.activeElement,first,'Tab wraps at the end');
  key('Tab',true);
  assert.equal(context.document.activeElement,last,'Shift-Tab wraps at the start');
  trigger.focus(); key('Tab');
  assert.equal(context.document.activeElement,first,'focus outside is brought into the sheet');
  key('Escape');
  controller.abort();
  assert.equal(context.document.activeElement,trigger,'dismissal restores trigger focus');
  process.stdout.write(JSON.stringify(changes()));
} else if(input.action) {
  if(input.owner) handler(input.owner)(cx.event({type:'mount'}));
  handler(input.action)(click());
  process.stdout.write(JSON.stringify(changes()));
} else {
  handler(input.owner)(cx.event({type:'mount'}));
  handler(input.trigger)(click());
  assert.equal(prevented,1,'Peek does not navigate');
  assert.equal(stopped,1,'Peek does not activate its row');
  const opened=changes();
  const current=snapshot();
  const changed=Object.keys(current).filter(id=>!isDeepStrictEqual(current[id],baseline[id]));
  assert.equal(changed.length,1);
  const selection=changed[0];
  context.document.dispatchEvent({type:'topcoat:before-navigation-commit'});
  assert.equal(snapshot()[selection],'','navigation dismisses the sheet');
  controller.abort();
  handler(input.trigger)(click());
  assert.equal(snapshot()[selection],'','disposed owner cannot reopen');
  assert.equal(listeners.size,0,'owner listeners are removed on disposal');
  process.stdout.write(JSON.stringify(opened));
}
