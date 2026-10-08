'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('./handler_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const flush = async () => { for (let i=0;i<12;i++) await Promise.resolve(); };
const unbox = value => {
  if (value && typeof value.dehydrate === 'function') return unbox(value.dehydrate());
  if (value && typeof value === 'object' && 'v' in value) return unbox(value.v);
  return value;
};

function makeFixture(api='success', legacy='success') {
  const copied=[], notifications=[], fields=[], timers=new Map(), completions=[];
  let now=0, nextTimer=0, legacyCalls=0, preference='';
  const f=handlerFixture(input.signals,async()=>{throw new Error('copy never fetches');},input.browser_source);
  const {context,cx}=f;
  context.setTimeout=(callback,delay)=>{const id=++nextTimer;timers.set(id,{callback,at:now+Number(delay)});return id;};
  context.clearTimeout=id=>timers.delete(id);
  context.CustomEvent=class extends context.Event {constructor(type,options={}){super(type);this.detail=options.detail;}};
  context.window.dispatchEvent=event=>{notifications.push(event);return true;};
  context.localStorage={getItem:()=>preference};
  context.navigator={};
  if(api!=='missing') context.navigator.clipboard={writeText:value=>{
    copied.push(value);
    if(api==='reject') return Promise.reject(new Error('clipboard denied'));
    if(api==='throw') throw new Error('clipboard unavailable');
    if(api==='pending') return new Promise(resolve=>completions.push(resolve));
    if(api==='pending_reject') return new Promise((_,reject)=>completions.push(reject));
    return Promise.resolve();
  }};
  context.document.createElement=tag=>{
    assert.equal(tag,'textarea');
    const field={value:'',style:{},removed:false,setAttribute(){},select(){},remove(){this.removed=true;}};
    fields.push(field);return field;
  };
  context.document.body={appendChild(){},removeChild:field=>{field.removed=true;}};
  context.document.execCommand=command=>{
    assert.equal(command,'copy');legacyCalls++;
    if(legacy==='throw') throw new Error('legacy copy failed');
    return legacy==='success';
  };
  const click=async source=>{
    const flags={stopped:false,prevented:false};
    const event=new context.Event('click');
    Object.assign(event,{target:{},currentTarget:{},preventDefault(){flags.prevented=true;},stopPropagation(){flags.stopped=true;}});
    const result=f.handler(source)(cx.event(event));
    if(!api.startsWith('pending')) await result;
    await flush();return flags;
  };
  const advance=async duration=>{
    now+=duration;
    for(const [id,timer] of [...timers]) if(timer.at<=now){timers.delete(id);timer.callback();}
    await flush();
  };
  const read=source=>unbox(f.handler(source));
  return {...f,copied,notifications,fields,timers,completions,click,advance,read,
    get legacyCalls(){return legacyCalls;},set preference(value){preference=value;}};
}

async function integration(checkEvents) {
  const f=makeFixture();
  const project=await f.click(input.project_handler);
  const identifier=await f.click(input.identifier_handler);
  assert.deepEqual(f.copied,[input.project_id,input.identifier]);
  if(checkEvents) for(const flags of [project,identifier]) assert.deepEqual(flags,{stopped:true,prevented:true});
  return {copied:f.copied,events_stopped:[project.stopped,identifier.stopped],events_prevented:[project.prevented,identifier.prevented]};
}

