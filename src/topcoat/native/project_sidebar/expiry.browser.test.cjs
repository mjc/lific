// Additive genuine host-Instant receipt expiry. Existing recovery's twelve cases stay unchanged.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const readline=require('node:readline');
const {mountedProxy,launchBrowser}=require(path.join(process.cwd(),'src/topcoat/native/browser_fixture.cjs'));
const upstream=new URL(process.argv[2]),token=process.argv[3];
const output='/tmp/lific-native-sidebar-expiry';
const lines=readline.createInterface({input:process.stdin});let sequence=0;const pending=new Map();
lines.on('line',line=>{const response=JSON.parse(line),resolve=pending.get(response.id);if(resolve){pending.delete(response.id);resolve(response);}});
function control(action,extra={}){const id=++sequence;return new Promise(resolve=>{pending.set(id,resolve);process.stdout.write(`CONTROL ${JSON.stringify({action,id,...extra})}\n`);});}
async function inspect(){const {id,...snapshot}=await control('inspect');return snapshot;}

test('expired committed receipt offers real page reload without resubmitting the frozen change',async t=>{
 fs.mkdirSync(output,{recursive:true});const browser=await launchBrowser();
 try{for(const prefix of ['', '/app','/ACC'])for(const phone of [false,true])await t.test(`${prefix||'root'} ${phone?'phone':'desktop'}`,async()=>{
  await control('reset');const proxy=await mountedProxy(upstream,prefix);
  const context=await browser.newContext({viewport:phone?{width:360,height:740}:{width:1000,height:760},isMobile:phone,hasTouch:phone});
  await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
  let release,page;const evidence={prefix,phone,requests:[],calls:[],errors:[]};
  try{
   page=await context.newPage();page.setDefaultTimeout(15000);
   page.on('pageerror',error=>evidence.errors.push(error.message));
   page.on('request',request=>{const pathname=new URL(request.url()).pathname;evidence.requests.push(pathname);if(pathname.endsWith('/__native_sidebar/apply'))evidence.calls.push(JSON.parse(request.postData())[1]);});
   const url=`${proxy.origin}${prefix}/ACC/overview`;
   assert.equal((await page.goto(url)).status(),200);await page.locator('.native-overview').waitFor();
   if(phone)await page.getByRole('button',{name:'Open navigation',exact:true}).click();
   const surface=phone?page.locator('[data-native-mobile-root]'):page.getByRole('complementary',{name:'Workspace sidebar',exact:true});
   await surface.getByRole('button',{name:'New project or group',exact:true}).click();await page.getByRole('menuitem',{name:'New group',exact:true}).click();
   const input=surface.getByRole('textbox',{name:'Group name',exact:true}),name='Committed before expiry';await input.fill(name);
   let arrived;const held=new Promise(resolve=>{arrived=resolve;}),gate=new Promise(resolve=>{release=resolve;});
   await page.route('**/__native_sidebar/apply',async route=>{
     const response=await route.fetch();assert.equal(response.status(),200,'Actual production commit finishes before the real reply is lost.');
     arrived();await gate;await route.abort('failed');
   },{times:1});
   await input.press('Enter');await held;assert.equal(await input.isDisabled(),true);
   evidence.committed=await inspect();assert.equal(evidence.committed.groups.length,1);assert.equal(evidence.committed.groups[0].name,name);assert.equal(evidence.committed.events,1);
   assert.equal(evidence.calls.length,1);const frozen=JSON.parse(evidence.calls[0]);assert.match(frozen.receipt,/^[a-f0-9]{48}$/);
   evidence.expiry=await control('expire',{key:frozen.receipt});assert.equal(evidence.expiry.expired,true,'Host expiry modifies the same Applied record used by production recover/finish.');
   release();release=undefined;
   const warning=surface.getByRole('alert').filter({hasText:"Couldn't confirm the change"});await warning.waitFor();
   assert.equal(await input.isDisabled(),true,'Unknown committed outcome retains the frozen operation instead of reopening Save.');
   assert.equal(await input.inputValue(),name);
   assert.equal(await surface.getByRole('button',{name:'Saving…',exact:true}).isDisabled(),true);
   const recoverBefore=evidence.requests.filter(url=>url.endsWith('/__native_sidebar/recover')).length;
   const recovered=page.waitForResponse(response=>new URL(response.url()).pathname.endsWith('/__native_sidebar/recover'));
   await surface.getByRole('button',{name:'Confirm change',exact:true}).click();await recovered;
   await page.waitForFunction(()=>[...document.querySelectorAll('[role=alert]')].some(node=>node.textContent.includes("Couldn't confirm the change")&&node.getClientRects().length));
   const recoverAfter=evidence.requests.filter(url=>url.endsWith('/__native_sidebar/recover')).length;
   assert.ok(recoverAfter>recoverBefore,'Confirm consults the same expired receipt.');
   evidence.afterConfirm=await inspect();assert.deepEqual(evidence.afterConfirm,evidence.committed,'Repeated confirmation cannot create another group or event.');
   assert.equal(evidence.calls.length,1);
   const reload=surface.getByRole('button',{name:'Reload page',exact:true});
   assert.equal(await reload.count(),1,'An unavailable receipt provides actual page reload recovery.');
   assert.match(await warning.innerText(),/reload/i,'Unknown outcome explains the safe reload action.');
   const cookiesBefore=await context.cookies(proxy.origin);const oldDocument=await page.evaluate(()=>performance.timeOrigin);
   await Promise.all([page.waitForNavigation({waitUntil:'domcontentloaded'}),reload.click()]);
   assert.equal(page.url(),url);assert.notEqual(await page.evaluate(()=>performance.timeOrigin),oldDocument,'Recovery reconstructs the real document.');
   await page.locator('.native-overview').waitFor();
   if(phone){
    const opener=page.getByRole('button',{name:'Open navigation',exact:true});
    if(await opener.getAttribute('aria-expanded')==='false')await opener.click();
    await page.getByRole('dialog',{name:'Workspace navigation',exact:true}).waitFor();
   }
   await surface.getByRole('button',{name:`Actions for ${phone?'':'group '}${name}`,exact:true}).waitFor();
   assert.equal(await surface.getByRole('textbox',{name:'Group name',exact:true}).count(),0,'Fresh owner has no disabled old editor.');
   assert.equal(await surface.getByRole('button',{name:'Confirm change',exact:true}).isVisible(),false);
   assert.equal(evidence.calls.length,1,'Reload never resubmits the frozen write.');
   evidence.final=await inspect();assert.deepEqual(evidence.final,evidence.committed);
   const cookiesAfter=await context.cookies(proxy.origin);assert.equal(cookiesAfter.find(cookie=>cookie.name==='lific_token').value,cookiesBefore.find(cookie=>cookie.name==='lific_token').value);
   assert.equal(evidence.requests.some(url=>url.split('/').includes('api')),false);assert.deepEqual(evidence.errors,[]);
  }catch(error){evidence.failure={message:error.message,stack:error.stack};throw error;}
  finally{if(release)release();if(page)await page.screenshot({path:path.join(output,`${prefix.slice(1)||'root'}-${phone?'phone':'desktop'}.png`),fullPage:true}).catch(()=>{});fs.writeFileSync(path.join(output,`${prefix.slice(1)||'root'}-${phone?'phone':'desktop'}.json`),JSON.stringify(evidence,null,2));await context.close();await proxy.close();}
 });}finally{await browser.close();lines.close();}
});
