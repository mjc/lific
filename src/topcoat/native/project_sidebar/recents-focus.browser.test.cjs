// Actual native workspace and sidebar only. The full-domain recents test stays separate.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const path=require('node:path');
const readline=require('node:readline');
const {mountedProxy,launchBrowser}=require(path.join(process.cwd(),'src/topcoat/native/browser_fixture.cjs'));
const upstream=new URL(process.argv[2]),token=process.argv[3],scenario=process.argv[4],seed=JSON.parse(process.argv[5]);
const input=readline.createInterface({input:process.stdin});
let pending;
input.on('line',line=>{assert.ok(pending);const resolve=pending;pending=undefined;resolve(JSON.parse(line));});
function control(action,values={}){assert.equal(pending,undefined);return new Promise(resolve=>{pending=resolve;process.stdout.write(`RECENTS_CONTROL ${JSON.stringify({action,...values})}\n`);});}
const procedure=(proxy,prefix,name)=>`${proxy.origin}${prefix}/__native_sidebar/recents_${name}`;
const recent=page=>page.locator('[data-topcoat-recents]:visible').first();
function recentLink(page,href){return recent(page).locator(`a[data-recents-href="${href}"]`);}
async function openPhone(page,phone){
  if(!phone)return;
  const nav=page.locator('[data-native-mobile-nav]');
  if(!await nav.isVisible())await page.getByRole('button',{name:'Open navigation',exact:true}).click();
  await nav.waitFor({state:'visible'});
  if(await page.locator('[data-native-mobile-root]').isVisible())await page.getByRole('button',{name:'Open Visible project navigation',exact:true}).click();
}
async function showRows(page,phone){
  await openPhone(page,phone);
  const toggle=recent(page).getByRole('button',{name:'Recent issues',exact:true});await toggle.waitFor();
  const state=await toggle.evaluate(button=>{
    const id=button.getAttribute('aria-controls'),content=document.getElementById(id);
    return {expanded:button.getAttribute('aria-expanded'),busy:content?.getAttribute('aria-busy'),hidden:content?.hidden,controlled:!!content&&button.closest('[data-topcoat-recents]')===content.closest('[data-topcoat-recents]')};
  });
  assert.ok(state.expanded==='false'||state.expanded==='true','The actual recent disclosure exposes explicit collapsed/expanded ARIA state.');
  assert.ok(state.busy==='false'||state.busy==='true','The actual recent content exposes explicit idle/loading ARIA state.');
  assert.equal(state.controlled,true,'The disclosure controls content in its actual visible recent component.');
  assert.equal(state.hidden,state.expanded==='false','Actual content visibility agrees with the disclosure ARIA state.');
  if(state.expanded==='false')await toggle.click();
  await recent(page).locator('[data-recents-content]').waitFor();
  assert.equal(await toggle.getAttribute('aria-expanded'),'true','A real disclosure interaction opens the actual recent content.');
  await page.waitForFunction(()=>[...document.querySelectorAll('[data-topcoat-recents]')].some(root=>root.getClientRects().length&&root.querySelector('[data-recents-list] a')));
  assert.equal(await recent(page).locator('[data-recents-list] a').count(),5);
}
async function projection(page){return recent(page).locator('[data-recents-list] a').evaluateAll(nodes=>nodes.map(node=>({href:node.dataset.recentsHref,label:node.querySelector('[data-recents-label]').textContent,identifier:node.dataset.recentsIdentifier||null})));}
async function remember(page){await page.evaluate(()=>{window.recentsProofOwner=document.querySelector('.native-home-shell');const root=[...document.querySelectorAll('[data-topcoat-recents]')].find(node=>node.getClientRects().length);window.recentsProofList=root.querySelector('[data-native-recents-rows]');window.recentsProofRow=root.querySelector('[data-recents-list] a');});}
async function sameOwner(page){assert.ok(await page.evaluate(()=>document.querySelector('.native-home-shell')===window.recentsProofOwner),'The real native workspace owner remains mounted.');}
async function sameRows(page){assert.ok(await page.evaluate(()=>{const root=[...document.querySelectorAll('[data-topcoat-recents]')].find(node=>node.getClientRects().length);return window.recentsProofList.isConnected&&window.recentsProofRow.isConnected&&root.querySelector('[data-native-recents-rows]')===window.recentsProofList&&root.querySelector('[data-recents-list] a')===window.recentsProofRow;}),'An unchanged authorized recent list and its first link retain their actual nodes across the parent update.');}
async function settleFrames(page){await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));}
async function gate(page,url,{fetchFirst=false,account=seed.account}={}){
  let arrived,release,finished;
  const entered=new Promise(resolve=>arrived=resolve),blocked=new Promise(resolve=>release=resolve),done=new Promise(resolve=>finished=resolve);
  await page.route(url,async route=>{
    try{
      const request=route.request();assert.equal(request.method(),'POST');
      assert.ok((await request.headerValue('cookie')).includes(`lific_token=${token}`),'The held procedure receives the genuine current browser session cookie.');
      assert.equal(await request.headerValue('authorization'),null,'No stored bearer substitutes for the current cookie.');
      const args=JSON.parse(request.postData());assert.deepEqual(args[0],{t:'i64',bits:64,v:String(account)},'The actual production procedure receives the immutable fixture account through its native typed surrogate.');
      const response=fetchFirst?await route.fetch():undefined;
      if(response)assert.equal(response.status(),200,'The held bytes are an actual successful server response.');
      arrived({args,response});await blocked;
      const actual=response||await route.fetch();assert.equal(actual.status(),200);
      await route.fulfill({response:actual});finished(null);
    }catch(error){finished(error);arrived({error});}
  },{times:1});
  return {entered,release,done};
}
async function delivered(page,url,held){
  const response=page.waitForResponse(reply=>reply.url()===url&&reply.request().method()==='POST');
  held.release();assert.equal((await response).status(),200);const error=await held.done;if(error)throw error;
}
async function nativeContract(page,requests,errors){
  const scripts=await page.locator('script[src]').evaluateAll(nodes=>nodes.map(node=>node.src));
  assert.equal(scripts.length,1);assert.ok(scripts[0].includes('__topcoat-runtime'));
  assert.equal(requests.some(request=>new URL(request.url).pathname.split('/').includes('api')),false,'All observed page and sidebar work uses the real native transport.');
  assert.equal(requests.some(request=>request.authorization),false);
  assert.equal(await page.evaluate(()=>localStorage.getItem('lific_token')),null);
  assert.deepEqual(errors,[]);
}
test(`native recents production ${scenario}`,async t=>{
  const browser=await launchBrowser();
  try{
    for(const prefix of ['', '/app','/ACC'])for(const phone of [false,true])await t.test(`${prefix||'root'} ${phone?'phone':'desktop'}`,async()=>{
      await control('reset');const proxy=await mountedProxy(upstream,prefix);
      const context=await browser.newContext({viewport:phone?{width:360,height:740}:{width:1100,height:800},isMobile:phone,hasTouch:phone,reducedMotion:'reduce'});
      const gates=[],requests=[],errors=[];
      try{
        await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
        const page=await context.newPage();page.setDefaultTimeout(15000);
        page.on('request',request=>requests.push({url:request.url(),type:request.resourceType(),authorization:request.headers().authorization}));
        page.on('pageerror',error=>errors.push(error.message));page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});
        if(scenario==='basic'){
          for(const [route,selector]of [['/','[data-native-home]'],['/ACC/overview','.native-overview'],['/projects/new','.native-project-create-page'],['/ACC/issues','[data-native-issue-list]']]){
            const response=await page.goto(`${proxy.origin}${prefix}${route}`);assert.equal(response.status(),200);const html=await response.text();
            assert.ok(html.includes('native-home-shell'));assert.ok(!html.includes('data-lific-session-state'));
            const scripts=[...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"/g)].map(match=>match[1]);assert.equal(scripts.length,1);assert.ok(scripts[0].includes('__topcoat-runtime'));
            await page.locator(selector).waitFor();
            assert.equal(await page.locator('[data-native-sidebar-project]').count(),1,'Only the fixture account’s visible project reaches desktop native chrome.');
            assert.equal(await page.getByText('Private hidden project',{exact:true}).count(),0);
            if(route==='/ACC/issues'){
              for(const expected of seed.rows){assert.ok(html.includes(`href="${prefix}${expected.href}"`));assert.ok(html.includes(expected.label));}
              await showRows(page,phone);assert.deepEqual(await projection(page),seed.rows);
              assert.equal(await recent(page).locator('.focus-title').first().isVisible(),false,'Pinned focus title stays hidden until its link receives visible keyboard focus.');
            }else assert.equal(await page.locator('[data-topcoat-recents]:visible').count(),0);
            await nativeContract(page,requests,errors);
          }
          return;
        }
        assert.equal((await page.goto(`${proxy.origin}${prefix}/ACC/issues`)).status(),200);await page.locator('[data-native-issue-list]').waitFor();
        await showRows(page,phone);assert.deepEqual(await projection(page),seed.rows);await remember(page);
        const documents=requests.filter(request=>request.type==='document').length;
        const redrawUrl=`${proxy.origin}${prefix}/__native_sidebar/${phone?'phone_panels':'desktop'}`;
        if(scenario==='catalog'){
          if(phone){await page.getByRole('button',{name:'Back to projects',exact:true}).click();await page.locator('[data-native-mobile-root]').waitFor();}
          const surface=phone?page.locator('[data-native-mobile-root]'):page.getByRole('complementary',{name:'Workspace sidebar',exact:true});
          await surface.getByRole('button',{name:'New project or group',exact:true}).click();await page.getByRole('menuitem',{name:'New group',exact:true}).click();
          const name=`Held recents ${prefix||'root'} ${phone?'phone':'desktop'}`;const field=surface.getByRole('textbox',{name:'Group name',exact:true});await field.fill(name);
          const parentRedraw=page.waitForResponse(response=>response.url()===redrawUrl&&response.request().method()==='POST'&&response.status()===200);await field.press('Enter');
          await surface.getByRole('button',{name:`Actions for ${phone?'':'group '}${name}`,exact:true}).waitFor();await (await parentRedraw).finished();await settleFrames(page);await showRows(page,phone);
          const inspected=await control('inspect');assert.equal(inspected.groups.filter(group=>group.name===name).length,1,'The catalog update committed through the real sidebar service.');
          await sameOwner(page);assert.deepEqual(await projection(page),seed.rows);await sameRows(page);
        }else{
          const readUrl=procedure(proxy,prefix,'read'),readGate=await gate(page,readUrl);gates.push(readGate);
          let expected=seed.rows;
          if(scenario==='focus'){const changed=await control('rename',{title:`Refreshed genuine row ${prefix||'root'} ${phone?'phone':'desktop'}`});expected=changed.rows;assert.equal(changed.role,'viewer');}
          // A normal actual recent link enters a real native issue, retaining the workspace.
          const parentRedraw=page.waitForResponse(response=>response.url()===redrawUrl&&response.request().method()==='POST'&&response.status()===200);
          const destination=seed.rows[0].href;await recentLink(page,destination).click();await page.waitForURL(`${proxy.origin}${prefix}${destination}`);
          await page.locator('[data-native-issue-editor]').waitFor();await (await parentRedraw).finished();await settleFrames(page);const read=(await readGate.entered);if(read.error)throw read.error;
          assert.equal(JSON.parse(read.args[1]).owner,seed.account);assert.equal(JSON.parse(read.args[1]).project[0],seed.project);
          await showRows(page,phone);await sameOwner(page);assert.equal(requests.filter(request=>request.type==='document').length,documents);
          if(scenario==='path'){
            assert.deepEqual(await projection(page),seed.rows);await sameRows(page);
            await delivered(page,readUrl,readGate);await page.waitForFunction(()=>[...document.querySelectorAll('[data-recents-content]')].some(node=>node.getClientRects().length&&node.getAttribute('aria-busy')==='false'));
            await sameRows(page);
          }else{
            const finishUrl=procedure(proxy,prefix,'finish'),finishGate=await gate(page,finishUrl,{fetchFirst:true});gates.push(finishGate);
            await recentLink(page,destination).focus();assert.equal(await recentLink(page,destination).evaluate(node=>document.activeElement===node),true);
            await delivered(page,readUrl,readGate);const finish=await finishGate.entered;if(finish.error)throw finish.error;
            assert.equal(finish.args[4],destination,'The genuine completion request captured the then-focused recent href.');
            assert.deepEqual(JSON.parse((await finish.response.json())[8]),[destination,destination],'The held production response contains the server-encoded domain focus target and captured source before the user moves.');
            const other=phone?page.getByRole('button',{name:'Back to projects',exact:true}):page.getByRole('button',{name:'New project or group',exact:true});
            await other.focus();assert.equal(await other.evaluate(node=>document.activeElement===node),true);
            await delivered(page,finishUrl,finishGate);
            await page.waitForFunction(({href,title})=>[...document.querySelectorAll(`[data-recents-href="${href}"] [data-recents-label]`)].some(node=>node.textContent===title),{href:destination,title:expected[0].label});
            await settleFrames(page);assert.deepEqual(await projection(page),expected);
            assert.equal(await other.evaluate(node=>document.activeElement===node),true,'A user focus choice made while genuine finish bytes are withheld survives the changed row render.');
          }
        }
        await nativeContract(page,requests,errors);
      }finally{for(const held of gates)held.release();await context.close();await proxy.close();}
    });
  }finally{await browser.close();input.close();}
});
