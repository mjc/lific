const {test} = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const path = require('node:path');
const runtime = fs.readFileSync(path.join(__dirname, '../assets/runtime.js'), 'utf8');
const reversals = [{"patched":"// Lific transport patch for Topcoat 0.10.0: mount logical HTTP requests and shared runs.\nfunction topcoatMountPrefix(){return document.documentElement.getAttribute(\"data-topcoat-runtime-prefix\")||\"\"}\nfunction topcoatMountedEndpoint(path){const mount=topcoatMountPrefix();return mount&&path.startsWith(\"/\")&&!path.startsWith(\"//\")?mount+path:path}\nfunction topcoatLogicalEndpoint(path){const mount=topcoatMountPrefix();return mount&&path.startsWith(mount+\"/\")?path.slice(mount.length):mount&&(path===mount||path.startsWith(mount+\"?\"))?\"/\"+path.slice(mount.length):path}\n// Generic mount lifecycle: factories run after hydration, once per element and owning scope.\nconst topcoatMountScopes=new WeakMap;\nfunction topcoatMount(element,factory,scope){let elements=topcoatMountScopes.get(scope);if(!elements){elements=new WeakSet;topcoatMountScopes.set(scope,elements)}if(elements.has(element))return;elements.add(element);queueMicrotask(()=>{if(!scope.abortSignal.aborted&&!scope.isDisposed&&element.isConnected){const event=new Event(\"mount\"),invoke=received=>{if(received!==event)return;try{Promise.resolve(factory()(new ae(received))).catch(error=>scope.runtime.reportError(error))}catch(error){scope.runtime.reportError(error)}};element.addEventListener(\"mount\",invoke,{signal:scope.abortSignal});try{element.dispatchEvent(event)}finally{element.removeEventListener(\"mount\",invoke)}}})}\n","upstream":"","count":1,"name":"helper prelude"},{"patched":"let{url:i,headers:o,body:s}=e.rerunRequest(),{pathname:a,search:c}=new URL(i,location.href),l=e.label===\"Page\"?topcoatLogicalEndpoint(`${a}${c}`):`${a}${c}`;return t.send(JSON.stringify({run:r,method:\"POST\",path:l,headers:o,body:s})),!0","upstream":"let{url:i,headers:o,body:s}=e.rerunRequest(),{pathname:a,search:u}=new URL(i,location.href),l=`${a}${u}`;return t.send(JSON.stringify({run:r,method:\"POST\",path:l,headers:o,body:s})),!0","count":1,"name":"logical run path"},{"patched":"fetch(topcoatMountedEndpoint(r),{method:\"POST\",cache:\"no-store\",headers:{...i,Accept:t},body:o,signal:e})","upstream":"fetch(r,{method:\"POST\",cache:\"no-store\",headers:{...i,Accept:t},body:o,signal:e})","count":1,"name":"render request mount"},{"patched":"refresh(){if(this.requiresConnection){let{connection:e}=this.runtime;if(e.run(this))return Promise.resolve();this.syncConnection();return Promise.resolve()}let{connection:e}=this.runtime;e.stop(this);let t=x();return this.requestController.run(r=>this.request(r,R),r=>W(this,r,this.label,t),this.label)}","upstream":"refresh(){let{connection:e}=this.runtime;if(this.requiresConnection&&e.run(this))return Promise.resolve();e.stop(this);let t=x();return this.requestController.run(r=>this.request(r,R),r=>W(this,r,this.label,t),this.label)}","count":1,"name":"connected render wait"},{"patched":"function Pe(n){let e=new DOMParser().parseFromString(n.replaceAll(\"<\",\"&lt;\"),\"text/html\").documentElement.textContent;","upstream":"function Pe(n){let e=new DOMParser().parseFromString(n,\"text/html\").documentElement.textContent;","count":1,"name":"marker literal less-than"},{"patched":"function Je(n,e,t){if(!e.name.startsWith(Ke))return;let r=e.name.substring(Ke.length),i=Object.assign(Object.create(t.runtime.context),{abortSignal:t.abortSignal});if(r===\"mount\"){topcoatMount(n,()=>I(e.value,`event @${r}`)(i),t);return}let o=I(e.value,`event @${r}`)(i);n.addEventListener(r,s=>o(new ae(s)),{signal:t.abortSignal})}","upstream":"function Je(n,e,t){if(!e.name.startsWith(Ke))return;let r=e.name.substring(Ke.length),o=I(e.value,`event @${r}`)(t.runtime.context);n.addEventListener(r,s=>o(new ae(s)),{signal:t.abortSignal})}","count":1,"name":"scoped events and mount"},{"patched":"record(e){return new A(Object.entries(e))}tuple(e){return new N(e)}event(e){return new ae(e)}};","upstream":"record(e){return new A(Object.entries(e))}tuple(e){return new N(e)}};","count":1,"name":"native event adapter"},{"patched":"clone(){return this.to_vec()}position(e){let n=this.items.findIndex(t=>t.eq(e).dehydrate());return n<0?h.none():h.some(new w(BigInt(n),this.usizeType))}clone_with_push(e){let n=this.items.slice();return n.push(e),new j(n,this.usizeType)}clone_without_index(e){let n=e.toIndex(this.items.length,this.usizeType.bits);if(n===void 0)throw new RangeError(\"Vec index out of bounds\");let t=this.items.slice();return t.splice(n,1),new j(t,this.usizeType)}dehydrate(){return{t:\"Vec\",bits:this.usizeType.bits,v:this.items.map(p)}}","upstream":"clone(){return this.to_vec()}dehydrate(){return{t:\"Vec\",bits:this.usizeType.bits,v:this.items.map(p)}}","count":1,"name":"immutable vector helpers"},{"patched":"decrement(){this.inner.set(e=>e instanceof w?e.decrement():e.sub(new T(1)))}push(e){this.inner.set(t=>t.clone_with_push(e))}remove(e){this.inner.set(t=>t.clone_without_index(e))}push_str(e){this.inner.set(t=>new S(`${t}${e}`))}","upstream":"decrement(){this.inner.set(e=>e instanceof w?e.decrement():e.sub(new T(1)))}push_str(e){this.inner.set(t=>new S(`${t}${e}`))}","count":1,"name":"vector signal writes"},{"patched":"le(e){return new c(se(this.v,e.v)<=0)}to_uppercase(){return new S(this.v.toUpperCase())}unicode_scalars(e){if(e.type.kind!==\"usize\")throw new Error(\"Unicode scalar vector requires target usize width\");return new j(Array.from(this.v,r=>new S(r)),e.type)}to_owned(){return new S(this.v)}is_empty()","upstream":"le(e){return new c(se(this.v,e.v)<=0)}to_owned(){return new S(this.v)}is_empty()","count":1,"name":"unicode string helpers"},{"patched":"to_owned(){return new S(this.v)}is_empty(){return new c(this.v.length===0)}len(){return new w(BigInt(It.encode(this.v).length),je())}trim_ecmascript(){return new S(this.v.trim())}trim(){","upstream":"to_owned(){return new S(this.v)}is_empty(){return new c(this.v.length===0)}len(){return new w(BigInt(It.encode(this.v).length),je())}trim(){","count":1,"name":"ECMAScript trim"},{"patched":"call(...e){return this.request(e,!1)}call_keepalive(...e){return this.request(e,!0)}with_keepalive(){return{call:(...e)=>this.call_keepalive(...e)}}request(e,n){return new ue(async()=>{let t=await fetch(topcoatMountedEndpoint(this.path),{method:\"POST\",headers:{\"Content-Type\":\"application/json\"},body:JSON.stringify(e.map(p)),redirect:\"manual\",...(n?{keepalive:!0}:{})}).finally(De);if(!t.ok)throw new Error(`Procedure call failed: ${t.status} ${t.statusText}`);return this.cx.hydrate(await t.json())})}","upstream":"call(...e){return new ue(async()=>{let t=await fetch(this.path,{method:\"POST\",headers:{\"Content-Type\":\"application/json\"},body:JSON.stringify(e.map(p))}).finally(De);if(!t.ok)throw new Error(`Procedure call failed: ${t.status} ${t.statusText}`);return this.cx.hydrate(await t.json())})}","count":1,"name":"generic keepalive and manual redirect"},{"patched":"case\"redirect\":n.runtime.redirect(e.location);break;","upstream":"case\"redirect\":location.assign(e.location);break;","count":1,"name":"connected render redirect"},{"patched":"constructor(e,t,r=x=>location.assign(x)){this.lifetime=e;this.reportError=t;this.redirect=r;e.addEventListener(\"abort\",()=>this.cancel(),{once:!0})}","upstream":"constructor(e,t){this.lifetime=e;this.reportError=t;e.addEventListener(\"abort\",()=>this.cancel(),{once:!0})}","count":1,"name":"request redirect callback"},{"patched":"lifetime;reportError;redirect;controller=null;","upstream":"lifetime;reportError;controller=null;","count":1,"name":"request redirect field"},{"patched":"if(s.redirected){this.redirect(s.url);return}","upstream":"if(s.redirected){location.assign(s.url);return}","count":1,"name":"HTTP render redirect"},{"patched":"new Y(this.lifetime.abortSignal,r=>t.reportError(r),r=>t.redirect(r))","upstream":"new Y(this.lifetime.abortSignal,r=>t.reportError(r))","count":1,"name":"request redirect wiring"},{"patched":"constructor(e,t,r=qt,i=x=>location.assign(x)){this.url=e;this.reportError=t;this.open=r;this.redirect=i}","upstream":"constructor(e,t,r=qt){this.url=e;this.reportError=t;this.open=r}","count":1,"name":"connection redirect callback"},{"patched":"url;reportError;open;redirect;socket=null;","upstream":"url;reportError;open;socket=null;","count":1,"name":"connection redirect field"},{"patched":"if(t.run===void 0){if(t.frame?.t===\"redirect\"){this.redirect(t.frame.location);return}let i=t.frame?.t===\"error\"?t.frame.status:\"\";this.reportError(new Error(`Connected request rejected: ${i}`));return}","upstream":"if(t.run===void 0){let i=t.frame.t===\"error\"?t.frame.status:\"\";this.reportError(new Error(`Connected request rejected: ${i}`));return}","count":1,"name":"runless connection redirect"},{"patched":"connection=new me(Me,e=>this.reportError(e),qt,e=>this.redirect(e))","upstream":"connection=new me(Me,e=>this.reportError(e))","count":1,"name":"runtime connection redirect wiring"},{"patched":"var ye=class{navigating=!1;redirect(e){if(this.navigating)return;this.navigating=!0;location.assign(e)}registry=new ve;","upstream":"var ye=class{registry=new ve;","count":1,"name":"document redirect claim"},{"patched":"reportError(e){this.runtime.reportError(e);if(this.isDisposed)return;this.failureTarget()?.dispatchEvent(new CustomEvent(\"topcoat:render-error\",{bubbles:!0,detail:{path:this.url()}}))}failureTarget(){return this.startNode?.parentNode??document}url(){return topcoatMountedEndpoint(this.rerunRequest().url)}*ancestors()","upstream":"reportError(e){this.runtime.reportError(e)}*ancestors()","count":1,"name":"scoped render failure notifications"},{"patched":"reset(){this.disconnect();this.members.clear()}disconnect(){","upstream":"disconnect(){","count":1,"name":"new page connection reset"},{"patched":"replaceDocument(e,t){if(this.isDisposed)return;this.runtime.connection.reset();","upstream":"replaceDocument(e,t){if(this.isDisposed)return;this.runtime.connection.stop(this);","count":1,"name":"new page handshake boundary"},{"patched":"t.addEventListener(\"message\",r=>{if(this.socket===t)this.receive(r.data)})","upstream":"t.addEventListener(\"message\",r=>this.receive(r.data))","count":1,"name":"physical socket frame ownership"},{"patched":"// Page owners may finish local work before history or the mounted scope changes.\nasync function topcoatBeforeNavigationCommit(detail,isCurrent){\n const {signal}=detail;\n if(signal.aborted||!isCurrent())return false;\n const waits=[];\n const event=new CustomEvent(\"topcoat:before-navigation-commit\",{cancelable:true,detail:{...detail,waitUntil:promise=>waits.push(Promise.resolve(promise))}});\n document.dispatchEvent(event);\n const ready=Promise.all(waits);\n let onAbort;\n const aborted=new Promise(resolve=>{onAbort=()=>resolve(false);signal.addEventListener(\"abort\",onAbort,{once:true})});\n try{\n  if(event.defaultPrevented||signal.aborted){ready.catch(()=>{});return false}\n  return await Promise.race([ready.then(()=>!signal.aborted&&isCurrent()),aborted]);\n }catch{return false}finally{signal.removeEventListener(\"abort\",onAbort)}\n}\nconst topcoatMountScopes=new WeakMap;","upstream":"const topcoatMountScopes=new WeakMap;","count":1,"name":"navigation owner barrier"},{"patched":"context=Object.assign(new fe(this.registry),{navigate:e=>this.navigation.navigate(new URL(e,location.href),\"push\")});","upstream":"context=new fe(this.registry);","count":1,"name":"programmatic native navigation"},{"patched":"if(!await topcoatBeforeNavigationCommit({url:a,mode:r,nextDocument:g,signal:e.controller.signal},o)){e.abort();return}if(!o())return;this.commit(g,a,r,i,l),d=!0;break","upstream":"this.commit(g,a,r,i,l),d=!0;break","count":1,"name":"navigation owner commit boundary"},{"patched":"replaceDocument(e,t){if(this.isDisposed)return;document.dispatchEvent(new CustomEvent(\"topcoat:before-page-replace\",{detail:{nextDocument:e}}));this.runtime.connection.reset();","upstream":"replaceDocument(e,t){if(this.isDisposed)return;this.runtime.connection.reset();","count":1,"name":"page owner transfer boundary"}];

