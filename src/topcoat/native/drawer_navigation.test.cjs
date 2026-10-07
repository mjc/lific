const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {getEventListeners, setMaxListeners} = require('node:events');
const {TextEncoder, TextDecoder} = require('node:util');
const {source, mount} = JSON.parse(fs.readFileSync(0,'utf8'));
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js','utf8');
const flush = async()=>{for(let i=0;i<12;i++)await Promise.resolve();};
class CustomEvent extends Event {constructor(type,options={}){super(type,options);this.detail=options.detail;}}

function fixture() {
  const base=`http://localhost${mount}/ACC/issues`, root={}, document=new EventTarget(), window=new EventTarget();
  const scope=new AbortController();setMaxListeners(0,scope.signal);
  const timers=new Map(), goes=[], commits=[];let timerId=0;
  const location={href:base,assign:()=>{throw new Error('Unexpected hard navigation');},reload:()=>{throw new Error('Unexpected reload');}};
  const history={state:{lificNativeHomeNav:{version:'1',owner:'fixture-owner',href:base,pane:'closed',project:''}},go:steps=>goes.push(Number(steps))};
  const media=new EventTarget();media.matches=false;
  window.location=location;window.matchMedia=()=>media;
  document.documentElement={getAttribute:()=>mount,setAttribute:()=>{}};
  document.querySelector=selector=>selector==='.native-home-shell'?root:null;
  document.getElementById=()=>null;
  const context={TextEncoder,TextDecoder,AbortController,Event,CustomEvent,DOMException,document,window,location,history,
    crypto:require('node:crypto').webcrypto,queueMicrotask,localStorage:{getItem:()=>null},
    setTimeout:(callback,delay)=>{const id=++timerId;timers.set(id,{callback,delay});return id;},clearTimeout:id=>timers.delete(id)};
  const bootstrap='var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  assert.equal(runtime.split(bootstrap).length-1,1);
  vm.runInNewContext(runtime.replace(bootstrap,'globalThis.fixture={Context:fe,Registry:ve,before:topcoatBeforeNavigationCommit};'),context);
  const registry=new context.fixture.Registry(), cx=Object.assign(new context.fixture.Context(registry),{abortSignal:scope.signal});
  let nextId=0;
  const hydrate=value=>cx.hydrate(value);
  const signal=value=>{const id=`fixture-${++nextId}`;registry.insert(id,hydrate(value));return cx.signal(id);};
  const usize=value=>({t:'usize',bits:64,v:String(value)});
  const chrome=[signal(false),signal('system'),signal(false),signal(false),signal('root'),signal(''),signal('fixture-owner'),signal(base),signal(false),signal(''),signal(false),signal('')];
  const palette=[signal(false),signal(''),signal(''),signal(usize(0)),signal(usize(0)),signal(usize(0)),signal(usize(0)),signal(''),signal(false),signal(usize(0)),signal(false),signal(false)];
  const status=[signal(false),signal(''),signal('')];
  const request=[hydrate('|"ACC"|'),hydrate(`${mount}/login`),hydrate({t:'i64',bits:64,v:'7'}),hydrate(false)];
  vm.runInNewContext(source.replace(/export const (\w+)=/g,'globalThis.$1='),context);
  context.mount(cx,{},cx.tuple(chrome),cx.tuple(palette),cx.tuple(status),cx.tuple(request));
  const open=(pane='root')=>{
    chrome[3].set(hydrate(true));chrome[4].set(hydrate(pane));chrome[5].set(hydrate(pane==='root'?'':'ACC'));
    history.state={lificNativeHomeNav:{version:'1',owner:'fixture-owner',href:base,pane,project:pane==='root'?'':'ACC'}};
  };
  const navigate=(mode='push',path='/ACC/board')=>{
    const controller=new AbortController();let current=true;
    const completion=context.fixture.before({signal:controller.signal,mode,url:new URL(`http://localhost${mount}${path}`),nextDocument:{}},()=>current)
      .then(ready=>{if(ready)commits.push(path);return ready;});
    return {controller,completion,supersede:()=>{current=false;controller.abort();}};
  };
  const pop=(foreign=false)=>{
    history.state={lificNativeHomeNav:{version:'1',owner:foreign?'foreign':'fixture-owner',href:base,pane:'closed',project:''}};
    window.dispatchEvent(new Event('popstate'));
  };
  return {open,navigate,pop,scope,timers,goes,commits,chrome,history,location,window,base,
    listeners:()=>getEventListeners(window,'popstate').length};
}

test('push waits for root/project drawer entries to unwind before committing',async()=>{
  for(const pane of ['root','project']) {
    const f=fixture(), listeners=f.listeners();f.open(pane);
    const navigation=f.navigate();await flush();
    assert.deepEqual(f.goes,[pane==='root'?-1:-2]);assert.deepEqual(f.commits,[]);
    assert.equal(f.listeners(),listeners+1);assert.equal(f.timers.size,1);
    f.pop();assert.equal(await navigation.completion,true);
    assert.deepEqual(f.commits,['/ACC/board']);assert.equal(f.listeners(),listeners);assert.equal(f.timers.size,0);
  }
});

test('traversal never unwinds the outgoing drawer or checks its old history entry',async()=>{
  const f=fixture();f.open('project');f.location.href=`http://localhost${mount}/ACC/board`;
  f.history.state={topcoat:{scroll:null}};
  assert.equal(await f.navigate('traverse').completion,true);
  assert.deepEqual(f.goes,[]);assert.equal(f.timers.size,0);assert.deepEqual(f.commits,['/ACC/board']);
});

test('a superseding target shares one unwind and only the current navigation commits',async()=>{
  const f=fixture();f.open('project');const first=f.navigate('push','/ACC/board');await flush();
  first.supersede();const next=f.navigate('push','/ACC/issues/ACC-1');await flush();
  assert.deepEqual(f.goes,[-2]);assert.equal(await first.completion,false);
  f.pop();assert.equal(await next.completion,true);assert.deepEqual(f.commits,['/ACC/issues/ACC-1']);assert.equal(f.timers.size,0);
});

test('scope abort cleans pending history resources and prevents commit',async()=>{
  const f=fixture();f.open();const navigation=f.navigate();await flush();
  navigation.supersede();f.scope.abort();await flush();
  assert.equal(await navigation.completion,false);assert.equal(f.listeners(),0);assert.equal(f.timers.size,0);assert.deepEqual(f.commits,[]);
});

test('canceling navigation retains a working current page after its drawer finishes closing',async()=>{
  const f=fixture(), listeners=f.listeners();f.open();const canceled=f.navigate();await flush();
  canceled.supersede();assert.equal(await canceled.completion,false);f.pop();await flush();
  assert.equal(f.chrome[3].get().toString(),'false');assert.equal(f.listeners(),listeners);assert.equal(f.timers.size,0);
  assert.equal(await f.navigate('push','/ACC/issues/ACC-1').completion,true);
  assert.deepEqual(f.commits,['/ACC/issues/ACC-1']);assert.deepEqual(f.goes,[-1]);
});

test('foreign history aborts the pending commit and removes the unwind listener',async()=>{
  const f=fixture(), listeners=f.listeners();f.open();const navigation=f.navigate();await flush();f.pop(true);
  assert.equal(await navigation.completion,false);assert.deepEqual(f.commits,[]);
  assert.equal(f.listeners(),listeners);assert.equal(f.timers.size,0);
});
