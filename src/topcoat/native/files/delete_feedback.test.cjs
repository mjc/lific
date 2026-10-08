'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('../handler_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const requests = [];
const notifications = [];
let settleFetch;
const fixture = handlerFixture(input.signals, async (url, options) => {
  requests.push({path:new URL(url,'http://localhost').pathname,body:JSON.parse(options.body)});
  if (input.phase === 'late_success' || input.phase === 'late_rejection') {
    return new Promise((resolve,reject) => { settleFetch=reply => input.phase==='late_rejection'
      ? reject(new Error('offline'))
      : resolve({ok:true,status:200,json:async()=>reply}); });
  }
  if (input.phase === 'transport_rejection') throw new Error('network unavailable');
  return {ok:input.phase!=='http_rejection',status:input.phase==='http_rejection'?403:200,json:async()=>input.reply};
},input.browser_source);
fixture.context.document.documentElement.getAttribute = name =>
  name === 'data-topcoat-runtime-prefix' ? input.mount || '' : '';
fixture.context.CustomEvent = class extends Event {
  constructor(type,options={}) { super(type,options); this.detail=options.detail; }
};
fixture.context.window.dispatchEvent = event => {
  if (event.type==='lific:native-toast-success'||event.type==='lific:native-toast-error')
    notifications.push({type:event.type,detail:JSON.parse(JSON.stringify(event.detail.dehydrate()))});
  return true;
};
const button = {
  getAttribute(name) {
    if (name==='data-native-files-confirm-delete') return input.button?.id ?? null;
    if (name==='data-native-files-delete-success') return input.button?.success ?? null;
    return null;
  },
  closest(selector) { return selector==='button[data-native-files-confirm-delete]' ? this : null; },
};
const icon = {closest:selector=>selector==='button[data-native-files-confirm-delete]'?button:null};
const root = {contains:node => !input.detached && node===button};
const target = input.nested_icon ? icon : button;
const event = fixture.cx.event({type:'click',target,currentTarget:root});
const run = source => fixture.handler(source)(event);
const ids = Object.keys(input.signals);
const scalar = wire => { while (wire && typeof wire==='object' && Object.hasOwn(wire,'v')) wire=wire.v; return wire; };
const numericSurrogate = value => value && typeof value==='object' &&
  ['i64','usize','f64'].includes(value.t) && (typeof value.v==='number' || (typeof value.v==='string' && /^-?\d+$/.test(value.v)));
const numericSnapshot = () => Object.fromEntries(ids.map(id=>[id,fixture.cx.signal(id).get().dehydrate()])
  .filter(([,wire])=>numericSurrogate(wire)));

async function flush() { for(let i=0;i<80;i++) await Promise.resolve(); }
async function main() {
  const numericBefore=numericSnapshot();
  const allBefore=Object.fromEntries(ids.map(id=>[id,fixture.cx.signal(id).get().dehydrate()]));
  if(input.phase==='parent_disposed') fixture.controller.abort();
  run(input.phase==='open' ? input.handler : input.root_handler);
  if(input.phase==='queued_dispose') fixture.controller.abort();
  if(input.phase==='late_success'||input.phase==='late_rejection') {
    for(let i=0;i<5;i++) await Promise.resolve();
    assert.equal(requests.length,1);
    assert.equal(requests[0].path,`${input.mount||''}/__native_files/delete`);
    assert.deepEqual(requests[0].body,input.expected_arguments);
    fixture.controller.abort();
    const before=Object.fromEntries(ids.map(id=>[id,fixture.cx.signal(id).get().dehydrate()]));
    settleFetch(input.reply);
    await flush();
    const after=Object.fromEntries(ids.map(id=>[id,fixture.cx.signal(id).get().dehydrate()]));
    assert.deepEqual(after,before,'navigation retirement makes late outcomes inert');
    assert.deepEqual(notifications,[]);
    process.stdout.write(JSON.stringify({requests,notifications,signals:after})); return;
  }
  await flush();
  if(input.phase==='detached'||input.phase==='parent_disposed'||input.phase==='queued_dispose') {
    assert.deepEqual(requests,[],'detached intents and queued disposed roots do not start a request');
    assert.deepEqual(notifications,[]);
    if(input.phase==='queued_dispose') assert.deepEqual(numericSnapshot(),numericBefore,'queued disposal does not mutate counters');
    if(input.phase==='detached'||input.phase==='parent_disposed') assert.deepEqual(
      Object.fromEntries(ids.map(id=>[id,fixture.cx.signal(id).get().dehydrate()])),allBefore,
      'detached targets and retired page owners are inert');
  } else if(['service_error','transport_rejection','http_rejection'].includes(input.phase)) {
    assert.equal(requests.length,1);
    assert.equal(requests[0].path,`${input.mount||''}/__native_files/delete`);
    assert.deepEqual(requests[0].body,input.expected_arguments);
    assert.deepEqual(notifications,[{type:'lific:native-toast-error',detail:input.error_request}]);
    assert.deepEqual(numericSnapshot(),numericBefore,'failure does not advance list revisions or offsets');
  } else if(input.phase==='delete') {
    assert.equal(requests.length,1);
    assert.equal(requests[0].path,`${input.mount||''}/__native_files/delete`);
    assert.deepEqual(requests[0].body,input.expected_arguments);
    assert.deepEqual(notifications,[{type:'lific:native-toast-success',detail:input.success_request}]);
  } else if(input.phase==='open') {
    assert.equal(requests.length,0);
    assert.deepEqual(notifications,[]);
  }
  const signals=Object.fromEntries(ids.map(id=>[id,fixture.cx.signal(id).get().dehydrate()]));
  process.stdout.write(JSON.stringify({requests,notifications,signals}));
}
main().catch(error=>{process.stderr.write(`${error.stack}\n`);process.exitCode=1;});
