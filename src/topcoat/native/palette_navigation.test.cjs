const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {setMaxListeners} = require('node:events');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0,'utf8'));
const {mount,source} = input;
const runtime = fs.readFileSync('src/topcoat/assets/runtime.js','utf8');
const decode = text => text.replace(/&(?:quot|apos|amp|lt|gt|#39);/g,value=>({'&quot;':'"','&apos;':"'",'&amp;':'&','&lt;':'<','&gt;':'>' ,'&#39;':"'"})[value]);
const attrs = tag => Object.fromEntries([...tag.matchAll(/([\w:-]+)="([^"]*)"/g)].map(([,key,value])=>[key,decode(value)]));
// Rust-generated expressions can contain a literal > inside quoted attributes.
const tags = html => [...html.matchAll(/<([a-z]+)\b(?:[^"'>]|"[^"]*"|'[^']*')*>/g)];

function fixture(markup) {
  const document=new EventTarget(), window=new EventTarget(), scope=new AbortController();setMaxListeners(0,scope.signal);
  const elements=tags(markup.html);
  const nav=elements.filter(match=>match[1]==='nav').map(match=>attrs(match[0])).find(row=>row.class==='native-home-palette-results');
  const rows=elements.filter(match=>match[1]==='a').map(match=>attrs(match[0])).filter(row=>'data-palette-index' in row);
  const root={}, navigations=[], tabs=[], historyMoves=[], focused=[], media=new EventTarget();media.matches=false;
  const history={state:null,back:()=>historyMoves.push(-1),go:steps=>historyMoves.push(Number(steps))};
  const focusItems=['first','last'].map(id=>({
    focus:()=>focused.push(id),getClientRects:()=>[{}],closest:()=>null,
  }));
  const pane={querySelectorAll:()=>focusItems,contains:node=>focusItems.includes(node)};
  const location={href:`http://localhost${mount}/`,assign:()=>{throw new Error('Unexpected hard navigation');}};
  const node=row=>({getAttribute:name=>row[name],scrollIntoView:()=>{}});
  document.documentElement={getAttribute:()=>mount,setAttribute:()=>{}};
  document.getElementById=id=>({focus:()=>focused.push(id)});
  document.querySelectorAll=()=>[];
  document.querySelector=selector=>{
    if(selector==='.native-home-shell')return root;
    if(selector==='[data-native-mobile-root]')return pane;
    if(selector==='[data-native-mobile-root] button')return focusItems[0];
    if(selector==='.native-home-palette-results')return node(nav);
    const index=selector.match(/data-palette-index="(\d+)"/);
    return index?node(rows.find(row=>row['data-palette-index']===index[1])):null;
  };
  window.location=location;window.matchMedia=()=>media;window.open=(...args)=>tabs.push(args);
  const context={TextEncoder,TextDecoder,AbortController,DOMException,Event,document,window,location,queueMicrotask,
    history,crypto:require('node:crypto').webcrypto,localStorage:{getItem:()=>null},setTimeout,clearTimeout};
  const bootstrap='var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  assert.equal(runtime.split(bootstrap).length-1,1);
  vm.runInNewContext(runtime.replace(bootstrap,'globalThis.fixture={Context:fe,Registry:ve};'),context);
  const registry=new context.fixture.Registry(),cx=Object.assign(new context.fixture.Context(registry),{
    abortSignal:scope.signal,navigate:href=>{navigations.push(href);return Promise.resolve();}
  });
  for(const match of markup.html.matchAll(/<!--::topcoat::signal\((.*?)\)-->/gs)){
    const value=JSON.parse(decode(match[1]));registry.insert(value.id,cx.hydrate(value.v));
  }
  // The last signal is read only by the server to scope issue search.
  const state=markup.state.slice(0,10).map(value=>cx.hydrate(value));
  let nextId=0;
  const signal=value=>{const id=`extra-${++nextId}`;registry.insert(id,cx.hydrate(value));return cx.signal(id);};
  const usize=value=>({t:'usize',bits:64,v:String(value)});
  const chrome=[signal(false),signal('system'),signal(false),signal(false),signal('root'),signal(''),signal(''),signal(location.href),signal(false),signal(''),signal(false),signal('')];
  const palette=[state[9],signal(''),signal(''),state[0],signal(usize(0)),state[5],state[1],state[2],state[3],state[4],state[6],state[7]];
  const status=[state[8],signal(''),signal('native-home-palette-open')];
  vm.runInNewContext(source.replace(/export const (\w+)=/g,'globalThis.$1='),context);
  context.mount(cx,{},cx.tuple(chrome),cx.tuple(palette),cx.tuple(status),
    cx.tuple([cx.hydrate('|"ACC"|"DCS"|'),cx.hydrate(`${mount}/login`),cx.hydrate({t:'i64',bits:64,v:'1'}),cx.hydrate(false)]));
  const invoke=code=>vm.runInNewContext(`cx=>(${code})`,context)(cx)({});
  const hover=()=>invoke(rows.find(row=>row.href===`${mount}/DCS/overview`)['data-topcoat-on:mouseenter']);
  const mounted=()=>invoke(nav['data-topcoat-on:mount']);
  const key=(value,{id='native-home-palette-query',metaKey=false,ctrlKey=false,shiftKey=false}={})=>{
    const event=new Event('keydown',{cancelable:true});
    Object.defineProperties(event,{key:{value},target:{value:{id}},metaKey:{value:metaKey},ctrlKey:{value:ctrlKey},shiftKey:{value:shiftKey}});
    window.dispatchEvent(event);
    return event;
  };
  const enter=(metaKey=false,ctrlKey=false)=>assert.equal(key('Enter',{metaKey,ctrlKey}).defaultPrevented,true);
  const typeInput=(id,value)=>{
    const event=new Event('input');
    Object.defineProperty(event,'target',{value:{id,value}});
    window.dispatchEvent(event);
  };
  const restoreMobile=()=>{
    history.state={lificNativeHomeNav:{version:'1',owner:chrome[6].get().toString(),href:location.href,pane:'root',project:''}};
    window.dispatchEvent(new Event('popstate'));
  };
  const click=id=>{
    const event=new Event('click');
    Object.defineProperty(event,'target',{value:{closest:()=>id?{id}:null}});
    window.dispatchEvent(event);
  };
  return {hover,mounted,enter,key,typeInput,restoreMobile,click,state,query:palette[1],chrome,history,historyMoves,focused,focusItems,window,document,navigations,tabs,scope};
}

test('hover retains mounted row identity when a refresh inserts preceding results',()=>{
  const first=fixture(input.first);first.hover();
  const hovered=first.state[2].get().toString();
  const next=fixture(hovered===`${mount}/DCS/overview`?input.refreshedMounted:input.refreshedLogical);
  next.mounted();
  assert.equal(next.state[1].get().toString(),'1','DCS remains selected after ACC is inserted before it.');
  assert.equal(next.state[2].get().toString(),`${mount}/DCS/overview`);
  first.scope.abort();next.scope.abort();
});

test('Enter, Meta Enter, and Ctrl Enter navigate the mounted hovered anchor',()=>{
  for(const [metaKey,ctrlKey] of [[false,false],[true,false],[false,true]]){
    const f=fixture(input.first);f.mounted();f.hover();f.enter(metaKey,ctrlKey);
    if(metaKey||ctrlKey)assert.deepEqual(f.tabs,[[`${mount}/DCS/overview`,'_blank','noopener']]);
    else assert.deepEqual(f.navigations,[`${mount}/DCS/overview`]);
    f.scope.abort();
  }
});

test('palette input remains a serializable Topcoat string and ignores unrelated inputs',()=>{
  const f=fixture(input.first);
  try {
    f.typeInput('other-query','unrelated');
    assert.equal(f.query.get().dehydrate(),'');
    f.typeInput('native-home-palette-query','Quoted "project" & 日本語');
    assert.equal(f.query.get().dehydrate(),'Quoted "project" & 日本語');
    assert.equal(f.state[8].get().toString(),'true','a new query waits for authorization');
  } finally {
    // Stop the owner before its queued authorization request starts.
    f.scope.abort();
  }
});

test('arrow keys clamp selection and only consume input events owned by the palette',()=>{
  const f=fixture(input.refreshedMounted);
  try {
    f.mounted();
    const selected=()=>f.state[1].get().toString();
    assert.equal(f.key('ArrowUp').defaultPrevented,true);
    assert.equal(selected(),'0');
    assert.equal(f.key('ArrowDown',{id:'other-query'}).defaultPrevented,false);
    assert.equal(selected(),'0');
    assert.equal(f.key('ArrowDown').defaultPrevented,true);
    assert.equal(selected(),'1');
    f.key('ArrowDown');
    assert.equal(selected(),'1','selection stops at the last result');
    f.key('ArrowUp');
    assert.equal(selected(),'0');
  } finally { f.scope.abort(); }
});

test('Escape closes the palette and restores the opener without navigating',async()=>{
  const f=fixture(input.first);
  try {
    f.mounted();
    assert.equal(f.key('Escape').defaultPrevented,true);
    assert.equal(f.state[9].get().toString(),'false');
    await Promise.resolve();
    assert.deepEqual(f.focused,['native-home-palette-open']);
    assert.deepEqual(f.navigations,[]);
  } finally { f.scope.abort(); }
});

test('popstate restores an owned mobile drawer and Escape unwinds its history',()=>{
  const f=fixture(input.first);
  try {
    f.restoreMobile();
    assert.equal(f.chrome[3].get().toString(),'true');
    assert.equal(f.chrome[4].get().toString(),'root');
    assert.equal(f.key('Escape').defaultPrevented,true);
    assert.deepEqual(f.historyMoves,[-1]);
    f.history.state=null;
    f.window.dispatchEvent(new Event('hashchange'));
    assert.equal(f.chrome[3].get().toString(),'false');
  } finally { f.scope.abort(); }
});

test('navigation traversal retains the current mobile history entry',()=>{
  const f=fixture(input.first);
  try {
    f.restoreMobile();
    const event=new Event('topcoat:before-navigation-commit');
    Object.defineProperty(event,'detail',{value:{mode:'traverse',waitUntil:()=>assert.fail('traversal must not unwind the drawer')}});
    f.document.dispatchEvent(event);
    assert.deepEqual(f.historyMoves,[]);
    assert.equal(f.chrome[3].get().toString(),'true');
  } finally { f.scope.abort(); }
});

test('delegated clicks close and reopen the palette through a nested target',()=>{
  const f=fixture(input.first);
  try {
    f.click('');
    assert.equal(f.state[9].get().toString(),'true');
    f.click('native-home-palette-close');
    assert.equal(f.state[9].get().toString(),'false');
    f.click('native-home-quick-jump');
    assert.equal(f.state[9].get().toString(),'true');
    assert.equal(f.state[8].get().toString(),'true');
    assert.equal(f.query.get().dehydrate(),'');
  } finally { f.scope.abort(); }
});

test('mobile Tab and Shift Tab wrap focus and focusin contains outside focus',async()=>{
  const f=fixture(input.first);
  try {
    f.restoreMobile();
    await Promise.resolve();
    f.focused.length=0;
    f.document.activeElement=f.focusItems[1];
    assert.equal(f.key('Tab').defaultPrevented,true);
    assert.deepEqual(f.focused,['first']);
    f.document.activeElement=f.focusItems[0];
    assert.equal(f.key('Tab',{shiftKey:true}).defaultPrevented,true);
    assert.deepEqual(f.focused,['first','last']);
    const focus=target=>{
      const event=new Event('focusin');
      Object.defineProperty(event,'target',{value:target});
      f.window.dispatchEvent(event);
    };
    focus(f.focusItems[0]);
    assert.deepEqual(f.focused,['first','last'],'owned focus is left alone');
    focus({});
    assert.deepEqual(f.focused,['first','last','first']);
  } finally { f.scope.abort(); }
});

test('navigation rejects a drawer entry that no longer belongs to the shell',async()=>{
  const f=fixture(input.first);
  try {
    f.restoreMobile();
    f.history.state=null;
    const pending=[];
    const event=new Event('topcoat:before-navigation-commit');
    Object.defineProperty(event,'detail',{value:{mode:'push',waitUntil:work=>pending.push(work)}});
    f.document.dispatchEvent(event);
    assert.equal(pending.length,1);
    await assert.rejects(pending[0],/drawer history entry is no longer owned/);
    assert.deepEqual(f.historyMoves,[]);
  } finally { f.scope.abort(); }
});

test('owned push navigation waits for the drawer history to unwind',async()=>{
  const f=fixture(input.first);
  try {
    f.restoreMobile();
    const go=f.history.go;
    f.history.go=steps=>{
      go(steps);
      queueMicrotask(()=>{
        f.history.state.lificNativeHomeNav.pane='closed';
        f.window.dispatchEvent(new Event('popstate'));
      });
    };
    const pending=[];
    const event=new Event('topcoat:before-navigation-commit');
    Object.defineProperty(event,'detail',{value:{mode:'push',signal:new AbortController().signal,waitUntil:work=>pending.push(work)}});
    f.document.dispatchEvent(event);
    assert.equal(pending.length,1);
    assert.deepEqual(f.historyMoves,[-1]);
    await pending[0];
    assert.equal(f.chrome[3].get().toString(),'false');
  } finally { f.scope.abort(); }
});
