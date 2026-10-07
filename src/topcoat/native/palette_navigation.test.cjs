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

function fixture(markup) {
  const document=new EventTarget(), window=new EventTarget(), scope=new AbortController();setMaxListeners(0,scope.signal);
  const nav=attrs(markup.html.match(/<nav[^>]*class="native-home-palette-results"[^>]*>/)[0]);
  const rows=[...markup.html.matchAll(/<a\s[^>]*>/g)].map(match=>attrs(match[0])).filter(row=>'data-palette-index' in row);
  const root={}, navigations=[], tabs=[], media=new EventTarget();media.matches=false;
  const location={href:`http://localhost${mount}/`,assign:()=>{throw new Error('Unexpected hard navigation');}};
  const node=row=>({getAttribute:name=>row[name],scrollIntoView:()=>{}});
  document.documentElement={getAttribute:()=>mount,setAttribute:()=>{}};
  document.getElementById=()=>null;
  document.querySelector=selector=>{
    if(selector==='.native-home-shell')return root;
    if(selector==='.native-home-palette-results')return node(nav);
    const index=selector.match(/data-palette-index="(\d+)"/);
    return index?node(rows.find(row=>row['data-palette-index']===index[1])):null;
  };
  window.matchMedia=()=>media;window.open=(...args)=>tabs.push(args);
  const context={TextEncoder,TextDecoder,AbortController,Event,document,window,location,queueMicrotask,
    history:{state:null},crypto:require('node:crypto').webcrypto,localStorage:{getItem:()=>null},setTimeout,clearTimeout};
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
  vm.runInNewContext(source.replace(/export const (\w+)=/g,'globalThis.$1='),context);
  context.mount(cx,{},cx.tuple(chrome),cx.tuple(palette),cx.tuple([state[8],signal(''),signal('')]),
    cx.tuple([cx.hydrate('|"ACC"|"DCS"|'),cx.hydrate(`${mount}/login`),cx.hydrate({t:'i64',bits:64,v:'1'}),cx.hydrate(false)]));
  const invoke=code=>vm.runInNewContext(`cx=>(${code})`,context)(cx)({});
  const hover=()=>invoke(rows.find(row=>row.href===`${mount}/DCS/overview`)['data-topcoat-on:mouseenter']);
  const mounted=()=>invoke(nav['data-topcoat-on:mount']);
  const enter=modifier=>{
    const event=new Event('keydown',{cancelable:true});
    Object.defineProperties(event,{key:{value:'Enter'},target:{value:{id:'native-home-palette-query'}},metaKey:{value:modifier},ctrlKey:{value:false}});
    window.dispatchEvent(event);assert.equal(event.defaultPrevented,true);
  };
  return {hover,mounted,enter,state,navigations,tabs,scope};
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

test('Enter and modifier Enter navigate the mounted hovered anchor',()=>{
  for(const modifier of [false,true]){
    const f=fixture(input.first);f.mounted();f.hover();f.enter(modifier);
    if(modifier)assert.deepEqual(f.tabs,[[`${mount}/DCS/overview`,'_blank','noopener']]);
    else assert.deepEqual(f.navigations,[`${mount}/DCS/overview`]);
    f.scope.abort();
  }
});
