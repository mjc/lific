// Port of pinned 9683d38 e2e/sidebar.ts:654. Only fixture/transport adapters change.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const path=require('node:path');
const readline=require('node:readline');
const {mountedProxy,launchBrowser}=require(path.join(process.cwd(),'src/topcoat/native/browser_fixture.cjs'));
const {attr}=require('./sidebar.browser.fixture.cjs');
const upstream=new URL(process.argv[2]),token=process.argv[3];
const inputLines=readline.createInterface({input:process.stdin});
let sequence=0;
const pending=new Map();inputLines.on('line',line=>{const response=JSON.parse(line);const resolve=pending.get(response.id);if(resolve){pending.delete(response.id);resolve(response);}});
function control(action,fields={}){const id=++sequence;return new Promise(resolve=>{pending.set(id,resolve);process.stdout.write(`CONTROL ${JSON.stringify({action,id,...fields})}\n`);});}
async function action(page,surface,name,choice){const trigger=surface.getByRole('button',{name,exact:true});await trigger.focus();await trigger.click();await page.getByRole('menuitem',{name:choice,exact:true}).click();}

test('group create rename failure retry Cancel Escape',async t=>{
 const browser=await launchBrowser();
 try{
  for(const prefix of ['', '/app','/ACC'])for(const phone of [false,true])await t.test(`${prefix||'root'} ${phone?'phone':'desktop'}`,async()=>{
   const baseline=await control('reset');
   const proxy=await mountedProxy(upstream,prefix);
   const context=await browser.newContext({viewport:phone?{width:360,height:740}:{width:1000,height:760},isMobile:phone,hasTouch:phone});
   await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
   const errors=[],calls=[],requests=[];let release;
   try{
    const page=await context.newPage();page.setDefaultTimeout(5000);
    page.on('pageerror',error=>errors.push(error.message));
    page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});
    page.on('request',request=>{const url=new URL(request.url());requests.push(url.pathname);if(url.pathname.endsWith('/__native_sidebar/apply'))calls.push(JSON.parse(request.postData()));});
    assert.equal((await page.goto(`${proxy.origin}${prefix}/ACC/overview`)).status(),200);
    await page.locator('.native-overview').waitFor();
    if(phone)await page.getByRole('button',{name:'Open navigation',exact:true}).click();
    const surface=phone?page.locator('[data-native-mobile-root]'):page.getByRole('complementary',{name:'Workspace sidebar',exact:true});
    await surface.getByRole('button',{name:'New project or group',exact:true}).waitFor();
    const groupActions=name=>`Actions for ${phone?'':'group '}${name}`;
    const input=surface.getByRole('textbox',{name:'Group name',exact:true});
    for(const mode of ['create','rename']){
     const start=async()=>{await action(page,surface,mode==='create'?'New project or group':groupActions('Work'),mode==='create'?'New group':'Rename');await input.waitFor();};
     // Retain both original cancellation paths and also exercise native
     // keyboard activation of Cancel: Enter must not submit this draft.
     for(const cancel of ['Cancel','Escape','Cancel Enter']){
      if(cancel==='Cancel Enter')await page.evaluate(()=>{
       const original=window.requestAnimationFrame.bind(window);
       window.nativeInitialFocusFrames=[];
       window.releaseNativeInitialFocus=()=>{window.requestAnimationFrame=original;for(const frame of window.nativeInitialFocusFrames.splice(0))frame(performance.now());};
       window.requestAnimationFrame=frame=>{
        if(frame.toString().includes('node.select()')){window.nativeInitialFocusFrames.push(frame);return 0;}
        return original(frame);
       };
      });
      await start();await input.fill('Discard this draft');
      const before=calls.length,dbBefore=await control('inspect');
      if(cancel==='Escape')await input.press('Escape');
      else if(cancel==='Cancel Enter'){
       const cancelButton=surface.getByRole('button',{name:'Cancel',exact:true});
       await cancelButton.focus();
       assert.equal(await cancelButton.evaluate(el=>document.activeElement===el),true);
       await page.waitForFunction(()=>window.nativeInitialFocusFrames.length>0);
       await page.evaluate(()=>window.releaseNativeInitialFocus());
       assert.equal(await cancelButton.evaluate(el=>document.activeElement===el),true,
        'Delayed initial editor focus preserves a newer keyboard focus choice.');
       await cancelButton.press('Enter');
      }else await surface.getByRole('button',{name:'Cancel',exact:true}).click();
      await input.waitFor({state:'hidden'});
      assert.equal(calls.length,before,`${cancel} does not invoke the real native write.`);
      const dbAfter=await control('inspect');assert.deepEqual(dbAfter.groups,dbBefore.groups,`${cancel} has no database writes.`);
      const trigger=surface.getByRole('button',{name:mode==='create'?'New project or group':groupActions('Work'),exact:true});
      assert.equal(await trigger.evaluate(el=>document.activeElement===el),true,`${cancel} restores the edit trigger`);
     }
     await start();const name=mode==='create'?'Research':'Renamed work';
     await input.fill(name);await input.press('Tab');
     assert.equal(await input.inputValue(),name,'Blur must not save or discard');
     const before=calls.length;
     await control('conflict',{name,enabled:true});
     let arrived;const held=new Promise(resolve=>{arrived=resolve;});
     const gate=new Promise(resolve=>{release=resolve;});
     await page.route('**/__native_sidebar/apply',async route=>{arrived();await gate;await route.continue();},{times:1});
     await surface.getByRole('button',{name:'Save',exact:true}).click();await held;
     await attr(input,'disabled','');
     assert.equal(await surface.getByRole('button',{name:'Saving…',exact:true}).isDisabled(),true);
     release();release=undefined;
     const alert=surface.getByRole('alert');await alert.waitFor();
     assert.match(await alert.innerText(),/a group named '.*' already exists/,'Actual shared policy returns the safe uniqueness conflict.');
     assert.equal(await input.inputValue(),name);
     await attr(input,'aria-invalid','true');
     assert.equal(await input.evaluate(el=>document.activeElement===el),true);
     await control('conflict',{name,enabled:false});
     await input.press('Enter');await input.waitFor({state:'hidden'});
     await surface.getByRole('button',{name:groupActions(name),exact:true}).waitFor();
     const writes=calls.slice(before).map(args=>{const frozen=JSON.parse(args[1]);return (frozen.write??frozen).SaveGroup;});
     assert.deepEqual(writes.map(write=>({name:write.name})),[{name},{name}]);
     assert.equal(writes.length,2);
     if(mode==='rename')assert.deepEqual(writes.map(write=>write.target),[{Existing:baseline.work},{Existing:baseline.work}]);
     else assert.deepEqual(writes.map(write=>write.target),[{New:{project:null}},{New:{project:null}}]);
     const db=await control('inspect');assert.equal(db.groups.filter(group=>group.name===name).length,1,'Retry commits once.');
    }
    const scripts=await page.locator('script[src]').evaluateAll(elements=>elements.map(element=>element.src));
    assert.equal(scripts.length,1);assert.ok(scripts[0].includes('__topcoat-runtime'));
    assert.equal(requests.some(url=>url.split('/').includes('api')),false,'The native sidebar does not call frontend REST.');
    assert.deepEqual(errors,[]);
   }finally{if(release)release();await context.close();await proxy.close();}
  });
 }finally{await browser.close();inputLines.close();}
});