// Exact reconstruction keeps the vendored runtime tied to the inspected crate.
test('0.10 transport patches reconstruct the rebased fork runtime bytes', () => {
  let original = runtime;
  for (const {patched, upstream, count, name} of [...reversals].reverse()) {
    assert.equal(original.split(patched).length - 1, count, `${name} occurs as expected`);
    original = original.replaceAll(patched, upstream);
  }
  assert.equal(crypto.createHash('sha256').update(original).digest('hex'),
    'deb99dd257cc94b3142561f99c47891a7d36cf3f79ea21dec7acaa12d68bc965');
});

// These framework-only tests run in Node's VM; no DOM, browser or server is involved.
test('mount helpers preserve boundaries and remove one mounted prefix for page runs', () => {
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  assert.equal(runtime.split(bootstrap).length - 1, 1);
  const context = {TextEncoder, TextDecoder, document: {documentElement: {getAttribute: () => '/ACC'}}};
  vm.runInNewContext(runtime.replace(bootstrap,
    'globalThis.fixture={mounted:topcoatMountedEndpoint,logical:topcoatLogicalEndpoint};'), context);
  const {mounted, logical} = context.fixture;
  assert.equal(mounted('/__native/refresh'), '/ACC/__native/refresh');
  assert.equal(mounted('https://example.test/ACC/page'), 'https://example.test/ACC/page');
  assert.equal(mounted('//cdn.example.test/a'), '//cdn.example.test/a');
  assert.equal(logical('/ACC/page?q=1'), '/page?q=1');
  assert.equal(logical('/ACC?q=1'), '/?q=1');
  assert.equal(logical('/ACC/ACC/page'), '/ACC/page');
  assert.equal(logical('/ACCT/page'), '/ACCT/page');
});

