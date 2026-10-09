const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {TextEncoder, TextDecoder} = require('node:util');
const {factory, mount} = JSON.parse(fs.readFileSync(0, 'utf8'));
const {fixtureRuntime} = require('./runtime_fixture.cjs');
const flush = async () => {for(let i=0;i<12;i++)await Promise.resolve();};

function fixture() {
  const document = new EventTarget();
  document.documentElement = {getAttribute: () => mount};
  const controller = new AbortController(), calls = [], pending = [], loads = [];
  const context = {TextEncoder, TextDecoder, AbortController, Event, document, queueMicrotask,
    location: {href:'http://localhost/old', assign: href => loads.push(href)},
    fetch: (url, options) => {calls.push({url, options});return new Promise((resolve,reject) => pending.push({resolve,reject}));}};
  vm.runInNewContext(fixtureRuntime(['Context', 'Registry']), context);
  const cx = Object.assign(new context.fixture.Context(new context.fixture.Registry()), {abortSignal:controller.signal});
  vm.runInNewContext(`cx=>(${factory})`, context)(cx)({});
  const before = ({account='9007199254740993', admin=true, shell=true, authority=''}={}) => {
    const signal = new AbortController(), waits = [];
    const event = new Event('topcoat:before-navigation-commit', {cancelable:true});
    event.detail = {url:new URL(`http://localhost${mount}/ACC/issues?view=active#selected`),
      mode:'push', signal:signal.signal, waitUntil: promise => waits.push(Promise.resolve(promise)),
      nextDocument:{querySelector: selector => {
        if(selector === '[data-native-project-authority]') {
          return authority ? {dataset:{nativeProjectAuthority:authority}} : null;
        }
        assert.equal(selector, '.native-home-shell');
        return shell ? {dataset:{accountId:account, accountAdmin:String(admin)}} : null;
      }}};
    document.dispatchEvent(event);
    const barrier = Promise.all(waits);
    barrier.catch(()=>{});
    return {signal, waits, barrier, event};
  };
  const answer = async verdict => {
    assert.equal(pending.length,1);
    pending.shift().resolve({ok:true, redirected:false, status:200, json:async()=>verdict});
    await flush();
  };
  return {before, answer, controller, calls, pending, loads};
}

test('commit waits for the mounted request with the incoming exact i64/admin baseline', async()=>{
  const f=fixture(), navigation=f.before();
  assert.equal(navigation.waits.length,1);
  let finished=false;
  navigation.barrier.then(()=>{finished=true;});
  await flush();
  assert.equal(finished,false);
  assert.equal(f.calls.length,1);
  assert.equal(f.calls[0].url,`${mount}/__native_workspace/authorize_navigation`);
  assert.deepEqual(JSON.parse(f.calls[0].options.body),[
    `${mount}/ACC/issues?view=active`,{t:'i64',bits:64,v:'9007199254740993'},true,'',
  ]);
  await f.answer('allow');
  await navigation.barrier;
  assert.equal(finished,true);
  assert.deepEqual(f.loads,[]);
});

test('commit sends the exact permission snapshot rendered by the destination',async()=>{
  for(const authority of ['{"project_id":17,"role":"viewer"}', 'invalid-json']) {
    const f=fixture(), navigation=f.before({authority});
    assert.equal(navigation.waits.length,1);
    await flush();
    assert.equal(JSON.parse(f.calls[0].options.body)[3],authority);
    await f.answer('authority-changed');
    await assert.rejects(navigation.barrier);
    assert.deepEqual(f.loads,[`http://localhost${mount}/ACC/issues?view=active#selected`]);
  }
});

test('denial and identity replacement reject cached commits and load the full intended URL',async()=>{
  for(const verdict of ['denied','identity-changed']) {
    const f=fixture(), navigation=f.before();
    await flush();await f.answer(verdict);
    await assert.rejects(navigation.barrier);
    assert.deepEqual(f.loads,[`http://localhost${mount}/ACC/issues?view=active#selected`]);
  }
});

test('late authority results from a superseded navigation cannot load its old target',async()=>{
  for(const retireScope of [false,true]) {
    const f=fixture(), navigation=f.before();
    await flush();
    (retireScope?f.controller:navigation.signal).abort();
    await f.answer('identity-changed');
    await navigation.barrier.catch(()=>{});
    assert.deepEqual(f.loads,[]);
  }
});

test('a retired scope removes the listener and canceled queued navigation starts no request',async()=>{
  const f=fixture(), navigation=f.before();
  navigation.signal.abort();
  await flush();await navigation.barrier.catch(()=>{});
  assert.equal(f.calls.length,0);
  f.controller.abort();
  assert.equal(f.before().waits.length,0);
});

test('transport failure rejects the pending commit',async()=>{
  const f=fixture(), navigation=f.before();
  await flush();f.pending.shift().reject(new Error('offline'));
  await assert.rejects(navigation.barrier);
  assert.deepEqual(f.loads,[]);
});

test('malformed incoming authority rejects an already registered commit barrier',async()=>{
  const f=fixture(), navigation=f.before({account:'invalid-i64'});
  assert.equal(navigation.waits.length,1);
  await assert.rejects(navigation.barrier);
  assert.equal(f.calls.length,0);
  assert.deepEqual(f.loads,[]);
});
