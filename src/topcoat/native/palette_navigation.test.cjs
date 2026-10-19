const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {setMaxListeners} = require('node:events');
const {TextEncoder, TextDecoder} = require('node:util');
const input = JSON.parse(fs.readFileSync(0,'utf8'));
const {mount,source} = input;
const {fixtureRuntime} = require('./runtime_fixture.cjs');
const decode = text => text.replace(/&(?:quot|apos|amp|lt|gt|#39);/g,value=>({'&quot;':'"','&apos;':"'",'&amp;':'&','&lt;':'<','&gt;':'>' ,'&#39;':"'"})[value]);
const attrs = tag => Object.fromEntries([...tag.matchAll(/([\w:-]+)="([^"]*)"/g)].map(([,key,value])=>[key,decode(value)]));
// Rust-generated expressions can contain a literal > inside quoted attributes.
const tags = html => [...html.matchAll(/<([a-z]+)\b(?:[^"'>]|"[^"]*"|'[^']*')*>/g)];

function fixture(markup,{domProjection=true,missingIds=new Set(),storage=null,historyNavigation=false,pendingPalette=false,initialHistoryState=null}={}) {
  const document=new EventTarget(), window=new EventTarget(), scope=new AbortController();setMaxListeners(0,scope.signal);
  const elements=tags(markup.html);
  const nav=elements.filter(match=>match[1]==='nav').map(match=>attrs(match[0])).find(row=>row.class==='native-home-palette-results');
  const rows=elements.filter(match=>match[1]==='a').map(match=>attrs(match[0])).filter(row=>'data-palette-index' in row);
  const rootListeners={}, root={addEventListener:(name,listener)=>{rootListeners[name]=listener;},contains:()=>true};
  const navigations=[], tabs=[], historyMoves=[], focused=[], media=new EventTarget();media.matches=false;
  const location={href:`http://localhost${mount}/`,assign:()=>{throw new Error('Unexpected hard navigation');}};
  const historyEntries=[{href:location.href,state:null}];let historyIndex=0;
  const traverse=steps=>{
    if(!historyNavigation)return;
    const index=historyIndex+Number(steps);
    if(index<0||index>=historyEntries.length)return;
    historyIndex=index;
    location.href=historyEntries[index].href;
    window.dispatchEvent(new Event('popstate'));
  };
  const history={
    get state(){return historyEntries[historyIndex].state;},
    set state(value){historyEntries[historyIndex].state=value;},
    replaceState:(state,_title)=>{historyEntries[historyIndex]={href:location.href,state};},
    pushState:(state,_title)=>{historyEntries.splice(historyIndex+1);historyEntries.push({href:location.href,state});historyIndex++;},
    back:()=>{historyMoves.push(-1);traverse(-1);},
    go:steps=>{historyMoves.push(Number(steps));traverse(Number(steps));},
  };
  const focusItems=['first','last'].map(id=>({
    focus:()=>focused.push(id),getClientRects:()=>[{}],closest:()=>null,
  }));
  const pane={querySelectorAll:()=>focusItems,contains:node=>focusItems.includes(node)};
  const projectTrigger={getAttribute:name=>name==='data-native-project-trigger'?'ACC':null,focus:()=>focused.push('project:ACC')};
  const unavailableFocus={focus:()=>focused.push('unavailable')};
  const projectPane={querySelectorAll:()=>focusItems,contains:node=>focusItems.includes(node)};
  const actionOwner={closest:selector=>selector==='.native-home-shell'?root:null};
  class FixtureElement {
    constructor(row){this.row=row;}
    closest(selector){return selector==='[data-native-mobile-action]'?this.row:selector==='.native-home-shell'?root:null;}
    getAttribute(name){return this.row[name]??null;}
  }
  const node=row=>({getAttribute:name=>row[name],scrollIntoView:()=>{}});
  document.documentElement={getAttribute:()=>mount,setAttribute:()=>{}};
  document.getElementById=id=>id==='native-mobile-action-owner'?actionOwner:missingIds.has(id)?null:({focus:()=>focused.push(id)});
  document.querySelectorAll=selector=>selector==='[data-native-project-trigger]'?[projectTrigger]:[];
  document.querySelector=selector=>{
    if(selector==='.native-home-shell')return root;
    if(selector==='[data-native-mobile-root]')return pane;
    if(selector==='[data-native-mobile-unavailable] button')return unavailableFocus;
    if(selector==='[data-native-mobile-project]:not([hidden]) button')return focusItems[0];
    if(selector==='[data-native-mobile-root] button')return focusItems[0];
    if(selector==='.native-home-theme-menu:not([hidden]) button')return focusItems[0];
    if(selector==='[data-native-mobile-project]:not([hidden])')return projectPane;
    if(selector==='[data-native-mobile-unavailable]')return projectPane;
    if(selector==='.native-home-palette-results')return domProjection?node(nav):null;
    const index=selector.match(/data-palette-index="(\d+)"/);
    return index&&domProjection?node(rows.find(row=>row['data-palette-index']===index[1])):null;
  };
  window.location=location;window.matchMedia=()=>media;window.open=(...args)=>tabs.push(args);
  class FixtureStorageEvent extends Event { constructor(name,options){super(name);Object.assign(this,options);} }
  const context={StorageEvent:FixtureStorageEvent,TextEncoder,TextDecoder,AbortController,DOMException,Event,Element:FixtureElement,document,window,location,queueMicrotask,
    history,crypto:require('node:crypto').webcrypto,localStorage:storage||{getItem:()=>null},setTimeout,clearTimeout};
  vm.runInNewContext(fixtureRuntime(['Context', 'Registry']),context);
  const registry=new context.fixture.Registry(),cx=Object.assign(new context.fixture.Context(registry),{
    abortSignal:scope.signal,navigate:href=>{navigations.push(href);return Promise.resolve();}
  });
  for(const match of markup.html.matchAll(/<!--::topcoat::signal\((.*?)\)-->/gs)){
    const value=JSON.parse(match[1]);registry.insert(value.id,cx.hydrate(value.v));
  }
  // The logical path remains server-side; query and pending focus are shared with the projection.
  const state=markup.state.map(value=>cx.hydrate(value));
  let nextId=0;
  const signal=value=>{const id=`extra-${++nextId}`;registry.insert(id,cx.hydrate(value));return cx.signal(id);};
  const usize=value=>({t:'usize',bits:64,v:String(value)});
  const chrome=[signal(false),signal('system'),signal(false),signal(false),signal('root'),signal(''),signal(''),signal(location.href),signal(pendingPalette),signal(''),signal(false),signal('')];
  const palette=[state[9],state[11],signal(''),state[0],signal(usize(0)),state[5],state[1],state[2],state[3],state[4],state[6],cx.tuple([state[7],state[12]])];
  const status=[state[8],signal(''),signal('native-home-palette-open')];
  vm.runInNewContext(source.replace(/export const (\w+)=/g,'globalThis.$1='),context);
  context.__lificNativeMounts={browser:context.browser,[input.handlerUrl+'#palette-projection']:context.paletteProjection};
  if(initialHistoryState)history.state=initialHistoryState;
  context.mount(cx,{},cx.tuple(chrome),cx.tuple(palette),cx.tuple(status),
    cx.tuple([cx.hydrate({t:'Vec',bits:64,v:['ACC','DCS']}),cx.hydrate(`${mount}/login`),cx.hydrate({t:'i64',bits:64,v:'1'}),cx.hydrate(false)]));
  const invoke=(code,event={})=>vm.runInNewContext(`cx=>(${code})`,context)(cx)(event);
  const hover=()=>invoke(rows.find(row=>row.href===`${mount}/DCS/overview`)['data-topcoat-on:mouseenter']);
  const mounted=()=>invoke(nav['data-topcoat-on:mount']);
  const inputNode=elements.filter(match=>match[1]==='input').map(match=>attrs(match[0])).find(row=>row.id==='native-home-palette-query');
  const inputMounted=()=>invoke(inputNode['data-topcoat-on:mount']);
  const project=(revision,destinations,allowed=true,mode='mount')=>context.paletteProjection(cx,cx.event(new Event('mount')),
    cx.tuple([...state.slice(0,11),cx.tuple([state[11],state[12]])]),
    cx.hydrate({t:'Record',v:{revision:usize(revision),destinations:{t:'Vec',bits:64,v:destinations}}}),
    cx.hydrate(allowed),cx.hydrate(mode));
  const key=(value,{id='native-home-palette-query',metaKey=false,ctrlKey=false,shiftKey=false}={})=>{
    const event=new Event('keydown',{cancelable:true});
    Object.defineProperties(event,{key:{value},target:{value:{id}},metaKey:{value:metaKey},ctrlKey:{value:ctrlKey},shiftKey:{value:shiftKey}});
    const inputNode=elements.filter(match=>match[1]==='input').map(match=>attrs(match[0])).find(row=>row.id===id);
    if(inputNode?.['data-topcoat-on:keydown'])invoke(inputNode['data-topcoat-on:keydown'],cx.event(event));
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
  const mobileMount=()=>context.mobileDispatch(cx,cx.event(new Event('mount')),
    cx.tuple([chrome[3],chrome[4],chrome[5],chrome[6],chrome[7],chrome[8],chrome[9],chrome[10]]));
  const mobileAction=(action,identifier='')=>{
    const row={'data-native-mobile-action':JSON.stringify([action,identifier]),getAttribute:name=>row[name]??null,closest:selector=>selector==='.native-home-shell'?root:null};
    const event=new Event('click');Object.defineProperty(event,'target',{value:new FixtureElement(row)});
    rootListeners.click?.(event);
  };
  const click=id=>{
    const event=new Event('click');
    Object.defineProperty(event,'target',{value:{closest:()=>id?{id}:null}});
    window.dispatchEvent(event);
  };
  return {toggleTheme:()=>context.chromeThemeToggle(cx,cx.event(new Event('click')),chrome[2]),
    chooseTheme:preference=>context.chromeThemeChoice(cx,cx.event(new Event('click')),chrome[1],chrome[2],cx.hydrate(preference)),
    collapse:()=>context.chromeCollapse(cx,cx.event(new Event('click')),chrome[0]),
    hover,mounted,inputMounted,project,browser:context.browser(cx),hydrate:value=>cx.hydrate(value),missingIds,enter,key,typeInput,restoreMobile,mobileMount,mobileAction,click,state,query:palette[1],chrome,history,historyMoves,focused,focusItems,window,document,navigations,tabs,scope,media,location,historyEntries};
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

test('the visible input handles Enter before mount without reading result DOM',()=>{
  const f=fixture(input.first,{domProjection:false});
  try {
    f.enter();
    assert.deepEqual(f.navigations,[`${mount}/DCS/overview`]);
  } finally { f.scope.abort(); }
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
    assert.equal(f.query.get().dehydrate(),'DCS');
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


test('Enter between input and the next projection waits for current authorized destinations',()=>{
  for(const [metaKey,ctrlKey] of [[false,false],[true,false],[false,true]]){
    const f=fixture(input.first,{domProjection:false});
    try {
      f.typeInput('native-home-palette-query','ACC');
      f.enter(metaKey,ctrlKey);
      assert.deepEqual(f.navigations,[]);
      assert.deepEqual(f.tabs,[]);
      assert.equal(f.state[6].get().dehydrate(),true);
      assert.equal(f.key('ArrowDown').defaultPrevented,true);
      f.project(0,[mount+'/DCS/overview']);
      assert.deepEqual(f.navigations,[],'stale destinations cannot drain Enter');
      f.project(1,[mount+'/ACC/overview']);
      if(metaKey||ctrlKey)assert.deepEqual(f.tabs,[[mount+'/ACC/overview','_blank','noopener']]);
      else assert.deepEqual(f.navigations,[mount+'/ACC/overview']);
      f.project(1,[mount+'/ACC/overview']);
      assert.equal(f.navigations.length+f.tabs.length,1,'queued Enter drains once');
    } finally { f.scope.abort(); }
  }
});

test('a disposed projection cannot navigate or change selection',()=>{
  const f=fixture(input.first);
  f.scope.abort();
  f.key('ArrowDown');
  f.key('Enter');
  f.mounted();
  assert.deepEqual(f.navigations,[]);
  assert.deepEqual(f.tabs,[]);
  assert.equal(f.state[4].get().toString(),'0');
});

test('palette focus waits for its input and is consumed once',async()=>{
  const missingIds=new Set(['native-home-palette-query']);
  const f=fixture(input.first,{missingIds});
  try {
    f.click('native-home-quick-jump');
    await Promise.resolve();
    assert.equal(f.state[12].get().dehydrate(),true);
    assert.deepEqual(f.focused,[]);
    missingIds.clear();
    f.project(1,[],false,'focus');
    assert.deepEqual(f.focused,['native-home-palette-query']);
    assert.equal(f.state[12].get().dehydrate(),false);
    f.project(1,[],false,'focus');
    assert.deepEqual(f.focused,['native-home-palette-query']);
  } finally { f.scope.abort(); }
});

test('closing the palette cancels delayed input focus',async()=>{
  const f=fixture(input.first);
  f.click('native-home-quick-jump');
  f.click('native-home-palette-close');
  await Promise.resolve();
  f.inputMounted();
  assert.deepEqual(f.focused,[]);
  assert.equal(f.state[12].get().dehydrate(),false);
  f.scope.abort();
});

test('shared browser bindings tolerate blocked storage and preserve callback arguments',async()=>{
  const storage={getItem(){throw Error('blocked');},setItem(){throw Error('blocked');},removeItem(){throw Error('blocked');}};
  const f=fixture(input.first,{storage});
  try {
    const key=f.hydrate('setting'),value=f.hydrate('value');
    assert.equal(f.browser.stored(key).dehydrate(),'');
    f.browser.store(key,value);
    f.browser.remove_storage(key);
    assert.equal(f.browser.call0(()=>42),42);
    assert.equal(f.browser.call1(arg=>arg,value),value);
    const calls=[];
    f.browser.microtask(()=>calls.push('queued'));
    f.scope.abort();
    await Promise.resolve();
    assert.deepEqual(calls,[]);
    f.browser.microtask(()=>calls.push('disposed'));
    await Promise.resolve();
    assert.deepEqual(calls,[]);
  } finally { f.scope.abort(); }
});

test('mobile actions push owned root and project entries and back restores project focus',async()=>{
  const f=fixture(input.first,{historyNavigation:true});
  try {
    f.mobileMount();
    f.mobileAction('open');
    assert.equal(f.chrome[3].get().toString(),'true');
    assert.equal(f.chrome[4].get().toString(),'root');
    assert.equal(f.history.state.lificNativeHomeNav.pane,'root');
    f.mobileAction('project','ACC');
    assert.equal(f.chrome[4].get().toString(),'project');
    assert.equal(f.chrome[5].get().toString(),'ACC');
    assert.equal(f.history.state.lificNativeHomeNav.project,'ACC');
    f.history.back();
    await Promise.resolve();
    assert.equal(f.chrome[4].get().toString(),'root');
    assert.equal(f.chrome[5].get().toString(),'');
    assert.deepEqual(f.focused,['first','first','project:ACC']);
    f.history.back();
    assert.equal(f.chrome[3].get().toString(),'false');
    assert.deepEqual(f.historyMoves,[-1,-1]);
  } finally { f.scope.abort(); }
});

test('owned history can present an unavailable project and focuses its recovery control',async()=>{
  const f=fixture(input.first);
  try {
    f.history.state={lificNativeHomeNav:{
      version:'1',owner:f.chrome[6].get().toString(),href:f.location.href,pane:'project',project:'MISSING',
    }};
    f.window.dispatchEvent(new Event('popstate'));
    await Promise.resolve();
    assert.equal(f.chrome[3].get().toString(),'true');
    assert.equal(f.chrome[4].get().toString(),'unavailable');
    assert.equal(f.chrome[9].get().toString(),'MISSING');
    assert.deepEqual(f.focused,['unavailable']);
  } finally { f.scope.abort(); }
});

test('malformed and foreign history records close the drawer and cannot authorize navigation',async()=>{
  const f=fixture(input.first);
  try {
    f.restoreMobile();
    f.history.state={lificNativeHomeNav:{
      version:'1',owner:'another-shell',href:f.location.href,pane:'root',project:'',
    }};
    f.window.dispatchEvent(new Event('popstate'));
    assert.equal(f.chrome[3].get().toString(),'false');

    f.restoreMobile();
    f.history.state={lificNativeHomeNav:{
      version:'1',owner:f.chrome[6].get().toString(),href:f.location.href,pane:'project',project:'',
    }};
    f.window.dispatchEvent(new Event('popstate'));
    assert.equal(f.chrome[3].get().toString(),'false');

    f.restoreMobile();
    f.history.state={lificNativeHomeNav:{
      version:'1',owner:'another-shell',href:f.location.href,pane:'root',project:'',
    }};
    const pending=[];
    const event=new Event('topcoat:before-navigation-commit');
    Object.defineProperty(event,'detail',{value:{mode:'push',signal:new AbortController().signal,waitUntil:work=>pending.push(work)}});
    f.document.dispatchEvent(event);
    assert.equal(pending.length,1);
    await assert.rejects(pending[0],/drawer history entry is no longer owned/);
    assert.deepEqual(f.historyMoves,[]);
  } finally { f.scope.abort(); }
});

test('desktop media change closes the root drawer through owned history',()=>{
  const f=fixture(input.first,{historyNavigation:true});
  try {
    f.mobileMount();
    f.mobileAction('open');
    f.media.matches=true;
    f.media.dispatchEvent(new Event('change'));
    assert.equal(f.chrome[3].get().toString(),'false');
    assert.deepEqual(f.historyMoves,[-1]);
    assert.equal(f.history.state.lificNativeHomeNav.pane,'closed');
  } finally { f.scope.abort(); }
});

test('closing the drawer through its action restores focus to the opener',async()=>{
  const f=fixture(input.first,{historyNavigation:true});
  try {
    f.mobileMount();
    f.mobileAction('open');
    await Promise.resolve();
    f.focused.length=0;
    f.mobileAction('close');
    await Promise.resolve();
    assert.equal(f.chrome[3].get().toString(),'false');
    assert.deepEqual(f.focused,['native-home-mobile-open']);
  } finally { f.scope.abort(); }
});

test('mobile search passes the opener and Escape restores that scalar focus target',async()=>{
  const f=fixture(input.first,{historyNavigation:true});
  try {
    f.mobileMount();
    f.mobileAction('open');
    await Promise.resolve();
    f.focused.length=0;
    f.mobileAction('search');
    await Promise.resolve();
    assert.equal(f.state[9].get().toString(),'true');
    f.focused.length=0;
    assert.equal(f.key('Escape').defaultPrevented,true);
    await Promise.resolve();
    assert.ok(f.focused.includes('native-home-mobile-open'),`focus targets: ${f.focused.join(', ')}`);
  } finally { f.scope.abort(); }
});

test('navigation waiter rejects when its navigation signal is cancelled',async()=>{
  const f=fixture(input.first,{historyNavigation:true});
  try {
    f.restoreMobile();
    const controller=new AbortController(),pending=[];
    const event=new Event('topcoat:before-navigation-commit');
    Object.defineProperty(event,'detail',{value:{mode:'push',signal:controller.signal,waitUntil:work=>pending.push(work)}});
    f.document.dispatchEvent(event);
    assert.equal(pending.length,1);
    controller.abort();
    await assert.rejects(pending[0],error=>error.name==='AbortError'&&/Navigation cancelled/.test(error.message));
  } finally { f.scope.abort(); }
});

test('a replacement shell owner clears stale palette intent and can open mobile navigation',()=>{
  const oldOwner='previous-shell-owner';
  const f=fixture(input.first,{
    pendingPalette:true,
    initialHistoryState:{lificNativeHomeNav:{
      version:'1',owner:oldOwner,href:`http://localhost${mount}/`,pane:'unknown',project:'',
    }},
  });
  try {
    assert.equal(f.chrome[8].get().toString(),'false');
    assert.notEqual(f.chrome[6].get().toString(),oldOwner);
    f.mobileMount();
    f.mobileAction('open');
    assert.equal(f.chrome[3].get().toString(),'true');
    assert.equal(f.chrome[4].get().toString(),'root');
    assert.equal(f.history.state.lificNativeHomeNav.owner,f.chrome[6].get().toString());
  } finally { f.scope.abort(); }
});


test('one cancelled navigation waiter leaves the shared drawer unwind available',async()=>{
  const f=fixture(input.first),first=new AbortController(),second=new AbortController(),pending=[];
  try {
    f.restoreMobile();
    for(const controller of [first,second]){
      const event=new Event('topcoat:before-navigation-commit');
      Object.defineProperty(event,'detail',{value:{mode:'push',signal:controller.signal,waitUntil:work=>pending.push(work)}});
      f.document.dispatchEvent(event);
    }
    assert.deepEqual(f.historyMoves,[-1],'two waiters share one history traversal');
    first.abort();
    await assert.rejects(pending[0],error=>error.name==='AbortError');
    f.history.state.lificNativeHomeNav.pane='closed';
    f.window.dispatchEvent(new Event('popstate'));
    await pending[1];
    assert.equal(f.chrome[3].get().dehydrate(),false);
  } finally { f.scope.abort(); }
});

test('retiring the shell rejects its pending drawer navigation',async()=>{
  const f=fixture(input.first),pending=[];
  f.restoreMobile();
  const event=new Event('topcoat:before-navigation-commit');
  Object.defineProperty(event,'detail',{value:{mode:'push',signal:new AbortController().signal,waitUntil:work=>pending.push(work)}});
  f.document.dispatchEvent(event);
  f.scope.abort();
  await assert.rejects(pending[0],error=>error.name==='AbortError'&&/owner ended/.test(error.message));
});

test('shared theme controls persist choices, notify storage listeners, and restore system mode',async()=>{
  const writes=[],removals=[],events=[],storage={getItem:()=>null,setItem:(...args)=>writes.push(args),removeItem:key=>removals.push(key)};
  const f=fixture(input.first,{storage});
  try {
    f.window.addEventListener('storage',event=>events.push([event.key,event.newValue]));
    f.toggleTheme();
    assert.equal(f.chrome[2].get().dehydrate(),true);
    await Promise.resolve();
    assert.deepEqual(f.focused,['first']);
    for(const preference of ['dark','light','system']){
      f.chooseTheme(preference);
      assert.equal(f.chrome[1].get().dehydrate(),preference);
      assert.equal(f.chrome[2].get().dehydrate(),false);
    }
    assert.deepEqual(writes,[['lific_theme','dark'],['lific_theme','light']]);
    assert.deepEqual(removals,['lific_theme']);
    assert.deepEqual(events,[['lific_theme','dark'],['lific_theme','light'],['lific_theme',null]]);
    f.collapse();f.collapse();
    assert.equal(f.chrome[0].get().dehydrate(),false);
    assert.deepEqual(writes.slice(-2),[['lific:sidebar:collapsed','1'],['lific:sidebar:collapsed','0']]);
  } finally { f.scope.abort(); }
});
