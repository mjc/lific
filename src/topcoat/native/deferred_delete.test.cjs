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
const tag = html.match(/<div id="native-deferred-delete-owner"[^>]*>/)?.[0];
assert.ok(tag, 'The actual Rust-rendered owner is provided.');
const source = decode(tag.match(/data-topcoat-on:mount="([^"]*)"/)[1]);
const ownerKey = decode(tag.match(/data-native-delete-owner="([^"]*)"/)?.[1] || '7:ACC');
const account = ownerKey.split(':')[0];
class Node extends EventTarget {
  constructor(dataset={}) {super();this.dataset=dataset;this.isConnected=true;this.hovered=false;}
  contains(node) {return node===this||node===this.undo||node===this.close;}
  matches(selector) {assert.equal(selector, ':hover');return this.hovered;}
  getAttribute(name) {return name==='data-native-delete-owner'?this.dataset.nativeDeleteOwner:null;}
  querySelector(selector) {return selector==='[data-native-toast-undo]'?this.undo:this.close;}
  querySelectorAll() {return this.toasts || [];}
}
const makeOwner = key => {
  const owner = new Node({nativeDeleteOwner:key});
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
  const context={TextEncoder,TextDecoder,AbortController,Event,document,window,queueMicrotask,
    performance:{now:()=>now},
    setTimeout:(callback,delay)=>{const id=++nextTimer;timers.set(id,{callback,at:now+delay});return id;},
    clearTimeout:id=>timers.delete(id),
    history:{pushState(_state,_title,href){legacy.push(href);}},
    fetch:(url,options)=>{calls.push({url,options});return new Promise(resolve=>pending.push(resolve));}};
  const bootstrap='var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  assert.equal(runtime.split(bootstrap).length-1,1);
  vm.runInNewContext(runtime.replace(bootstrap,'globalThis.fixture={Context:fe,Registry:ve};'),context);
  const registry=new context.fixture.Registry(), base=new context.fixture.Context(registry);
  for(const match of html.matchAll(/<!--::topcoat::signal\((.*?)\)-->/gs)){
    const signal=JSON.parse(decode(match[1]));registry.insert(signal.id,base.hydrate(signal.v));
  }
  const invoke=()=>{
    controller=new AbortController();require('node:events').setMaxListeners(0,controller.signal);
    const cx=Object.assign(Object.create(base),{abortSignal:controller.signal,navigate:href=>{navigations.push(href);return Promise.resolve();}});
    const factory=vm.runInNewContext(`cx => (${source})`,context)(cx);
    factory({});
  };
  invoke();
  const click=node=>node.dispatchEvent(new Event('click'));
  const schedule=(issue=42)=>{
    const event=new Event('lific:native-issue-delete-request',{cancelable:true});
    event.detail={account_id:account,issue_id:issue,identifier:`ACC-${issue}`,list_path:'/ACC/issues',detail_path:`/ACC/issues/ACC-${issue}`};
    window.dispatchEvent(event);assert.ok(event.defaultPrevented,'The owning Rust handler accepts deletion.');
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
    pending.shift()({ok,status:403,statusText:'Forbidden',json:async()=>[{t:'i64',bits:64,v:'42'},{t:'i64',bits:64,v:'1'},{t:'i64',bits:64,v:'2'}]});
    for(let i=0;i<8;i++)await Promise.resolve();
  };
  return {schedule,advance,replace,finish,click,calls,navigations,legacy,timers,window,get owner(){return owner;}};
}
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