test('generic procedure calls mount paths and keep keepalive opt-in', async () => {
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  const calls = [];
  const context = {TextEncoder, TextDecoder,
    document: {documentElement: {getAttribute: () => '/app'}},
    fetch: async (url, options) => {
      calls.push({url, options});
      return {ok: true, json: async () => ({saved: true})};
    }};
  vm.runInNewContext(runtime.replace(bootstrap,
    'globalThis.fixture={Procedure:ce};'), context);
  const procedure = new context.fixture.Procedure({hydrate: value => value}, '/__native/save');
  await procedure.call('ordinary');
  await procedure.with_keepalive().call('kept');
  assert.equal(calls.length, 2);
  assert.equal(calls[0].url, '/app/__native/save');
  assert.equal(calls[0].options.redirect, 'manual');
  assert.equal(calls[0].options.keepalive, undefined);
  assert.equal(calls[1].url, '/app/__native/save');
  assert.equal(calls[1].options.redirect, 'manual');
  assert.equal(calls[1].options.keepalive, true);
  assert.equal(calls[1].options.credentials, undefined);
  assert.equal(calls[1].options.headers.Authorization, undefined);
});

test('connection-required refresh waits for the shared connection instead of falling back to HTTP', async () => {
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  const context = {TextEncoder, TextDecoder,
    document: {documentElement: {getAttribute: () => '/app'}, querySelector: () => ({dataset: {topcoatUsizeBits: '64'}})}};
  vm.runInNewContext(runtime.replace(bootstrap,
    'globalThis.fixture={RenderUnit:_};'), context);
  const {RenderUnit} = context.fixture;
  let syncs = 0, httpRuns = 0, stops = 0;
  const unit = Object.create(RenderUnit.prototype);
  unit.contentScope = {contentRequiresConnection: () => true};
  unit.runtime = {connection: {run: () => false, stop: () => stops++}};
  unit.syncConnection = () => syncs++;
  unit.requestController = {run: () => {httpRuns++;}};
  await unit.refresh();
  assert.equal(syncs, 1);
  assert.equal(httpRuns, 0);
  assert.equal(stops, 0);
  unit.runtime.connection.run = () => true;
  await unit.refresh();
  assert.equal(syncs, 1);
  assert.equal(httpRuns, 0);
});

