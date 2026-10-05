// Pinned main MobileNav/Layout contracts through actual native content and transport.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const readline=require('node:readline');
const {mountedProxy,launchBrowser,cookie,observations,nativeContract,attr,settle,bounded}=require('./project_sidebar/sidebar.browser.fixture.cjs');
const upstream=new URL(process.argv[2]),token=process.argv[3],scenario=process.argv[4],seed=JSON.parse(process.argv[5]);
const input=readline.createInterface({input:process.stdin});let pending;
input.on('line',line=>{assert.ok(pending);const resolve=pending;pending=undefined;resolve(JSON.parse(line));});
function control(action,values={}){assert.equal(pending,undefined);return new Promise(resolve=>{pending=resolve;process.stdout.write(`OWNER_CONTROL ${JSON.stringify({action,...values})}\n`);});}
const drawer=page=>page.getByRole('dialog',{name:'Workspace navigation',exact:true});
async function openProject(page,name){const nav=drawer(page);if(!await nav.isVisible())await page.getByRole('button',{name:'Open navigation',exact:true}).click();await page.getByRole('button',{name:`Open ${name} navigation`,exact:true}).click();await page.getByRole('button',{name:'Back to projects',exact:true}).waitFor();return nav;}
async function overview(page,origin,prefix,identifier){await page.waitForURL(`${origin}${prefix}/${identifier}/overview`);await page.locator('.native-overview').waitFor();await page.waitForFunction(expected=>document.querySelector('.native-overview__breadcrumb a')?.textContent===expected,identifier,{timeout:5000});assert.equal(await page.locator('.native-overview__breadcrumb a').textContent(),identifier);}
async function adopted(page,seen,label='Actual native region adoption'){await bounded(new Promise(resolve=>{const check=()=>{if(seen.pending.size===0)resolve();else setTimeout(check,20);};check();}),5000,label);await settle(page,seen);}
async function owner(page,seen,count){assert.ok(await page.evaluate(()=>document.querySelector('.native-home-shell')===window.commonProofOwner),'The genuine native owner stays mounted.');assert.equal(seen.requests.filter(r=>r.type==='document').length,count,'Native route changes do not request another document.');assert.equal(await page.title(),'Lific');}
async function hold(page,url,fetchFirst){
  let enter,release,finish;const entered=new Promise(r=>enter=r),blocked=new Promise(r=>release=r),done=new Promise(r=>finish=r);
  await page.route(url,async route=>{
    try{const request=route.request();assert.equal(request.method(),'POST');assert.equal(await request.headerValue('authorization'),null);
      const args=JSON.parse(request.postData());assert.equal(args[0],'/ACC/overview');assert.deepEqual(args[2],{t:'i64',bits:64,v:String(seed.account)});
      const response=fetchFirst?await route.fetch({maxRedirects:0}):undefined;if(response)assert.equal(response.status(),200);
      enter({response});await blocked;const actual=response||await route.fetch({maxRedirects:0});await route.fulfill({response:actual});finish({status:actual.status()});
    }catch(error){enter({error});finish({error});try{await route.abort('failed');}catch{}}
  },{times:1});return {entered,release,done};
}
async function jump(page,name){await page.locator('#native-home-palette-open').click();const palette=page.getByRole('dialog',{name:'Jump to project',exact:true});await palette.getByRole('searchbox').fill(name);await palette.getByRole('link').filter({hasText:name}).first().click();await palette.waitFor({state:'hidden'});}

