// Real browser transport fault regression; domain outcomes come from SQLite and RealtimeHub.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const path=require('node:path');
const fs=require('node:fs');
const {tmpdir}=require('node:os');
const readline=require('node:readline');
const {mountedProxy,launchBrowser}=require(path.join(process.cwd(),'src/topcoat/native/browser_fixture.cjs'));
const upstream=new URL(process.argv[2]),token=process.argv[3];
const inputLines=readline.createInterface({input:process.stdin});
let sequence=0;const pending=new Map();
inputLines.on('line',line=>{const response=JSON.parse(line),resolve=pending.get(response.id);if(resolve){pending.delete(response.id);resolve(response);}});
function control(action){const id=++sequence;return new Promise(resolve=>{pending.set(id,resolve);process.stdout.write(`CONTROL ${JSON.stringify({action,id})}\n`);});}
test('before-send abort and lost committed reply recover without duplicate groups',async t=>{
 const browser=await launchBrowser();
 try{
  for(const prefix of ['', '/app','/ACC'])for(const phone of [false,true])for(const afterCommit of [false,true])await t.test(`${prefix||'root'} ${phone?'phone':'desktop'} ${afterCommit?'lost reply':'before send'}`,async()=>{
   await control('reset');const proxy=await mountedProxy(upstream,prefix);
   const context=await browser.newContext({viewport:phone?{width:360,height:740}:{width:1000,height:760},isMobile:phone,hasTouch:phone});
   await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
   let release,page;const errors=[],calls=[],requests=[];try{
    page=await context.newPage();page.setDefaultTimeout(15000);
    page.on('pageerror',error=>errors.push(error.message));
    page.on('request',request=>{const pathname=new URL(request.url()).pathname;requests.push(pathname);if(pathname.endsWith('/__native_sidebar/apply'))calls.push(JSON.parse(request.postData())[1]);});
    assert.equal((await page.goto(`${proxy.origin}${prefix}/ACC/overview`)).status(),200);await page.locator('.native-overview').waitFor();
    if(phone)await page.getByRole('button',{name:'Open navigation',exact:true}).click();
    const surface=phone?page.locator('[data-native-mobile-root]'):page.getByRole('complementary',{name:'Workspace sidebar',exact:true});
    const trigger=surface.getByRole('button',{name:'New project or group',exact:true});await trigger.click();await page.getByRole('menuitem',{name:'New group',exact:true}).click();
    const input=surface.getByRole('textbox',{name:'Group name',exact:true}),name=afterCommit?'Committed once':'Retry once';await input.fill(name);
    await input.evaluate(el=>el.setSelectionRange(4,8,'backward'));
    let arrived;const held=new Promise(resolve=>{arrived=resolve;}),gate=new Promise(resolve=>{release=resolve;});
    await page.route('**/__native_sidebar/apply',async route=>{
      if(afterCommit){const response=await route.fetch();assert.equal(response.status(),200,'The real server commits before its response is discarded.');}
      arrived();await gate;await route.abort('failed');
    },{times:1});
    await input.press('Enter');await held;
    await surface.getByRole('button',{name:'Saving…',exact:true}).waitFor();
    assert.equal(await input.isDisabled(),true);assert.equal(await surface.getByRole('button',{name:'Saving…',exact:true}).isDisabled(),true);
    const during=await control('inspect');assert.equal(during.groups.length,afterCommit?1:0);assert.equal(during.events,afterCommit?1:0);
    release();release=undefined;
    if(afterCommit){
      await input.waitFor({state:'hidden'});await surface.getByRole('button',{name:`Actions for ${phone?'':'group '}${name}`,exact:true}).waitFor();
      assert.equal(await trigger.evaluate(el=>document.activeElement===el),true,'Recovered commit returns focus to its original trigger.');
      assert.equal(calls.length,1,'Recovery replays the receipt; it does not send another domain mutation.');
    }else{
      // Both responsive editors exist; wait on the editor selected from this surface.
      const editorId=await input.getAttribute('id');assert.ok(editorId);
      await page.waitForFunction(id=>{const input=document.getElementById(id);return input&&!input.disabled;},editorId);
      const alert=surface.getByRole('alert');await alert.waitFor();assert.match(await alert.innerText(),/wasn't sent.*[Tt]ry again/);
      assert.equal(await input.inputValue(),name);assert.equal(await input.evaluate(el=>document.activeElement===el),true);
      assert.deepEqual(await input.evaluate(el=>[el.selectionStart,el.selectionEnd,el.selectionDirection]),[4,8,'backward'],'Transport recovery retains the draft selection.');
      const stopped=await control('inspect');assert.equal(stopped.groups.length,0);assert.equal(stopped.events,0);
      await input.press('Enter');await input.waitFor({state:'hidden'});assert.equal(calls.length,2);
      const writes=calls.map(value=>{const frozen=JSON.parse(value);return(frozen.write??frozen).SaveGroup;});assert.deepEqual(writes.map(write=>write.name),[name,name]);
    }
    const final=await control('inspect');assert.equal(final.groups.length,1);assert.equal(final.groups[0].name,name);assert.equal(final.events,1,'Only one successful shared mutation publishes its event.');
    assert.equal(requests.some(url=>url.split('/').includes('api')),false);assert.deepEqual(errors,[]);
   }catch(error){
    const output=path.join(tmpdir(),'lific-native-sidebar-recovery');fs.mkdirSync(output,{recursive:true});
    const name=`${prefix.replaceAll('/','_')||'root'}-${phone?'phone':'desktop'}-${afterCommit?'lost-reply':'before-send'}`;
    if(page){
      await page.screenshot({path:path.join(output,`${name}-failure.png`),fullPage:true}).catch(()=>{});
      fs.writeFileSync(path.join(output,`${name}-failure.html`),await page.content().catch(()=>''));
      const editors=await page.locator('[aria-label="Group name"]').evaluateAll(elements=>elements.map(el=>({id:el.id,disabled:el.disabled,
        visible:!!el.getClientRects().length,value:el.value,focused:document.activeElement===el,
        selection:[el.selectionStart,el.selectionEnd,el.selectionDirection]}))).catch(()=>[]);
      fs.writeFileSync(path.join(output,`${name}-failure.json`),JSON.stringify({error:error.stack,errors,calls,requests,editors},null,2));
    }
    throw error;
   }finally{if(release)release();await context.close();await proxy.close();}
  });
 }finally{await browser.close();inputLines.close();}
});