test('Unicode helpers preserve target usize width and use ECMAScript trimming', () => {
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  const context = {TextEncoder, TextDecoder,
    document: {documentElement: {getAttribute: () => '/app'}, querySelector: () => ({dataset: {topcoatUsizeBits: '64'}})}};
  vm.runInNewContext(runtime.replace(bootstrap,
    'globalThis.fixture={String:H,Owned:S,Vec:j,Integer:w,Signal:pe};'), context);
  const {String: RuntimeString, Owned, Vec, Integer, Signal} = context.fixture;
  const owned = value => new Owned(value);
  const json = value => JSON.parse(JSON.stringify(value));
  for (const bits of [32, 64]) {
    const type = {kind: 'usize', bits, min: 0n, max: (1n << BigInt(bits)) - 1n, signed: false};
    const input = new RuntimeString('Straße ﬃ 😀a');
    assert.equal(input.to_uppercase().dehydrate(), 'STRASSE FFI 😀A');
  assert.equal(input.le(new RuntimeString('T')).dehydrate(), true);
  assert.equal(input.le(new RuntimeString('S')).dehydrate(), false);
  assert.equal(input.len().type.bits, 64);
    const vector = input.to_uppercase().unicode_scalars(new Integer(0n, type));
    assert.deepEqual(json(vector.dehydrate()), {t: 'Vec', bits, v: [...'STRASSE FFI 😀A']});
    const pushed = vector.clone_with_push(owned('!'));
    assert.deepEqual(pushed.dehydrate().v.at(-1), '!');
    assert.equal(pushed.clone_without_index(new Integer(0n, type)).dehydrate().v.length,
      [...'STRASSE FFI 😀A'].length);
    assert.throws(() => vector.clone_without_index(new Integer(99n, type)), error => error.name === 'RangeError');
    let current = vector;
    const reactive = new Signal('letters', Object.assign(() => current, {set: update => { current = update(current); }}));
    reactive.push(owned('!'));
    assert.equal(reactive.dehydrate().v.v.at(-1), '!');
  }
  assert.equal(new RuntimeString('\uFEFFx\uFEFF').trim_ecmascript().dehydrate(), 'x');
  assert.equal(new RuntimeString('\u0085x\u0085').trim_ecmascript().dehydrate(), '\u0085x\u0085');
  assert.throws(() => new RuntimeString('x').unicode_scalars(
    new Integer(0n, {kind: 'u64', bits: 64, min: 0n, max: 2n ** 64n - 1n, signed: false})));
});

