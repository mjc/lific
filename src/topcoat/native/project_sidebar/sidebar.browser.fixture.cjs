// Observation helpers for the actual production sidebar; no application model.
const assert=require('node:assert/strict');
const path=require('node:path');
const {mountedProxy,launchBrowser}=require(path.join(process.cwd(),'src/topcoat/native/browser_fixture.cjs'));
async function cookie(context,origin,token){await context.addCookies([{name:'lific_token',value:token,url:origin,httpOnly:true,sameSite:'Lax'}]);}
function observations(page){
  const requests=[],errors=[],pending=new Set();
  page.on('request',request=>{requests.push({url:request.url(),method:request.method(),type:request.resourceType(),authorization:request.headers().authorization,body:request.postData()});if(nativeRequest(request.url(),request.resourceType()))pending.add(request);});
  page.on('requestfinished',request=>pending.delete(request));
  page.on('requestfailed',request=>pending.delete(request));
  page.on('pageerror',error=>errors.push(error.message));
  page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});
  return {requests,errors,pending};
}
async function nativeContract(page,seen){
  const scripts=await page.locator('script[src]').evaluateAll(nodes=>nodes.map(node=>node.src));
  assert.equal(scripts.length,1);assert.ok(scripts[0].includes('__topcoat-runtime'));
  assert.equal(seen.requests.some(request=>new URL(request.url).pathname.split('/').includes('api')),false,'The real native sidebar does not call frontend REST.');
  assert.equal(seen.requests.some(request=>request.authorization),false,'The actual browser uses its current session cookie.');
  assert.equal(await page.evaluate(()=>localStorage.getItem('lific_token')),null);
  assert.equal(await page.getByText('Private hidden project',{exact:true}).count(),0);
  assert.deepEqual(seen.errors,[]);
}
async function openPhoneNavigation(page){
  const trigger=page.getByRole('button',{name:'Open navigation',exact:true});
  if(await trigger.getAttribute('aria-expanded')!=='true')await trigger.click();
  const nav=page.locator('[data-native-mobile-nav]');await nav.waitFor({state:'visible'});return nav;
}
async function surface(page,phone){
  if(!phone)return page.getByRole('complementary',{name:'Workspace sidebar',exact:true});
  await openPhoneNavigation(page);
  const root=page.locator('[data-native-mobile-root]');await root.waitFor();return root;
}
// Original master attr predicate keeps its exact four-second observation bound.
async function attr(locator,name,value){
  await locator.evaluate((element,args)=>new Promise((resolve,reject)=>{
    const end=performance.now()+4000;
    function check(){
      const actual=element.getAttribute(args.name);
      if(actual===args.value)resolve();
      else if(performance.now()>end)reject(new Error(JSON.stringify({id:element.id,name:args.name,expected:args.value,actual,workspaceOwnerRetained:window.nativeSidebarDisclosureOwner?document.querySelector('.native-home-shell')===window.nativeSidebarDisclosureOwner:null})));
      else requestAnimationFrame(check);
    }
    check();
  }),{name,value});
}
function nativeRequest(url,type){return (type==='fetch'||type==='xhr')&&new URL(url).pathname.split('/').some(segment=>segment.startsWith('__native_'));}
// Retain the original 150+150 ms idle observation on actual native business IO.
async function settle(page,seen){
  await page.waitForTimeout(150);
  const calls=seen.requests.filter(request=>nativeRequest(request.url,request.type)).length;
  await page.waitForTimeout(150);
  assert.equal(seen.requests.filter(request=>nativeRequest(request.url,request.type)).length,calls,'Native requests continued after the UI settled');
  assert.equal(seen.pending.size,0,'Native requests never completed');
  assert.deepEqual(seen.errors,[]);
}
// Original called() polls at 20 ms for at most 100 iterations.
async function called(seen,count){
  for(let index=0;index<100;index++){
    const calls=appliedRequests(seen);
    if(calls.length>=count)return calls.at(-1);
    await new Promise(resolve=>setTimeout(resolve,20));
  }
  throw new Error(`Missing actual native apply request (#${count})`);
}
async function bounded(promise,milliseconds,label){
  let timer;
  try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error(`${label} did not complete within ${milliseconds} ms`)),milliseconds);})]);}
  finally{clearTimeout(timer);}
}
async function action(page,tree,trigger,choice){
  const button=tree.getByRole('button',{name:trigger,exact:true});await button.focus();await button.click();
  await page.getByRole('menuitem',{name:choice,exact:true}).click();
}
function appliedRequests(seen){return seen.requests.filter(request=>new URL(request.url).pathname.endsWith('/__native_sidebar/apply'));}
function frozenWrite(request,account){
  assert.equal(request.method,'POST');const args=JSON.parse(request.body);
  assert.deepEqual(args[0],{t:'i64',bits:64,v:String(account)},'Native writes carry the immutable actual cookie account.');
  const frozen=JSON.parse(args[1]);assert.equal(typeof frozen.receipt,'string');assert.ok(frozen.receipt);
  return frozen.write;
}
// Two independent transport gates: first withhold the real request, then the
// actual server reply. Real SQLite and receipt execution remain untouched.
async function holdApply(page,url,token,account){
  let enter,dispatch,fetchDone,deliver,done;
  const entered=new Promise(resolve=>enter=resolve),dispatchGate=new Promise(resolve=>dispatch=resolve),fetched=new Promise(resolve=>fetchDone=resolve),deliveryGate=new Promise(resolve=>deliver=resolve),finished=new Promise(resolve=>done=resolve);
  await page.route(url,async route=>{
    try{
      const request=route.request();assert.equal(request.method(),'POST');
      assert.ok((await request.headerValue('cookie')).includes(`lific_token=${token}`));
      assert.equal(await request.headerValue('authorization'),null);
      const args=JSON.parse(request.postData());assert.deepEqual(args[0],{t:'i64',bits:64,v:String(account)});
      enter({args});await dispatchGate;
      const response=await route.fetch();assert.equal(response.status(),200,'The response is a real native receipt result, including its safe domain failure.');
      const wire=await response.json();assert.equal(typeof wire,'string');
      fetchDone({response,applied:JSON.parse(wire)});await deliveryGate;
      await route.fulfill({response});done(null);
    }catch(error){enter({error});fetchDone({error});done(error);try{await route.abort('failed');}catch{}}
  },{times:1});
  return {entered,fetched,finished,dispatch,deliver,release(){dispatch();deliver();}};
}
module.exports={openPhoneNavigation,mountedProxy,launchBrowser,cookie,observations,nativeContract,surface,attr,action,appliedRequests,frozenWrite,holdApply,settle,called,bounded};