async function mainParity() {
  assert.ok(input.copy_hidden_binding,'Copy icon exposes its real reactive visibility');
  assert.ok(input.check_hidden_binding,'Confirmed copy exposes its real reactive checkmark visibility');
  assert.ok(input.project_copy_hidden_binding);assert.ok(input.project_check_hidden_binding);
  const check=(f,copied,project=false)=>{
    assert.equal(f.read(project?input.project_copy_hidden_binding:input.copy_hidden_binding),copied,'copy icon visibility follows confirmed completion');
    assert.equal(f.read(project?input.project_check_hidden_binding:input.check_hidden_binding),!copied,'checkmark visibility follows confirmed completion');
  };
  const independent=makeFixture();
  await independent.click(input.project_handler);check(independent,true,true);check(independent,false);
  await independent.advance(1000);await independent.click(input.identifier_handler);
  check(independent,true,true);check(independent,true);
  await independent.advance(500);check(independent,false,true);check(independent,true);
  await independent.advance(1000);check(independent,false,true);check(independent,false);
  const success=makeFixture();check(success,false);
  assert.deepEqual(await success.click(input.identifier_handler),{stopped:true,prevented:true});
  check(success,true);assert.equal(success.legacyCalls,0);assert.equal(success.notifications.length,0);
  await success.advance(1499);check(success,true);
  await success.advance(1);check(success,false);

  const repeat=makeFixture();await repeat.click(input.identifier_handler);await repeat.advance(1000);
  await repeat.click(input.identifier_handler);assert.equal(repeat.timers.size,1,'a new success replaces the old deadline');
  await repeat.advance(500);check(repeat,true);await repeat.advance(1000);check(repeat,false);

  for(const api of ['missing','reject','throw']) {
    const fallback=makeFixture(api);await fallback.click(input.identifier_handler);check(fallback,true);
    assert.equal(fallback.legacyCalls,1);assert.equal(fallback.fields[0].value,input.identifier);
    assert.ok(fallback.fields.every(field=>field.removed),'fallback always removes its temporary textarea');
    assert.equal(fallback.notifications.length,0);
  }
  for(const legacy of ['false','throw']) {
    const failure=makeFixture('reject',legacy);await failure.click(input.identifier_handler);check(failure,false);
    assert.ok(failure.fields.every(field=>field.removed),'failed legacy copy removes its textarea');
    assert.equal(failure.timers.size,0);
    assert.equal(failure.notifications.length,1);
    const event=failure.notifications[0];assert.equal(event.type,'lific:native-toast-error');
    const detail=unbox(event.detail);
    assert.equal(String(unbox(detail.account_id)),String(input.account_id));
    assert.equal(unbox(detail.message),"Couldn't copy to clipboard");
  }

  const pending=makeFixture('pending');await pending.click(input.identifier_handler);check(pending,false);
  pending.completions.shift()();await flush();check(pending,true);
  pending.controller.abort();assert.equal(pending.timers.size,0,'disposal clears the confirmation timer');

  const late=makeFixture('pending');await late.click(input.identifier_handler);late.controller.abort();
  late.completions.shift()();await flush();check(late,false);
  assert.equal(late.timers.size,0);assert.equal(late.notifications.length,0);
  const rejected=makeFixture('pending_reject','throw');
  await rejected.click(input.identifier_handler);rejected.controller.abort();
  rejected.completions.shift()(new Error('clipboard rejected after disposal'));await flush();
  check(rejected,false);
  assert.equal(rejected.legacyCalls,0,'a rejected clipboard request cannot begin fallback after disposal');
  assert.equal(rejected.fields.length,0);assert.equal(rejected.notifications.length,0);
  assert.equal(rejected.timers.size,0);
  const disposed=makeFixture();disposed.controller.abort();await disposed.click(input.identifier_handler);
  assert.deepEqual(disposed.copied,[]);assert.deepEqual(disposed.notifications,[]);
  return {passed:true};
}

async function listReturn() {
  const f=makeFixture();
  for(const [preference,label,href] of [['board','Board',input.board_href],['list','Issues',input.list_href]]) {
    f.preference=preference;
    f.handler(input.return_mount)();
    assert.equal(f.read(input.return_href_binding),href);
    assert.equal(f.read(input.return_title_binding),label);
  }
  return {passed:true};
}

async function run() {
  const result=input.phase==='main_parity'?await mainParity():input.phase==='issue_list_return'?await listReturn():await integration(input.phase!=='baseline');
  process.stdout.write(JSON.stringify(result));
}
run().catch(error=>{process.stderr.write(`${error.stack}\n`);process.exitCode=1;});