test('vector position uses typed equality and preserves target usize width', () => {
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  const context = {TextEncoder, TextDecoder};
  vm.runInNewContext(runtime.replace(bootstrap,
    'globalThis.fixture={Owned:S,Vec:j,Integer:w};'), context);
  const {Owned, Vec, Integer} = context.fixture;
  const json = value => JSON.parse(JSON.stringify(value.dehydrate()));
  for (const bits of [32, 64]) {
    const type = {kind:'usize',bits,min:0n,max:(1n<<BigInt(bits))-1n,signed:false};
    const values = new Vec([new Owned('first'),new Owned('😀'),new Owned('first')],type);
    assert.deepEqual(json(values.position(new Owned('first'))), {t:'Option',v:{t:'usize',bits,v:'0'}});
    assert.deepEqual(json(values.position(new Owned('😀'))), {t:'Option',v:{t:'usize',bits,v:'1'}});
    assert.deepEqual(json(values.position(new Owned('missing'))), {t:'Option',v:null});
    assert.deepEqual(json(new Vec([],type).position(new Owned('first'))), {t:'Option',v:null});
    assert.deepEqual(json(values).v,['first','😀','first']);
    const indices = new Vec([new Integer(9007199254740993n,{kind:'i64',bits:64,min:-(1n<<63n),max:(1n<<63n)-1n,signed:true})],type);
    assert.deepEqual(json(indices.position(new Integer(9007199254740993n,{kind:'i64',bits:64,min:-(1n<<63n),max:(1n<<63n)-1n,signed:true}))), {t:'Option',v:{t:'usize',bits,v:'0'}});
  }
});

