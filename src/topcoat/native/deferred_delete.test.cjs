const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const input = process.argv[2] ? {html:fs.readFileSync(process.argv[2], 'utf8')} : JSON.parse(fs.readFileSync(0, 'utf8'));
const {html} = input;
const mount = input.mount ?? html.match(/data-topcoat-runtime-prefix="([^"]*)"/)?.[1] ?? '';
const flush = async()=>{for(let i=0;i<8;i++)await Promise.resolve();};
const tick = async(f,amount)=>{f.advance(amount);await flush();};
const runtime = fs.readFileSync(path.join(process.cwd(), 'src/topcoat/assets/runtime.js'), 'utf8');
const decode = text => text.replace(/&(?:quot|apos|amp|lt|gt|#39);/g, value => ({'&quot;':'"','&apos;':"'",'&#39;':"'",'&amp;':'&','&lt;':'<','&gt;':'>'})[value]);
const tag = [...html.matchAll(/<div\b(?:[^"'>]|"[^"]*"|'[^']*')*>/g)]
  .map(match=>match[0]).find(tag=>/\bid="native-deferred-delete-owner"/.test(tag));
assert.ok(tag, 'The actual Rust-rendered owner is provided.');
const source = decode(tag.match(/data-topcoat-on:mount="([^"]*)"/)[1]);
const ownerKey = decode(tag.match(/data-native-delete-owner="([^"]*)"/)?.[1] || '7:ACC');
const account = ownerKey.split(':')[0];
const capturedHandles = decode(tag.match(/data-native-action-handles="([^"]*)"/)[1]);
class Node extends EventTarget {
  constructor(dataset={}) {super();this.dataset=dataset;this.isConnected=true;this.hovered=false;}
  contains(node) {return node===this||node===this.undo||node===this.close;}
  matches(selector) {assert.equal(selector, ':hover');return this.hovered;}
  getAttribute(name) {return name==='data-native-delete-owner'?this.dataset.nativeDeleteOwner:name==='data-native-action-account'?this.dataset.nativeActionAccount:null;}
  querySelector(selector) {return selector==='[data-native-toast-undo]'?this.undo:this.close;}
  querySelectorAll() {return this.toasts || [];}
}
const makeOwner = key => {
  const owner = new Node({nativeDeleteOwner:key,nativeActionAccount:key?.split(':')[0],nativeActionHandles:capturedHandles});
  owner.toasts=Array.from({length:4},(_,i)=>{
    const toast=new Node({nativeToastSlot:String(i),nativeToastId:'0'});
    toast.undo=new Node();toast.close=new Node();return toast;
  });return owner;
};
function fixture() {
  let now=0, nextTimer=0, owner=makeOwner(ownerKey), controller;
  const timers=new Map(), calls=[], navigations=[], legacy=[], pending=[];
  const document=new EventTarget();
  document.documentElement={getAttribute:()=>mount};
  document.getElementById=()=>owner;
  document.querySelector=selector=>selector.includes('data-native-toast-slot')?owner.toasts[Number(selector.match(/"(\d+)"/)[1])]:{dataset:{topcoatUsizeBits:'64'}};
  document.activeElement=null;
  const window=new EventTarget();
  const context={TextEncoder,TextDecoder,AbortController,Event,CustomEvent,document,window,queueMicrotask,
    performance:{now:()=>now},
    setTimeout:(callback,delay)=>{const id=++nextTimer;timers.set(id,{callback,at:now+delay});return id;},
    clearTimeout:id=>timers.delete(id),
    history:{pushState(_state,_title,href){legacy.push(href);}},
    fetch:(url,options)=>{calls.push({url,options});return new Promise((resolve,reject)=>pending.push({resolve,reject}));}};
  const bootstrap='var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  assert.equal(runtime.split(bootstrap).length-1,1);
  vm.runInNewContext(runtime.replace(bootstrap,'globalThis.fixture={Context:fe,Registry:ve};'),context);
  vm.runInNewContext(input.handler_source.replace(/export const (\w+)=/g,'globalThis.$1='),context);
  context.__lificNativeMounts={[input.handler_url+'#durable-actions']:context.durableActions};
  const registry=new context.fixture.Registry(), base=new context.fixture.Context(registry), signalIds=[];
  for(const match of html.matchAll(/<!--::topcoat::signal\((.*?)\)-->/gs)){
    const signal=JSON.parse(decode(match[1]));registry.insert(signal.id,base.hydrate(signal.v));signalIds.push(signal.id);
  }
  const invoke=()=>{
    controller=new AbortController();require('node:events').setMaxListeners(0,controller.signal);
    const cx=Object.assign(Object.create(base),{abortSignal:controller.signal,navigate:href=>{navigations.push(href);return Promise.resolve();}});
    const factory=vm.runInNewContext(`cx => (${source})`,context)(cx);
    factory(base.event({type:'mount',target:owner}));
    assert.equal(base.signal(JSON.parse(capturedHandles).v.activated.id).get().toString(),'true',
      'Mounting an issue owner retains the full action stack on later common routes.');
  };
  invoke();
  const click=node=>node.dispatchEvent(new Event('click'));
  const schedule=(issue=42)=>{
    const event=new Event('lific:native-issue-delete-request',{cancelable:true});
    event.detail={account_id:account,issue_id:issue,identifier:`ACC-${issue}`,list_path:'/ACC/issues',detail_path:`/ACC/issues/ACC-${issue}`};
    window.dispatchEvent(event);assert.ok(event.defaultPrevented,'The owning Rust handler accepts deletion.');
  };
  const assignModule=(issue=42,next='9',previous=null,accepted=true)=>{
    const event=new Event('lific:native-issue-module-request',{cancelable:true});
    event.detail=base.hydrate(input.module_requests[`${issue}:${next}:${previous}`]);
    window.dispatchEvent(event);
    assert.equal(event.defaultPrevented,accepted,'The durable Rust owner acknowledges accepted Module updates.');
  };
  const updateLabels=(mode='update',accepted=true)=>{
    const sample=input.module_requests['42:9:null'].v;
    const request=input.label_requests?.[mode] ?? {t:'Record',v:{
      mode,account_id:sample.account_id,issue_id:sample.issue_id,identifier:sample.identifier,
      labels:{t:'Vec',bits:64,v:['bug']},name:mode==='create'?'new label':'',color:'#2563EB',
    }};
    const event=new Event('lific:native-issue-label-request',{cancelable:true});
    event.detail=base.hydrate(request);
    window.dispatchEvent(event);
    assert.equal(event.defaultPrevented,accepted,'The durable Rust owner acknowledges accepted label updates.');
  };
  const advance=amount=>{
    const target=now+amount;
    while(true){const next=[...timers].filter(([,timer])=>timer.at<=target).sort((a,b)=>a[1].at-b[1].at)[0];if(!next)break;timers.delete(next[0]);now=next[1].at;next[1].callback();}
    now=target;
  };
  const replace=({key=ownerKey,ordinary=false,mount=true,hovered=false}={})=>{
    if(!ordinary){const event=new Event('topcoat:before-page-replace');event.detail={nextDocument:{getElementById:()=>key===null?null:makeOwner(key)}};document.dispatchEvent(event);}
    controller.abort();owner.isConnected=false;
    owner=makeOwner(key);owner.toasts[0].hovered=hovered;
    if(mount)invoke();
  };
  const finish=async(ok)=>{
    pending.shift().resolve({ok,status:403,statusText:'Forbidden',json:async()=>[{t:'i64',bits:64,v:'42'},{t:'i64',bits:64,v:'1'},{t:'i64',bits:64,v:'2'}]});
    for(let i=0;i<8;i++)await Promise.resolve();
  };
  const finishModule=async(next='9',issue=42)=>{
    pending.shift().resolve({ok:true,json:async()=>input.module_replies[`${issue}:${next}`]});await flush();
  };
  const failNetwork=async()=>{pending.shift().reject(new TypeError('offline'));await flush();};
  const failModule=async()=>{pending.shift().resolve({ok:true,json:async()=>input.module_failure});await flush();};
  const texts=()=>signalIds.map(id=>base.signal(id).get().toString());
  const snapshot=()=>Object.fromEntries(signalIds.map(id=>[id,base.signal(id).get().dehydrate()]));
  return {schedule,assignModule,updateLabels,advance,replace,finish,finishModule,failNetwork,failModule,texts,snapshot,click,calls,navigations,legacy,timers,window,get owner(){return owner;}};
}
if(input.probe_only){
  process.stdout.write(JSON.stringify(fixture().snapshot()));
  return;
}
test('Label writes start immediately without an Undo action',async()=>{
  const f=fixture();f.updateLabels();await flush();
  assert.equal(f.calls.length,1);
  assert.ok(f.calls[0].url.endsWith('/__native_issue_edit/update_labels'));
  assert.ok(f.owner.toasts.every(toast=>toast.dataset.nativeToastId==='0'));
  assert.deepEqual(f.navigations,[]);
});
test('Module updates start immediately from the durable owner',async()=>{
  const f=fixture();f.assignModule();await flush();
  assert.equal(f.calls.length,1);
  assert.ok(f.calls[0].url.endsWith('/__native_issue_edit/assign_module'));
  assert.deepEqual(f.navigations,[],'Updating metadata does not navigate.');
});
test('Module Undo survives another project and restores only the captured issue once',async()=>{
  const f=fixture();f.assignModule();await f.finishModule();
  assert.ok(f.texts().includes('ACC-42 → Release'));
  f.replace({key:'7:OTHER'});f.click(f.owner.toasts[0].undo);f.click(f.owner.toasts[0].undo);
  await flush();assert.equal(f.calls.length,2);
  assert.deepEqual(JSON.parse(f.calls[1].options.body),[input.module_requests['42:null:9']]);
  await f.finishModule(null);assert.ok(f.texts().includes('Restored ACC-42'));
  assert.deepEqual(f.navigations,[]);
});
test('Module forward and inverse replies reach the transferred host',async()=>{
  const f=fixture();f.assignModule();f.replace({key:'7:'});await f.finishModule();
  assert.ok(f.texts().includes('ACC-42 → Release'));
  f.click(f.owner.toasts[0].undo);f.replace({key:'7:OTHER'});await f.finishModule(null);
  assert.ok(f.texts().includes('Restored ACC-42'));assert.equal(f.calls.length,2);
});
test('Module updates reject duplicate pending requests and keep a failed Undo consumed',async()=>{
  const f=fixture();f.assignModule();f.assignModule(42,'9',null,false);
  assert.equal(f.calls.length,1);await f.finishModule();
  f.click(f.owner.toasts[0].undo);await f.finish(false);
  assert.ok(f.texts().some(text=>text.startsWith("Couldn't undo ACC-42: ")));
  f.click(f.owner.toasts[0].undo);assert.equal(f.calls.length,2);
});
test('Module timeout discards its inverse without writing',async()=>{
  const f=fixture();f.assignModule();await f.finishModule();await tick(f,5000);
  f.click(f.owner.toasts[0].undo);assert.equal(f.calls.length,1);
});
test('Module network failures use Main text and consume failed Undo',async()=>{
  const f=fixture();f.assignModule();await f.failNetwork();
  assert.ok(f.texts().includes("Couldn't update ACC-42: Couldn't reach the server. Check your connection and try again."));
  f.assignModule();await f.finishModule();f.click(f.owner.toasts[1].undo);await f.failNetwork();
  assert.ok(f.texts().includes("Couldn't undo ACC-42: Couldn't reach the server. Check your connection and try again."));
  f.click(f.owner.toasts[1].undo);assert.equal(f.calls.length,3);
});
test('typed Module errors preserve their reason without publishing success or offering Undo',async()=>{
  const f=fixture();let applied=0;f.window.addEventListener('lific:native-issue-module-applied',()=>applied++);
  f.assignModule();await f.failModule();
  assert.ok(f.texts().includes("Couldn't update ACC-42: Forbidden: insufficient project role"));
  assert.equal(applied,0);f.click(f.owner.toasts[0].undo);assert.equal(f.calls.length,1);
  f.assignModule();assert.equal(f.calls.length,2,'A failed forward action releases its pending state.');
});
test('clearing a Module and Undo preserve the typed nullable inverse',async()=>{
  const f=fixture();f.assignModule(42,null,'9');await f.finishModule(null);
  assert.ok(f.texts().includes('ACC-42 → No module'));
  f.click(f.owner.toasts[0].undo);assert.deepEqual(JSON.parse(f.calls[1].options.body),[input.module_requests['42:9:null']]);
  await f.finishModule();assert.ok(f.texts().includes('Restored ACC-42'));
});
test('evicting Module toasts never starts inverse or delete transport',async()=>{
  const f=fixture();
  for(let index=0;index<6;index++){f.assignModule();await f.finishModule();}
  assert.equal(f.calls.length,6);assert.equal(f.timers.size,4);
  await tick(f,5000);assert.equal(f.calls.length,6);assert.equal(f.timers.size,0);
});
test('Module hover pauses the remaining Undo deadline across navigation',async()=>{
  const f=fixture();f.assignModule();await f.finishModule();await tick(f,1000);
  f.owner.toasts[0].hovered=true;f.owner.toasts[0].dispatchEvent(new Event('mouseenter'));
  await tick(f,9000);f.replace({key:'7:',hovered:true});await tick(f,9000);
  f.owner.toasts[0].hovered=false;f.owner.toasts[0].dispatchEvent(new Event('mouseleave'));
  await tick(f,3999);f.click(f.owner.toasts[0].undo);assert.equal(f.calls.length,2);
});
test('retired account owners cannot publish late Module results',async()=>{
  const f=fixture();f.assignModule();f.replace({key:'8:ACC',mount:false});await f.finishModule();
  assert.ok(!f.texts().includes('ACC-42 → Release'));assert.equal(f.timers.size,0);
});
test('cross-project transfer claims deletion once while retaining Module Undo',async()=>{
  const f=fixture();f.assignModule(43);await f.finishModule('9',43);f.schedule(42);
  f.replace({key:'7:OTHER'});await flush();
  assert.equal(f.calls.length,2,'Leaving the project immediately claims its pending delete.');
  assert.ok(f.calls[1].url.endsWith('/__native_issue_edit/delete'));
  assert.equal(f.calls[1].options.keepalive,true);
  await f.finish(true);
  f.replace({key:'7:'});await flush();
  assert.equal(f.calls.length,2,'Further navigation cannot claim that deletion again.');
  f.click(f.owner.toasts[0].undo);await flush();
  assert.equal(f.calls.length,3,'The Module inverse survives the project and common-page transfer.');
  assert.deepEqual(JSON.parse(f.calls[2].options.body),[input.module_requests['43:null:9']]);
  await f.finishModule(null,43);assert.ok(f.texts().includes('Restored ACC-43'));
  await tick(f,10000);assert.equal(f.calls.length,3);
});
test('delete and Module actions share slots without crossing their Undo behavior',async()=>{
  const f=fixture();f.schedule(42);f.assignModule(43);await f.finishModule('9',43);
  f.click(f.owner.toasts[1].undo);await f.finishModule(null,43);
  f.click(f.owner.toasts[0].undo);await tick(f,10000);
  assert.equal(f.calls.length,2,'Delete Undo cancels; Module Undo writes exactly once.');
});
test('native same-workspace replacement preserves Undo and its original deadline',async()=>{
  const f=fixture();f.schedule();assert.equal(f.legacy.length,0,'Delete actions delegate to native navigation.');assert.deepEqual(f.navigations,[`${mount}/ACC/issues`]);
  await tick(f,2000);f.replace();assert.equal(f.calls.length,0);assert.equal(f.timers.size,1);
  await tick(f,2999);assert.equal(f.calls.length,0);await tick(f,1);assert.equal(f.calls.length,1);assert.equal(f.calls[0].options.keepalive,undefined);
});
test('Undo after owner transfer never starts delete transport',async()=>{
  const f=fixture();f.schedule();await tick(f,1200);f.replace();f.click(f.owner.toasts[0].undo);
  assert.deepEqual(f.navigations,[`${mount}/ACC/issues`,`${mount}/ACC/issues/ACC-42`]);await tick(f,10000);assert.equal(f.calls.length,0);
});
test('paused deletion retains remaining time across replacement and resumes on the new host',async()=>{
  const f=fixture();f.schedule();await tick(f,1000);f.owner.toasts[0].hovered=true;f.owner.toasts[0].dispatchEvent(new Event('mouseenter'));await tick(f,9000);
  f.replace();await tick(f,3999);assert.equal(f.calls.length,0);await tick(f,1);assert.equal(f.calls.length,1);
});
test('leaving the workspace claims keepalive exactly once before scope teardown',async()=>{
  for(const key of ['7:OTHER','8:ACC',null]){
    const f=fixture();f.schedule();f.replace({key,mount:false});assert.equal(f.calls.length,1);assert.equal(f.calls[0].options.keepalive,true);
    f.window.dispatchEvent(new Event('pagehide'));assert.equal(f.calls.length,1);
  }
});
test('a held failure follows transferred ownership but a retired owner stays silent',async()=>{
  const f=fixture();f.schedule();await tick(f,5000);assert.equal(f.calls.length,1);f.replace();await f.finish(false);
  assert.deepEqual(f.navigations,[`${mount}/ACC/issues`,`${mount}/ACC/issues/ACC-42`]);
  const retired=fixture();retired.schedule();await tick(retired,5000);retired.replace({ordinary:true});await retired.finish(false);
  assert.deepEqual(retired.navigations,[`${mount}/ACC/issues`]);
});

test('a still-hovered transferred toast stays paused until its new host resumes',async()=>{
  const f=fixture();f.schedule();await tick(f,1000);f.owner.toasts[0].hovered=true;
  f.owner.toasts[0].dispatchEvent(new Event('mouseenter'));f.replace({hovered:true});
  await tick(f,9000);assert.equal(f.calls.length,0);
  f.owner.toasts[0].hovered=false;f.owner.toasts[0].dispatchEvent(new Event('mouseleave'));
  await tick(f,3999);assert.equal(f.calls.length,0);await tick(f,1);assert.equal(f.calls.length,1);
});
test('focus pause ends when replacement releases the focused control',async()=>{
  const f=fixture();f.schedule();await tick(f,1000);f.owner.toasts[0].dispatchEvent(new Event('focusin'));
  await tick(f,9000);f.replace();await tick(f,3999);assert.equal(f.calls.length,0);
  await tick(f,1);assert.equal(f.calls.length,1);
});
test('superseding a deletion commits the old batch once and preserves the newest Undo',async()=>{
  const f=fixture();f.schedule(42);await tick(f,1000);f.schedule(43);await flush();assert.equal(f.calls.length,1);
  f.replace();await f.finish(true);f.click(f.owner.toasts[1].undo);await tick(f,10000);
  assert.equal(f.calls.length,1);assert.equal(f.navigations.at(-1),`${mount}/ACC/issues/ACC-43`);
});
test('pagehide claims the current pending deletion once with genuine keepalive',async()=>{
  const f=fixture();f.schedule();f.window.dispatchEvent(new Event('pagehide'));
  f.window.dispatchEvent(new Event('pagehide'));assert.equal(f.calls.length,1);assert.equal(f.calls[0].options.keepalive,true);
});
