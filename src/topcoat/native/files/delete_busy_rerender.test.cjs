'use strict';
const assert=require('node:assert/strict');
const readline=require('node:readline');
const {handlerFixture}=require('../handler_fixture.cjs');
const inputLines=readline.createInterface({input:process.stdin,crlfDelay:Infinity})[Symbol.asyncIterator]();
async function line(){const item=await inputLines.next();assert.equal(item.done,false);return JSON.parse(item.value)}
const scalar=wire=>{while(wire&&typeof wire==='object'&&Object.hasOwn(wire,'v'))wire=wire.v;return wire};
(async()=>{
 const input=await line(),requests=[],notifications=[];let settleFetch;
 const fixture=handlerFixture(input.signals,(url,options)=>{
  requests.push({path:new URL(url,'http://localhost').pathname,body:JSON.parse(options.body)});
  return new Promise(resolve=>{settleFetch=reply=>resolve({ok:true,status:200,json:async()=>reply})});
 },input.browser_source);
 fixture.context.document.documentElement.getAttribute=name=>name==='data-topcoat-runtime-prefix'?input.mount:'';
 fixture.context.CustomEvent=class extends Event{constructor(type,options={}){super(type,options);this.detail=options.detail}};
 fixture.context.window.dispatchEvent=event=>{if(event.type==='lific:native-toast-success'||event.type==='lific:native-toast-error')notifications.push({type:event.type,detail:JSON.parse(JSON.stringify(event.detail.dehydrate()))});return true};
 const button={getAttribute:name=>name==='data-native-files-confirm-delete'?input.button.id:name==='data-native-files-delete-success'?input.button.success:null,closest:selector=>selector==='button[data-native-files-confirm-delete]'?button:null};
 const root={contains:node=>!input.detached&&node===button};
 const event=fixture.cx.event({type:'click',target:button,currentTarget:root});
 fixture.handler(input.root_handler)(event);
 for(let i=0;i<30;i++)await Promise.resolve();
 assert.equal(requests.length,1);
 assert.equal(requests[0].path,`${input.mount}/__native_files/delete`);
 assert.deepEqual(requests[0].body,input.expected_arguments);
 const pending=Object.fromEntries(Object.keys(input.signals).map(id=>[id,fixture.cx.signal(id).get().dehydrate()]));
 const changed=Object.keys(input.signals).filter(id=>scalar(input.signals[id])===false&&scalar(pending[id])===true);
 assert.equal(changed.length,1,'delete marks shared parent busy before awaiting');
 process.stdout.write(`${JSON.stringify({stage:'pending',signals:pending,busy_id:changed[0]})}\n`);
 const control=await line();
 input.detached=true;
 fixture.handler(input.root_handler)(event);
 assert.equal(requests.length,1,'the old detached confirmation cannot queue a duplicate action');
 settleFetch(control.reply);
 for(let i=0;i<80;i++)await Promise.resolve();
 const final=Object.fromEntries(Object.keys(input.signals).map(id=>[id,fixture.cx.signal(id).get().dehydrate()]));
 assert.equal(scalar(final[changed[0]]),false,'parent completion clears busy after body shard replacement');
 assert.deepEqual(notifications,[{type:'lific:native-toast-error',detail:input.error_request}]);
 process.stdout.write(`${JSON.stringify({stage:'complete',requests,notifications,signals:final})}\n`);
})().catch(error=>{process.stderr.write(`${error.stack}\n`);process.exitCode=1;});