test('runless and run-scoped redirects share one document navigation claim', () => {
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  const locations = [], failures = [];
  const context = {
    document: {documentElement: {getAttribute: () => '/app'}},
    location: {origin: 'https://example.test', pathname: '/app/issues', search: '?q=1', assign: url => locations.push(url)},
    AbortController, queueMicrotask, TextEncoder, TextDecoder,
    console: {error: error => failures.push(error)},
  };
  vm.runInNewContext(runtime.replace(bootstrap,
    'globalThis.fixture={Runtime:ye,Connection:me,frame:W};'), context);
  const {Runtime, Connection, frame} = context.fixture;
  const documentRuntime = new Runtime();
  assert.equal(documentRuntime.connection.url(), 'https://example.test/app/issues?q=1');
  assert.ok(documentRuntime.connection instanceof Connection);
  documentRuntime.connection.receive(JSON.stringify({frame: {t: 'redirect', location: '/app/retired'}}));
  frame({runtime: documentRuntime}, {t: 'redirect', location: '/app/second'}, 'Connected', Symbol('render'));
  assert.deepEqual(locations, ['/app/retired']);
  assert.deepEqual(failures, []);
});

test('shared connection admits one target Run with a logical mounted path', () => {
  const bootstrap = runtime.match(/var \w+=new \w+;(\w+)\.start\(document\);\1\.page\.listenForDevRefresh\(\);/)?.[0];
  assert.ok(bootstrap, 'runtime startup can be replaced with an isolated fixture');
  const context = {TextEncoder, TextDecoder, URL, location: {href: 'https://example.test/ACC/'}, document: {documentElement: {getAttribute: () => '/ACC'}}};
  vm.runInNewContext(runtime.replace(bootstrap, 'globalThis.fixture={Connection:me};'), context);
  const sent = [];
  const connection = new context.fixture.Connection(() => '/ACC/', () => {}, url => url, () => {});
  connection.socket = {readyState: 1, send: frame => sent.push(JSON.parse(frame))};
  const target = {label: 'Page', rerunRequest: () => ({url: 'https://example.test/ACC/ACC/page?q=1', headers: {}, body: '{}'})};
  assert.equal(connection.run(target), true);
  assert.deepEqual(JSON.parse(JSON.stringify(sent)), [{run: 1, method: 'POST', path: '/ACC/page?q=1', headers: {}, body: '{}'}]);
  connection.run({...target, label: 'Shard', rerunRequest: () => ({url: '/ACC/overview', headers: {}, body: '{}'})});
  assert.equal(sent[1].path, '/ACC/overview', 'logical shard paths that resemble the mount remain intact');
});