test(`native common owner ${scenario}; auth ${seed.auth_required?'required':'optional'}`,async t=>{
 const browser=await launchBrowser();
 try{for(const prefix of ['', '/app','/ACC']){
   const cases=scenario==='phone'?['focus','forward','forward_hash','foreign_pop','held_back']:scenario==='held'?['newer','hash','account']:[scenario];
   for(const kind of cases)await t.test(`${prefix||'root'} ${kind}`,async()=>{
    const proxy=await mountedProxy(upstream,prefix),context=await browser.newContext({viewport:scenario==='phone'?{width:390,height:844}:{width:1280,height:1000},reducedMotion:'reduce',colorScheme:'light'});
    const gates=[];let diagnostic;
    try{
      const currentToken=scenario==='held'?(await control('renew')).token:token;await cookie(context,proxy.origin,currentToken);
      if(kind==='foreign_pop')await context.addInitScript(()=>{const original=history.go;history.go=function(delta){if(window.holdNativeHistoryTraversal){window.heldNativeHistoryDelta=Number(delta);return;}return original.call(this,delta);};});
      if(scenario==='scroll')await context.addInitScript(()=>{window.sidebarScrollCalls=[];const original=Element.prototype.scrollIntoView;Element.prototype.scrollIntoView=function(options){const id=this.getAttribute('data-sidebar-project');if(id)window.sidebarScrollCalls.push({id,options});return original.call(this,options);};});
      if(scenario==='notice')await context.addInitScript(()=>{const send=WebSocket.prototype.send;window.noticePaletteSockets=[];WebSocket.prototype.send=function(value){if(new URL(this.url).pathname.endsWith('/__native_home/palette')&&!window.noticePaletteSockets.includes(this))window.noticePaletteSockets.push(this);return send.call(this,value);};});
      const page=await context.newPage();page.setDefaultTimeout(5000);const seen=observations(page),responses=[],consoleErrors=[];
      page.on('response',response=>responses.push({url:response.url(),method:response.request().method(),status:response.status(),location:response.headers().location,from:response.request().redirectedFrom()?.url()}));
      page.on('console',message=>{if(message.type()==='error')consoleErrors.push({text:message.text(),location:message.location()});});
      diagnostic=()=>({responses,consoleErrors,pending:[...seen.pending].map(request=>({url:request.url(),method:request.method(),type:request.resourceType()}))});
      let target=kind==='forward_hash'?'/#main-content':'/';if(scenario==='notice')target=(await control('notice',{identifier:`NT${prefix===''?0:prefix==='/app'?1:2}`})).destination;
      const response=await page.goto(`${proxy.origin}${prefix}${target}`);assert.equal(response.status(),200);
      await page.locator('.native-home-shell').waitFor();if(scenario!=='notice')await page.locator('[data-native-home]').waitFor();
      await page.evaluate(()=>{window.commonProofOwner=document.querySelector('.native-home-shell');});const documents=seen.requests.filter(r=>r.type==='document').length;
      if(kind==='held_back'){
        const nav=await openProject(page,'One'),gate=await hold(page,`${proxy.origin}${prefix}/__native_workspace/destination`,true);gates.push(gate);
        await nav.getByRole('link',{name:'Overview',exact:true}).click();const entered=await bounded(gate.entered,2000,'Genuine phone destination reply');if(entered.error)throw entered.error;
        await page.goBack();await nav.waitFor();await attr(page.locator('[data-native-mobile-root]'),'hidden',null);assert.equal((await page.evaluate(()=>history.state.lificNativeHomeNav)).pane,'root');
        gate.release();const result=await bounded(gate.done,5000,'Phone reply after newer genuine Back');if(result.error)throw result.error;
        await adopted(page,seen);assert.equal(page.url(),`${proxy.origin}${prefix}/`,'A newer same-route user traversal cancels the older destination.');await nav.waitFor();await attr(page.locator('[data-native-mobile-root]'),'hidden',null);assert.equal(await page.locator('.native-overview').count(),0);await owner(page,seen,documents);
      }else if(kind==='focus'){
        const nav=await openProject(page,'One');await nav.getByRole('link',{name:'Overview',exact:true}).click();await overview(page,proxy.origin,prefix,'ACC');await nav.waitFor({state:'hidden'});
        await page.waitForFunction(()=>document.activeElement===document.getElementById('native-home-mobile-open'));
        assert.equal(await page.getByRole('button',{name:'Open navigation',exact:true}).evaluate(n=>n===document.activeElement),true,'Main close(callback) restores the real trigger during owned navigation.');
        await page.goBack();await page.waitForURL(`${proxy.origin}${prefix}/`);await page.locator('[data-native-home]').waitFor();assert.equal(await nav.isVisible(),false);
        await page.goForward();await overview(page,proxy.origin,prefix,'ACC');assert.equal(await nav.isVisible(),false);await owner(page,seen,documents);
      }else if(kind==='forward'||kind==='forward_hash'){
        const nav=await openProject(page,'One');await nav.getByRole('link',{name:'Overview',exact:true}).click();await overview(page,proxy.origin,prefix,'ACC');await nav.waitFor({state:'hidden'});
        await page.getByRole('button',{name:'Open navigation',exact:true}).click();await nav.waitFor();await attr(page.locator('[data-native-mobile-root]'),'hidden',null);
        const record=await page.evaluate(()=>history.state.lificNativeHomeNav);assert.equal(record.pane,'root');assert.equal(record.href,page.url());
        await page.evaluate(()=>history.go(-2));await page.waitForURL(`${proxy.origin}${prefix}/${kind==='forward_hash'?'#main-content':''}`);await page.locator('[data-native-home]').waitFor();await nav.waitFor({state:'hidden'});
        await page.evaluate(()=>history.go(2));await overview(page,proxy.origin,prefix,'ACC');await nav.waitFor();
        await attr(page.locator('[data-native-mobile-root]'),'hidden',null);assert.equal((await page.evaluate(()=>history.state.lificNativeHomeNav)).pane,'root','Main routeChanged preserves an owned Forward pane at its changed underlying route.');
        assert.equal(await page.locator('.native-home-body').evaluate(n=>n.inert),true);await attr(page.getByRole('button',{name:'Open One navigation',exact:true}),'data-current-project','true');await owner(page,seen,documents);
      }else if(kind==='foreign_pop'){
        const nav=await openProject(page,'One');await page.evaluate(()=>{window.holdNativeHistoryTraversal=true;});await nav.getByRole('link',{name:'Overview',exact:true}).click();
        await page.waitForFunction(()=>window.heldNativeHistoryDelta===-2);
        // The platform traversal is held, then a real browser Back reaches
        // the owned ROOT, not the queued CLOSED base. No popstate is fabricated.
        await page.goBack();await nav.waitFor();await attr(page.locator('[data-native-mobile-root]'),'hidden',null);
        await adopted(page,seen);assert.equal(page.url(),`${proxy.origin}${prefix}/`,'Main cancels the queued destination at a non-base real pop.');assert.equal(await page.locator('.native-overview').count(),0);await page.locator('[data-native-home]').waitFor();
        await page.evaluate(()=>{window.holdNativeHistoryTraversal=false;window.heldNativeHistoryDelta=undefined;});await owner(page,seen,documents);
      }else if(kind==='newer'||kind==='hash'||kind==='account'){
        const url=`${proxy.origin}${prefix}/__native_workspace/destination`,gate=await hold(page,url,kind!=='account');gates.push(gate);
        const aside=page.getByRole('complementary',{name:'Workspace sidebar',exact:true});await aside.locator('a[title="One"]').click();const entered=await bounded(gate.entered,2000,'Actual destination request');if(entered.error)throw entered.error;
        if(kind==='newer'){
          await aside.locator('a[title="Two"]').click();await overview(page,proxy.origin,prefix,'TWO');gate.release();const result=await bounded(gate.done,5000,'Genuine held destination delivery');if(result.error)throw result.error;
          await adopted(page,seen);assert.equal(page.url(),`${proxy.origin}${prefix}/TWO/overview`);await attr(aside.locator('a[title="Two"]'),'aria-current','page');await owner(page,seen,documents);
        }else if(kind==='hash'){
          const skip=page.locator('.tc-shell__skip');await skip.focus();await skip.press('Enter');await page.waitForURL(`${proxy.origin}${prefix}/#main-content`);
          gate.release();const result=await bounded(gate.done,5000,'Genuine held destination delivery after hash');if(result.error)throw result.error;
          await adopted(page,seen);assert.equal(page.url(),`${proxy.origin}${prefix}/#main-content`,'Main cancels a pending destination when newer genuine hash navigation wins.');assert.equal(await page.locator('.native-overview').count(),0);await page.locator('[data-native-home]').waitFor();await owner(page,seen,documents);
        }else{
          await cookie(context,proxy.origin,seed.other_token);assert.equal((await control('expire')).expired,true);gate.release();const result=await bounded(gate.done,5000,'Stale current-cookie dispatch');if(result.error)throw result.error;assert.ok(result.status>=300,'The real server rejects the held expired credential.');
          await page.getByRole('heading',{name:"Couldn't load this project",exact:true}).waitFor();await page.locator('.native-home-account').getByText('non_member',{exact:true}).waitFor();
          assert.equal(await page.getByText('Visible active initial work',{exact:true}).count(),0);assert.equal(await page.getByText('One',{exact:true}).count(),0);assert.ok(seen.requests.filter(r=>r.type==='document').length>documents,'A replacement account starts a genuine fresh document owner.');
        }
      }else if(kind==='scroll'){
        const aside=page.getByRole('complementary',{name:'Workspace sidebar',exact:true}),nav=aside.locator('nav'),target=aside.locator(`a[data-sidebar-project="${seed.far[44]}"]`);
        // Observe genuine large-catalog bootstrap completion, then retain the
        // original independent 150+150ms idle predicate unchanged.
        await adopted(page,seen,'Actual 45-row native bootstrap adoption');
        assert.equal(await aside.locator('[data-native-sidebar-project]').count(),45);const navBox=await nav.boundingBox();assert.ok((await target.boundingBox()).y>navBox.y+navBox.height);assert.equal(await nav.evaluate(n=>n.scrollTop),0);
        await jump(page,'Project 45');await overview(page,proxy.origin,prefix,'P45');
        await attr(target,'aria-current','page');await attr(aside.locator(`#project-nav-${seed.far[44]}`),'hidden',null);
        await adopted(page,seen,'Actual route and reveal adoption');const box=await target.boundingBox();
        assert.ok(box.y>=navBox.y&&box.y+box.height<=navBox.y+navBox.height+1,'Route entry scrolls the actual row into view.');assert.ok(Math.abs(box.y+box.height-navBox.y-navBox.height)<=2,'Nearest scroll aligns the row to the lower edge.');
        assert.deepEqual(await page.evaluate(()=>window.sidebarScrollCalls),[{id:String(seed.far[44]),options:{block:'nearest'}}]);
        await page.mouse.move(navBox.x+navBox.width/2,navBox.y+navBox.height/2);await page.mouse.wheel(0,-10000);await page.waitForFunction(()=>document.querySelector('.native-home-workspace').scrollTop===0);
        await jump(page,'Project 45');await adopted(page,seen);assert.equal(await nav.evaluate(n=>n.scrollTop),0,'Same-project revalidation respects real manual wheel scrolling.');assert.deepEqual(await page.evaluate(()=>window.sidebarScrollCalls),[{id:String(seed.far[44]),options:{block:'nearest'}}]);
        await jump(page,'Project 44');await overview(page,proxy.origin,prefix,'P44');await adopted(page,seen);assert.ok(await nav.evaluate(n=>n.scrollTop>0));assert.deepEqual(await page.evaluate(()=>window.sidebarScrollCalls),[{id:String(seed.far[44]),options:{block:'nearest'}},{id:String(seed.far[43]),options:{block:'nearest'}}]);await owner(page,seen,documents);
      }else if(kind==='notice'){
        const notice=page.locator('[data-native-project-notice]');await notice.waitFor();const message=await notice.textContent();assert.ok(message.startsWith("Project created, but it wasn't added to the group:"));
        const probe=await context.request.get(`${proxy.origin}${prefix}${target}`);assert.equal(probe.status(),200);const probeHtml=await probe.text(),probeNotice=await page.evaluate(html=>new DOMParser().parseFromString(html,'text/html').querySelector('[data-native-project-notice]')?.textContent,probeHtml);assert.notEqual(probeNotice,message,'The actual one-time notice has already been consumed; a fresh entry cannot consume it again.');
        await page.waitForFunction(()=>window.noticePaletteSockets.length===1);await page.evaluate(()=>new Promise(resolve=>{const socket=window.noticePaletteSockets[0];socket.addEventListener('close',resolve,{once:true});socket.close();}));
        await page.waitForFunction(()=>window.noticePaletteSockets.length===2);assert.equal(await notice.textContent(),message,'Entered presentation survives genuine sibling framework reconnection.');await owner(page,seen,documents);
      }
      if(kind!=='account')await adopted(page,seen);await nativeContract(page,seen);
    }catch(error){throw new Error(`${error.message}\nActual transport: ${JSON.stringify(diagnostic?.())}`,{cause:error});}finally{for(const gate of gates){gate.release();await bounded(gate.done,5000,'Held transport cleanup');}await context.close();await proxy.close();}
   });
 }}finally{await browser.close();input.close();}
});