test('committing a new page retires its socket before fresh targets join', () => {
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  const opened = [], closed = [], sent = [], rendered = [], lifecycle = [];
  class CustomEvent { constructor(type, options) { this.type = type; this.detail = options.detail; } }
  let url = 'https://example.test/app/ACC/pages';
  const context = {TextEncoder, TextDecoder, URL, CustomEvent, queueMicrotask: () => {},
    location: {href: url}, document: {activeElement: null,
      dispatchEvent: event => lifecycle.push(event),
      documentElement: {getAttribute: () => '/app'}}, HTMLElement: class {}};
  vm.runInNewContext(runtime.replace(bootstrap,
    'globalThis.fixture={Connection:me,Page:ge};'), context);
  const {Connection, Page} = context.fixture;
  const connection = new Connection(() => url, error => {throw error;}, endpoint => {
    opened.push(endpoint);
    return {readyState: 1, addEventListener() {}, send: value => sent.push(JSON.parse(value)),
      close: () => closed.push(endpoint)};
  });
  const page = Object.create(Page.prototype);
  Object.defineProperty(page, 'isDisposed', {value: false});
  page.runtime = {connection};
  page.rerunRequest = () => ({url, headers: {}, body: '{}'});
  page.replaceContent = html => rendered.push(html);
  connection.join(page);
  connection.run(page);
  const previousRun = sent.at(-1).run;
  url = 'https://example.test/app/ACC/plans';
  const nextDocument = {};
  page.replace = () => { lifecycle.push('old scope abort'); connection.join(page); };
  page.replaceDocument(nextDocument, Symbol('new-page'));
  assert.equal(lifecycle[0].type, 'topcoat:before-page-replace');
  assert.equal(lifecycle[0].detail.nextDocument, nextDocument);
  assert.equal(lifecycle[1], 'old scope abort');
  assert.deepEqual(opened, ['wss://example.test/app/ACC/pages', 'wss://example.test/app/ACC/plans']);
  assert.deepEqual(closed, ['wss://example.test/app/ACC/pages']);
  connection.receive(JSON.stringify({run: previousRun, frame: {t: 'snapshot', html: 'retired private data'}}));
  assert.deepEqual(rendered, [], 'frames from the retired page cannot change its replacement');
});

test('a retired page socket cannot redirect its replacement', () => {
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  const sockets = [], redirects = [];
  const context = {TextEncoder, TextDecoder, URL, queueMicrotask: () => {},
    location: {href: 'https://example.test/app/'}, document: {documentElement: {getAttribute: () => '/app'}}};
  vm.runInNewContext(runtime.replace(bootstrap, 'globalThis.fixture={Connection:me};'), context);
  const connection = new context.fixture.Connection(() => '/app/', error => {throw error;}, () => {
    const listeners = {};
    const socket = {readyState: 1, send() {}, close() {},
      addEventListener: (name, listener) => {listeners[name] = listener;}, listeners};
    sockets.push(socket);
    return socket;
  }, destination => redirects.push(destination));
  connection.join({});
  connection.reset();
  connection.join({});
  sockets[0].listeners.message({data: JSON.stringify({frame: {t: 'redirect', location: '/app/retired'}})});
  assert.deepEqual(redirects, [], 'queued retirement frames belong to the old physical socket');
  sockets[1].listeners.message({data: JSON.stringify({frame: {t: 'redirect', location: '/app/current'}})});
  assert.deepEqual(redirects, ['/app/current']);
});

function navigationFixture(listener) {
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  const events = new EventTarget();
  const commits = [], loads = [], nextDocument = {title: 'destination'};
  events.addEventListener('topcoat:before-navigation-commit', listener);
  class CustomEvent extends Event {
    constructor(name, options = {}) { super(name, options); this.detail = options.detail; }
  }
  const context = {TextEncoder, TextDecoder, URL, AbortController, CustomEvent,
    document: events, location: {href: 'https://example.test/app/', assign: value => loads.push(value)},
    DOMParser: class { parseFromString() { return nextDocument; } }};
  vm.runInNewContext(runtime.replace(bootstrap, 'globalThis.fixture={Navigation:G};'), context);
  const navigation = Object.create(context.fixture.Navigation.prototype);
  navigation.runtime = {page: {}};
  navigation.canCommit = () => true;
  navigation.commit = (...args) => commits.push(args);
  const controller = new AbortController();
  const request = {controller, head: Promise.resolve({frames: true, url: '/app/ACC/pages'}),
    async *frames() { yield {t: 'snapshot', html: '<html></html>'}; },
    abort() { controller.abort(); }};
  return {navigation, request, commits, loads, nextDocument};
}

test('native navigation waits for page owners before committing history', async () => {
  let release, detail;
  const barrier = new Promise(resolve => { release = resolve; });
  const fixture = navigationFixture(event => { detail = event.detail; event.detail.waitUntil(barrier); });
  const pending = fixture.navigation.follow(fixture.request,
    new URL('https://example.test/app/ACC/pages'), 'push', null, () => true);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(fixture.commits.length, 0, 'the current page remains mounted while its owner finishes');
  assert.equal(detail.url.href, 'https://example.test/app/ACC/pages');
  assert.equal(detail.mode, 'push');
  assert.equal(detail.nextDocument, fixture.nextDocument);
  assert.equal(detail.signal, fixture.request.controller.signal);
  release();
  await pending;
  assert.equal(fixture.commits.length, 1);
  assert.deepEqual(fixture.loads, []);
});

for (const cancellation of ['reject', 'abort', 'supersede', 'preventDefault']) {
  test(`native navigation ${cancellation} leaves the current page and history intact`, async () => {
    let current = true;
    let fixture;
    fixture = navigationFixture(event => {
      if (cancellation === 'reject') event.detail.waitUntil(Promise.reject(new Error('owner failed')));
      if (cancellation === 'abort') {
        event.detail.waitUntil(new Promise(() => {}));
        queueMicrotask(() => fixture.request.abort());
      }
      if (cancellation === 'supersede') {
        event.detail.waitUntil(Promise.resolve().then(() => { current = false; }));
      }
      if (cancellation === 'preventDefault') event.preventDefault();
    });
    await fixture.navigation.follow(fixture.request,
      new URL('https://example.test/app/ACC/pages'), 'push', null, () => current);
    assert.equal(fixture.commits.length, 0);
    assert.deepEqual(fixture.loads, [], 'owner cancellation must not bypass the owner with a hard load');
    assert.equal(fixture.request.controller.signal.aborted, true);
  });
}

test('programmatic navigation uses the same native controller as links', async () => {
  const bootstrap = 'var et=new ye;et.start(document);et.page.listenForDevRefresh();';
  const context = {TextEncoder, TextDecoder, URL, AbortController,
    document: {documentElement: {getAttribute: () => '/app'}}, location: {href: 'https://example.test/app/'}};
  vm.runInNewContext(runtime.replace(bootstrap, 'globalThis.fixture={Runtime:ye};'), context);
  const instance = new context.fixture.Runtime();
  const calls = [];
  instance.navigation.navigate = (url, mode) => { calls.push([url.href, mode]); return Promise.resolve(); };
  await instance.context.navigate('/app/ACC/pages');
  assert.deepEqual(calls, [['https://example.test/app/ACC/pages', 'push']]);
});
