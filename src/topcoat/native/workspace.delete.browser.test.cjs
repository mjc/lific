// Real normal routes/buttons, current cookie authority and real fixture DB.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const readline = require('node:readline');
const {mountedProxy, launchBrowser, launchVisibilityBrowser} = require(path.join(process.cwd(),'src/topcoat/native/browser_fixture.cjs'));
const upstream = new URL(process.argv[2]), token = process.argv[3];
const input = readline.createInterface({input:process.stdin});
let awaiting;
input.on('line',line=>{assert.ok(awaiting);const resolve=awaiting;awaiting=undefined;resolve(JSON.parse(line));});
function control(action,values={}) {
  return new Promise(resolve=>{
    assert.equal(awaiting,undefined);awaiting=resolve;
    process.stdout.write(`WORKSPACE_CONTROL ${JSON.stringify({action,...values})}\n`);
  });
}
const snapshot=seed=>control('snapshot',{issue_id:seed.issue_id});
const deleted=seed=>control('wait_deleted',{issue_id:seed.issue_id});
function unchanged(current,seed) {
  assert.equal(current.deleted_at,null,'Pending/Undo never tombstones the live row.');
  assert.equal(current.seq,seed.seq,'No issue mutation occurs before commit.');
  assert.deepEqual(current.delete_audits,[]);assert.deepEqual(current.restore_audits,[]);
  assert.deepEqual(current.deleted_events,[]);
}
function committed(current,seed) {
  assert.ok(current.deleted_at);assert.ok(current.seq>seed.seq);
  assert.deepEqual(current.delete_audits,[{actor_id:seed.account_id,transport:'web'}]);
  assert.deepEqual(current.restore_audits,[]);
  assert.deepEqual(current.deleted_events,[{issue_id:seed.issue_id,project_id:seed.project_id,seq:current.seq}]);
}
const toast=(page,seed)=>page.getByRole('status').filter({has:page.getByText(`Deleted ${seed.identifier}`,{exact:true})});
const failureText=seed=>`Couldn't delete ${seed.identifier} — restored`;
async function owner(page) {
  assert.ok(await page.evaluate(()=>document.querySelector('.native-home-shell')===window.deleteParent&&window.deleteParent.testOwner===window.deleteParentToken));
}
async function detail(page,proxy,prefix,seed) {
  await page.waitForURL(`${proxy.origin}${prefix}/ACC/issues/${seed.identifier}`);
  try {await page.getByRole('button',{name:seed.title,exact:true}).waitFor();}
  catch(error) {
    const output=path.join(require('node:os').tmpdir(),'lific-native-deferred-delete');
    fs.mkdirSync(output,{recursive:true});
    fs.writeFileSync(path.join(output,`${process.argv[4]}-${prefix.slice(1)||'root'}-${seed.identifier}-detail.html`),await page.content());
    console.error(JSON.stringify({expected:seed.title,identifier:seed.identifier,url:page.url(),editor:await page.locator('[data-native-issue-editor]').innerText()}));
    throw error;
  }
  assert.equal(Number(await page.locator('[data-native-issue-seq]').textContent()),seed.seq);
}
async function pause(page) {
  // Installed Playwright browser clock controls only the costly timer boundary.
  // Pause before scheduling so 4999+1ms expresses the exact original5s delay.
  await page.clock.pauseAt(await page.evaluate(()=>Date.now()+1000));
}
async function schedule(page,proxy,prefix,seed) {
  const more=page.getByTitle('More actions',{exact:true});
  assert.equal(await more.count(),1,'Real Maintainer More must exist.');
  await more.click();await page.getByRole('button',{name:'Delete issue',exact:true}).click();
  await page.getByText(`Delete ${seed.identifier}?`,{exact:true}).waitFor();
  assert.equal(await page.getByText("This can't be undone.",{exact:true}).count(),1);
  await page.getByRole('button',{name:'Delete',exact:true}).click();
  await page.waitForURL(`${proxy.origin}${prefix}/ACC/issues`);
  await page.locator('[data-native-issue-list]').waitFor();
  assert.equal(await page.locator(`[data-native-issue-list] a[href="${prefix}/ACC/issues/${seed.identifier}"]`).count(),0);
  await toast(page,seed).waitFor();
  assert.equal(await toast(page,seed).getByRole('button',{name:'Undo',exact:true}).count(),1);
  await owner(page);
  assert.ok(await page.evaluate(()=>window.deleteSource && !window.deleteSource.isConnected),'Scheduling retires the actual source issue scope.');
  await page.mouse.move(0,0);
}
async function cancelAndOutside(page,seed) {
  const more=page.getByTitle('More actions',{exact:true});
  assert.equal(await more.count(),1);
  await more.click();await page.getByRole('button',{name:'Delete issue',exact:true}).click();
  await page.getByRole('button',{name:'Cancel',exact:true}).click();
  assert.equal(await page.getByRole('button',{name:'Delete issue',exact:true}).count(),0);
  assert.equal(await page.getByText(`Delete ${seed.identifier}?`,{exact:true}).isVisible(),false);
  await more.click();
  await page.getByRole('heading',{name:'Workflow description',exact:true}).click();
  assert.equal(await page.getByRole('button',{name:'Delete issue',exact:true}).count(),0);
  await more.click();await page.getByRole('button',{name:'Delete issue',exact:true}).click();
  await page.getByRole('heading',{name:'Workflow description',exact:true}).click();
  assert.equal(await page.getByText(`Delete ${seed.identifier}?`,{exact:true}).isVisible(),false);
}
async function productionRefresh(page) {
  return page.evaluate(async()=>{
    const previous=document.querySelector('.native-home-shell'),detail={};
    window.dispatchEvent(new CustomEvent('topcoat:dev-runtime:v1',{detail}));
    if(!detail.runtime)throw new Error('The installed PageUnit refresh listener is missing.');
    const response=await detail.runtime.request(new AbortController().signal);
    if(!response.ok)throw new Error(`Actual production refresh failed: ${response.status}`);
    const parsed=new DOMParser().parseFromString(await response.text(),'text/html');
    if(!parsed.querySelector('[data-native-issue-list]'))throw new Error('Refresh must return actual production list HTML.');
    detail.runtime.replace(()=>document.body.replaceChildren(...Array.from(parsed.body.childNodes,node=>document.importNode(node,true))));
    return {status:response.status,contentType:response.headers.get('content-type'),oldDetached:!previous.isConnected};
  });
}

const visibleToasts=page=>page.locator('#native-deferred-delete-owner .native-toast:visible');
const listRow=(page,prefix,seed)=>page.locator(`[data-native-issue-list] a[href="${prefix}/ACC/issues/${seed.identifier}"]`);
async function enterListed(page,proxy,prefix,seed) {
  await listRow(page,prefix,seed).click();await detail(page,proxy,prefix,seed);
  await page.evaluate(()=>window.deleteSource=document.querySelector('[data-native-issue-editor]'));
}
async function omitted(page,prefix,seeds) {
  await page.waitForFunction(({prefix,ids})=>ids.every(id=>!document.querySelector(`[data-native-issue-list] a[href="${prefix}/ACC/issues/${id}"]`)),{prefix,ids:seeds.map(seed=>seed.identifier)});
  for(const seed of seeds)assert.equal(await listRow(page,prefix,seed).count(),0);
}
async function toastOrder(page,expected) {
  const observed=await visibleToasts(page).evaluateAll(nodes=>nodes.map(node=>({
    id:Number(node.dataset.nativeToastId),message:node.querySelector('p').textContent,
    order:Number(getComputedStyle(node).order),y:node.getBoundingClientRect().y,
  })).sort((a,b)=>a.id-b.id));
  assert.deepEqual(observed.map(item=>item.message),expected);
  assert.ok(observed.every(item=>item.id>0&&item.order===item.id));
  assert.equal(new Set(observed.map(item=>item.id)).size,observed.length);
  assert.ok(observed.every((item,index)=>index===0||item.y>observed[index-1].y),'Actual rendered stack follows the monotonic original toast order.');
  return observed;
}
// Hold transport at a genuine route interception boundary. A denial is fetched
// immediately from the production service; a success can be held before forwarding
// so its independent live DB row proves omission while the real request is in flight.
async function holdCommits(page,url,{fetchFirst=false,status=200}={}) {
  const records=[],waiting=[];
  await page.route(url,async route=>{
    const actual=fetchFirst?await route.fetch():undefined;
    if(actual)assert.equal(actual.status(),status);
    let release,finish;
    const blocked=new Promise(resolve=>release=resolve),done=new Promise(resolve=>finish=resolve);
    const record={release,done,status:actual?.status()};records.push(record);
    waiting.shift()?.(record);
    await blocked;
    try {
      const response=actual||await route.fetch();assert.equal(response.status(),status);
      await route.fulfill({response});finish({status:response.status()});
    }catch(error){finish({error:error.message});}
  });
  return {
    records,
    next:()=>{
      const record=records.find(record=>!record.observed);
      if(record){record.observed=true;return Promise.resolve(record);}
      return new Promise((resolve,reject)=>{
        const deadline=setTimeout(()=>reject(new Error('The genuine native deletion request did not reach its interception boundary within15s.')),15000);
        waiting.push(record=>{clearTimeout(deadline);record.observed=true;resolve(record);});
      });
    },
    releaseAll:()=>records.forEach(record=>record.release()),
  };
}
async function delivered(context,url,record) {
  const terminal=context.waitForEvent('response',{predicate:response=>response.url()===url&&response.request().method()==='POST',timeout:15000});
  record.release();const response=await terminal;assert.equal(response.status(),(await record.done).status);
  return response;
}

test('native deferred Delete/Undo uses real workspace lifetime and fresh authority',async t=>{
  const visibility=process.argv[4] === 'visibility';
  const browser=visibility ? null : await launchBrowser();
  try {
    const supported=['undo','close','timeout','pause_timeout','visibility','pagehide','fresh_viewer','viewer_commit','replacement_commit','stale_owner','stack_eviction','focused_eviction','overlapping','same_issue'];
    assert.ok(supported.includes(process.argv[4]),'The Rust fixture supplies one explicit workflow scenario.');
    const scenarios=[process.argv[4]];
    for(const prefix of ['', '/app', '/ACC'])for(const scenario of scenarios)await t.test(`${prefix||'root'} ${scenario}`,async()=>{
      const seed=await control('prepare',{label:`${prefix||'root'} ${scenario}`});
      const proxy=await mountedProxy(upstream,prefix);
      const requests=[],errors=[],consoleErrors=[];
      let visibilityOwner,context,releaseHeld,heldCommits;
      try {
      visibilityOwner=visibility ? await launchVisibilityBrowser() : null;
      context=visibilityOwner ? visibilityOwner.context : await browser.newContext({viewport:{width:1440,height:900},reducedMotion:'reduce'});
      context.on('request',request=>requests.push(request));
      await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
      // Test observation only: preserve actual generic Fetch options across the
      // genuine pagehide into the next same-origin document, without changing
      // arguments, response, timing or business state.
      await context.addInitScript(()=>{
        const fetch=window.fetch;
        window.fetch=function(resource,options){
          const url=new URL(typeof resource==='string'||resource instanceof URL?resource:resource.url,location.href);
          if(url.pathname.endsWith('/__native_issue_edit/delete')){
            const records=JSON.parse(sessionStorage.getItem('lific_test_delete_fetches')||'[]');
            records.push({url:url.href,keepalive:options?.keepalive===true});
            sessionStorage.setItem('lific_test_delete_fetches',JSON.stringify(records));
          }
          return fetch.apply(this,arguments);
        };
      });
      const page=await context.newPage();
      if(visibilityOwner){await page.setViewportSize({width:1440,height:900});await page.emulateMedia({reducedMotion:'reduce'});}
      page.setDefaultTimeout(15000);
      page.on('pageerror',error=>errors.push(error.message));
      page.on('console',message=>{if(message.type()==='error')consoleErrors.push(message.text());});
      await page.clock.install();
      const commitUrl=`${proxy.origin}${prefix}/__native_issue_edit/delete`;
      const commitRequests=()=>requests.filter(request=>request.url()===commitUrl);
      const response=()=>context.waitForEvent('response',{predicate:response=>response.url()===commitUrl&&response.request().method()==='POST',timeout:15000});
        if(scenario==='fresh_viewer')await control('membership',{role:'viewer'});
        const initial=await page.goto(`${proxy.origin}${prefix}/ACC/issues/${seed.identifier}`);
        assert.equal(initial.status(),200);
        assert.ok((await initial.text()).includes(`data-native-issue-editor="${seed.identifier}"`));
        const source=await page.locator('#native-deferred-delete-owner').getAttribute('data-topcoat-on:mount');
        const artifacts=path.join(require('node:os').tmpdir(),'lific-native-deferred-delete');
        fs.mkdirSync(artifacts,{recursive:true});
        fs.writeFileSync(path.join(artifacts,`${scenario}-${prefix.slice(1)||'root'}-owner.js`),source);
        const parsed=await page.evaluate(source=>{
          try{new Function('cx',`return ${source};`);return {ok:true};}
          catch(error){return {ok:false,name:error.name,message:error.message};}
        },source);
        assert.equal(parsed.ok,true,`The genuine emitted owner factory must parse: ${JSON.stringify(parsed)}`);
        await page.getByRole('heading',{name:'Workflow description',exact:true}).waitFor();
        await page.evaluate(()=>{
          window.deleteParent=document.querySelector('.native-home-shell');
          window.deleteParentToken=Symbol('original workspace');window.deleteParent.testOwner=window.deleteParentToken;
          window.deleteSource=document.querySelector('[data-native-issue-editor]');
        });
        const documents=()=>requests.filter(request=>request.resourceType()==='document'&&request.frame()===page.mainFrame()).length;
        const initialDocuments=documents();
        if(scenario==='fresh_viewer'){
          await page.getByRole('heading',{name:seed.title,exact:true}).waitFor();
          assert.equal(await page.getByTitle('More actions',{exact:true}).count(),0);
          assert.equal(await page.getByRole('button',{name:'Delete issue',exact:true}).count(),0);
          assert.equal(await page.getByRole('button',{name:'Export',exact:true}).count(),1);
          unchanged(await snapshot(seed),seed);assert.equal(commitRequests().length,0);
        }else{
          await detail(page,proxy,prefix,seed);
          if(['stack_eviction','focused_eviction','overlapping','same_issue'].includes(scenario)){
            await pause(page);
            if(scenario==='focused_eviction'){
              const seeds=[seed];
              for(let index=1;index<4;index++)seeds.push(await control('prepare',{label:`focused ${prefix||'root'} ${index}`}));
              heldCommits=await holdCommits(page,commitUrl,{fetchFirst:true,status:403});
              const held=[];await schedule(page,proxy,prefix,seeds[0]);
              for(let index=1;index<seeds.length;index++){
                await enterListed(page,proxy,prefix,seeds[index]);
                await control('membership',{role:'viewer'});
                await schedule(page,proxy,prefix,seeds[index]);held.push(await heldCommits.next());
                await control('membership',{role:'maintainer'});
              }
              await omitted(page,prefix,seeds);
              assert.equal(await visibleToasts(page).count(),4);
              assert.equal(commitRequests().length,3);
              for(const current of seeds)unchanged(await snapshot(current),current);
              const oldest=toast(page,seeds[0]);
              await oldest.getByRole('button',{name:'Dismiss notification',exact:true}).focus();
              await oldest.evaluate(node=>{window.evictedToast=node;window.evictedToastButton=document.activeElement;});
              assert.ok(await oldest.evaluate(node=>node.contains(document.activeElement)));
              assert.equal((await delivered(context,commitUrl,held[0])).status(),403);
              const failure=page.getByRole('alert').filter({has:page.getByText(failureText(seeds[0]),{exact:true})});
              await failure.waitFor();await detail(page,proxy,prefix,seeds[0]);await owner(page);
              assert.equal(await page.evaluate(()=>document.activeElement===window.evictedToastButton),false,'Eviction loses the old focused control as in the original toast-ID keyed presentation.');
              assert.equal(await failure.evaluate(node=>node.contains(document.activeElement)),false,'The new notice cannot inherit focus without its own focus event.');
              await toast(page,seeds[3]).waitFor();assert.equal(commitRequests().length,3);
              for(let index=1;index<held.length;index++){
                assert.equal((await delivered(context,commitUrl,held[index])).status(),403);
                await page.getByText(failureText(seeds[index]),{exact:true}).waitFor();
                await detail(page,proxy,prefix,seeds[index]);await owner(page);
              }
              await page.unroute(commitUrl);
              await toast(page,seeds[3]).getByRole('button',{name:'Undo',exact:true}).click();
              await detail(page,proxy,prefix,seeds[3]);await owner(page);
              for(const current of seeds)unchanged(await snapshot(current),current);
              assert.equal(commitRequests().length,3,'Eviction and newest Undo cannot create duplicate or new commits.');
              await page.mouse.move(0,0);
              await failure.getByRole('button',{name:'Dismiss notification',exact:true}).focus();
              assert.equal(await failure.evaluate(node=>node.matches(':hover')),false,'Only focus pauses this timer; the pointer stays outside the stack.');
              await page.clock.fastForward(8000);
              await failure.waitFor();
              await page.keyboard.press('Tab');
              assert.equal(await failure.evaluate(node=>node.contains(document.activeElement)),false);
              await page.clock.fastForward(7999);await failure.waitFor();
              await page.clock.fastForward(1);await failure.waitFor({state:'hidden'});
              for(const current of seeds)unchanged(await snapshot(current),current);
              assert.equal(commitRequests().length,3);
            }else if(scenario==='stack_eviction'){
              const seeds=[seed];
              for(let index=1;index<5;index++)seeds.push(await control('prepare',{label:`stack ${prefix||'root'} ${index}`}));
              heldCommits=await holdCommits(page,commitUrl);
              const held=[];
              await schedule(page,proxy,prefix,seeds[0]);
              for(let index=1;index<seeds.length;index++){
                await enterListed(page,proxy,prefix,seeds[index]);
                await schedule(page,proxy,prefix,seeds[index]);held.push(await heldCommits.next());
                await omitted(page,prefix,seeds.slice(0,index+1));
                for(const current of seeds.slice(0,index+1))unchanged(await snapshot(current),current);
                assert.equal(await visibleToasts(page).count(),Math.min(index+1,4));
              }
              const beforeUndo=await toastOrder(page,seeds.slice(1).map(current=>`Deleted ${current.identifier}`));
              assert.equal(await page.getByText(`Deleted ${seeds[0].identifier}`,{exact:true}).isVisible(),false,'The actual fifth toast evicts the oldest presentation slot.');
              assert.equal(commitRequests().length,4,'Evicting an already claimed oldest toast cannot send a second commit.');
              assert.equal(await visibleToasts(page).getByRole('button',{name:'Undo',exact:true}).count(),4,'Original committed batches retain their Undo presentation until dismissal.');
              await toast(page,seeds[1]).getByRole('button',{name:'Undo',exact:true}).click();
              await toast(page,seeds[1]).waitFor({state:'hidden'});
              await toast(page,seeds[4]).waitFor();
              await omitted(page,prefix,seeds);
              assert.equal(commitRequests().length,4,'Old committed Undo dismisses only its toast and cannot recommit or cancel the newest batch.');
              unchanged(await snapshot(seeds[4]),seeds[4]);
              for(let index=0;index<held.length;index++){
                assert.equal((await delivered(context,commitUrl,held[index])).status(),200);
                committed(await deleted(seeds[index]),seeds[index]);
              }
              await page.unroute(commitUrl);unchanged(await snapshot(seeds[4]),seeds[4]);
              await toast(page,seeds[4]).getByRole('button',{name:'Undo',exact:true}).click();
              await detail(page,proxy,prefix,seeds[4]);await owner(page);
              unchanged(await snapshot(seeds[4]),seeds[4]);
              await page.getByText(`Restored ${seeds[4].identifier}`,{exact:true}).waitFor();
              const afterUndo=await toastOrder(page,[...seeds.slice(2,4).map(current=>`Deleted ${current.identifier}`),`Restored ${seeds[4].identifier}`]);
              assert.ok(afterUndo.at(-1).id>Math.max(...beforeUndo.map(item=>item.id)),'Undo pushes a genuine new restored notification as in the pinned store.');
              assert.equal(commitRequests().length,4);
              await page.clock.fastForward(6000);
              for(let index=0;index<4;index++)committed(await snapshot(seeds[index]),seeds[index]);
              unchanged(await snapshot(seeds[4]),seeds[4]);assert.equal(commitRequests().length,4);
            }else if(scenario==='overlapping'){
              const seeds=[seed,await control('prepare',{label:`overlap ${prefix||'root'} second`}),await control('prepare',{label:`overlap ${prefix||'root'} third`})];
              heldCommits=await holdCommits(page,commitUrl,{fetchFirst:true,status:403});
              const held=[];await schedule(page,proxy,prefix,seeds[0]);
              for(let index=1;index<seeds.length;index++){
                await enterListed(page,proxy,prefix,seeds[index]);
                // The existing real Maintainer editor can schedule; its replaced
                // batch must still use fresh DB Viewer authority at commit time.
                await control('membership',{role:'viewer'});
                await schedule(page,proxy,prefix,seeds[index]);held.push(await heldCommits.next());
                await control('membership',{role:'maintainer'});
              }
              await omitted(page,prefix,seeds);
              for(const current of seeds)unchanged(await snapshot(current),current);
              assert.equal(commitRequests().length,2);
              for(let index=0;index<held.length;index++){
                assert.equal((await delivered(context,commitUrl,held[index])).status(),403);
                await page.getByText(failureText(seeds[index]),{exact:true}).waitFor();
                // Pinned IssueDetail::handleDelete captures detailHref and its
                // onRestore unconditionally navigates there on commit failure.
                await detail(page,proxy,prefix,seeds[index]);await owner(page);
                await toast(page,seeds[2]).waitFor();
                await page.getByRole('navigation',{name:'Breadcrumb',exact:true}).getByRole('link',{name:'Issues',exact:true}).click();
                await page.waitForURL(`${proxy.origin}${prefix}/ACC/issues`);
                await page.locator('[data-native-issue-list]').waitFor();
                await listRow(page,prefix,seeds[index]).waitFor();
                await omitted(page,prefix,seeds.slice(index+1));
                await toast(page,seeds[2]).waitFor();
                for(const current of seeds)unchanged(await snapshot(current),current);
              }
              await page.unroute(commitUrl);
              await toast(page,seeds[2]).getByRole('button',{name:'Undo',exact:true}).click();
              await detail(page,proxy,prefix,seeds[2]);await owner(page);
              assert.equal(commitRequests().length,2);
              for(const current of seeds)unchanged(await snapshot(current),current);
            }else{
              heldCommits=await holdCommits(page,commitUrl,{fetchFirst:true,status:403});
              await schedule(page,proxy,prefix,seed);await control('membership',{role:'viewer'});
              await toast(page,seed).getByRole('button',{name:'Dismiss notification',exact:true}).click();
              const held=await heldCommits.next();unchanged(await snapshot(seed),seed);
              await control('membership',{role:'maintainer'});
              // Revisit the still-live omitted issue through genuine history; no
              // fabricated link, manual view or direct scheduling intent.
              await page.goBack();await detail(page,proxy,prefix,seed);await owner(page);
              await page.evaluate(()=>window.deleteSource=document.querySelector('[data-native-issue-editor]'));
              await schedule(page,proxy,prefix,seed);await omitted(page,prefix,[seed]);
              assert.equal((await delivered(context,commitUrl,held)).status(),403);
              await page.getByText(failureText(seed),{exact:true}).waitFor();
              await detail(page,proxy,prefix,seed);await owner(page);
              await toast(page,seed).waitFor();
              await page.getByRole('navigation',{name:'Breadcrumb',exact:true}).getByRole('link',{name:'Issues',exact:true}).click();
              await page.waitForURL(`${proxy.origin}${prefix}/ACC/issues`);
              await page.locator('[data-native-issue-list]').waitFor();
              await omitted(page,prefix,[seed]);await toast(page,seed).waitFor();
              assert.equal(await toast(page,seed).getByRole('button',{name:'Undo',exact:true}).count(),1,'Older same-ID completion preserves the newer pending owner.');
              unchanged(await snapshot(seed),seed);await page.unroute(commitUrl);
              await toast(page,seed).getByRole('button',{name:'Undo',exact:true}).click();
              await detail(page,proxy,prefix,seed);await owner(page);unchanged(await snapshot(seed),seed);
              await page.clock.fastForward(6000);unchanged(await snapshot(seed),seed);
              assert.equal(commitRequests().length,1,'New pending Undo never commits, despite the earlier same-ID denial.');
            }
            assert.equal(documents(),initialDocuments,'Stack/replacement/history retains the original parent document.');
          }else{
          if(scenario==='undo'){
            await cancelAndOutside(page,seed);unchanged(await snapshot(seed),seed);
            assert.equal(commitRequests().length,0,'Cancel/outside never call deletion.');
          }
          await pause(page);await schedule(page,proxy,prefix,seed);
          unchanged(await snapshot(seed),seed);
          assert.equal(commitRequests().length,0,'Confirmation schedules Undo rather than immediately deleting.');
          const live=await context.request.get(`${proxy.origin}${prefix}/ACC/issues/${seed.identifier}`);
          assert.equal(live.status(),200,'Pending row remains genuinely readable in the DB.');
          assert.ok((await live.text()).includes(seed.title));
          if(scenario==='undo'||scenario==='visibility'){
            if(scenario==='visibility'){
              const other=await context.newPage();
              await other.goto(`${proxy.origin}${prefix}/`);await other.bringToFront();
              // Real browser tab activation must be observable as hidden. No
              // synthetic visibility event or patched visibilityState fallback.
              await page.waitForFunction(()=>document.visibilityState==='hidden');
              unchanged(await snapshot(seed),seed);assert.equal(commitRequests().length,0);
              await page.bringToFront();await page.waitForFunction(()=>document.visibilityState==='visible');
              await other.close();await toast(page,seed).waitFor();
            }
            const fresh=await control('rename',{issue_id:seed.issue_id,title:`Fresh restored ${seed.identifier}`});
            await toast(page,seed).getByRole('button',{name:'Undo',exact:true}).click();
            await detail(page,proxy,prefix,fresh);await owner(page);
            const restored=await snapshot(seed);assert.equal(restored.title,fresh.title);assert.equal(restored.seq,fresh.seq);
            assert.equal(restored.deleted_at,null);assert.deepEqual(restored.delete_audits,[]);
            assert.deepEqual(restored.restore_audits,[]);assert.deepEqual(restored.deleted_events,[]);
            assert.equal(commitRequests().length,0,'Undo performs no delete/restore/cancel request.');
            assert.ok(!requests.some(request=>/\/(?:restore|cancel|undo)(?:[_/-]|$)/.test(new URL(request.url()).pathname)));
            await page.clock.fastForward(6000);assert.equal(commitRequests().length,0,'Canceled timer cannot later commit.');
            assert.equal(documents(),initialDocuments,'Undo keeps the genuine parent document.');
          }else if(scenario==='pagehide'){
            await page.goto(`${proxy.origin}${prefix}/`,{waitUntil:'domcontentloaded'});
            await page.locator('[data-native-home]').waitFor();
            committed(await deleted(seed),seed);
            const observed=await page.evaluate(()=>JSON.parse(sessionStorage.getItem('lific_test_delete_fetches')||'[]'));
            assert.deepEqual(observed,[{url:commitUrl,keepalive:true}],'Actual pagehide uses the same native cookie route with genuine Fetch keepalive.');
            assert.equal(commitRequests().length,1);
            const received=proxy.requests.filter(request=>request.method==='POST'&&request.path===`${prefix}/__native_issue_edit/delete`);
            assert.equal(received.length,1,'The actual proxy receives one final deletion request.');
            assert.ok(received[0].headers.cookie.includes(`lific_token=${token}`));
            assert.equal(received[0].headers.authorization,undefined);
            await page.reload();await page.locator('[data-native-home]').waitFor();
            committed(await snapshot(seed),seed);assert.equal(commitRequests().length,1,'Further pagehide cannot recommit the old pending slot.');
          }else if(scenario==='stale_owner'){
            await control('membership',{role:'viewer'});
            let held,routeDone,requestDone;
            const entered=new Promise(resolve=>held=resolve),completedRoute=new Promise(resolve=>routeDone=resolve);
            const completedRequest=new Promise(resolve=>requestDone=resolve);
            context.on('requestfinished',request=>{if(request.url()===commitUrl)requestDone({type:'finished'});});
            context.on('requestfailed',request=>{if(request.url()===commitUrl)requestDone({type:'failed',reason:request.failure()?.errorText});});
            const blocked=new Promise(resolve=>releaseHeld=resolve);
            await page.route(commitUrl,async route=>{
              const actual=await route.fetch();assert.equal(actual.status(),403,'The held failure is a genuine fresh Viewer denial.');
              held();await blocked;
              try{await route.fulfill({response:actual});routeDone(null);}catch(failure){routeDone(failure.message);}
            });
            await toast(page,seed).getByRole('button',{name:'Dismiss notification',exact:true}).click();await entered;
            unchanged(await snapshot(seed),seed);
            const next=await control('prepare',{label:`fresh owner ${prefix||'root'}`});
            const refresh=await productionRefresh(page);
            assert.equal(refresh.status,200);assert.match(refresh.contentType,/^text\/html/);assert.equal(refresh.oldDetached,true);
            await page.locator('[data-native-issue-list]').waitFor();
            const nextRow=page.locator(`[data-native-issue-list] a[href="${prefix}/ACC/issues/${next.identifier}"]`);
            await nextRow.click();await detail(page,proxy,prefix,next);
            await page.evaluate(()=>{
              window.deleteParent=document.querySelector('.native-home-shell');window.deleteParentToken=Symbol('fresh owner');
              window.deleteParent.testOwner=window.deleteParentToken;window.deleteSource=document.querySelector('[data-native-issue-editor]');
            });
            await schedule(page,proxy,prefix,next);
            releaseHeld();const terminal=await completedRequest,deliveryFailure=await completedRoute;
            if(deliveryFailure){assert.equal(terminal.type,'failed');assert.match(terminal.reason,/abort|cancel/i);}
            await page.unroute(commitUrl);
            await toast(page,next).waitFor();
            assert.equal(page.url(),`${proxy.origin}${prefix}/ACC/issues`);
            assert.equal(await page.getByText(failureText(seed),{exact:true}).count(),0,'Retired owner cannot replace new toast or navigate to its old detail.');
            unchanged(await snapshot(seed),seed);unchanged(await snapshot(next),next);
            await toast(page,next).getByRole('button',{name:'Undo',exact:true}).click();
            await detail(page,proxy,prefix,next);await owner(page);unchanged(await snapshot(next),next);
            assert.equal(commitRequests().length,1,'Only the old genuine denied commit was sent; fresh Undo does not commit.');
          }else{
            if(scenario==='viewer_commit')await control('membership',{role:'viewer'});
            if(scenario==='replacement_commit'){
              const replacement=await control('replacement_session');
              assert.notEqual(replacement.account_id,seed.account_id);
              await context.addCookies([{name:'lific_token',value:replacement.token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
            }
            const pendingResponse=response();
            if(scenario==='timeout'||scenario==='pause_timeout'){
              if(scenario==='pause_timeout'){
                await toast(page,seed).hover();await page.clock.fastForward(6000);
                unchanged(await snapshot(seed),seed);assert.equal(commitRequests().length,0,'Hover pauses the real timer.');
                await toast(page,seed).getByRole('button',{name:'Undo',exact:true}).focus();await page.mouse.move(0,0);
                await page.clock.fastForward(6000);unchanged(await snapshot(seed),seed);assert.equal(commitRequests().length,0,'Focus also pauses the real timer.');
                await page.keyboard.press('Tab');await page.keyboard.press('Tab');
                assert.equal(await toast(page,seed).evaluate(element=>element.contains(document.activeElement)),false);
              }
              await page.clock.fastForward(4999);unchanged(await snapshot(seed),seed);assert.equal(commitRequests().length,0);
              await page.clock.fastForward(1);
            }else await toast(page,seed).getByRole('button',{name:'Dismiss notification',exact:true}).click();
            const result=await pendingResponse;
            if(scenario==='viewer_commit'||scenario==='replacement_commit'){
              assert.equal(result.status(),403);unchanged(await snapshot(seed),seed);
              if(scenario==='viewer_commit'){
                await page.getByText(failureText(seed),{exact:true}).waitFor();
                await page.getByRole('heading',{name:seed.title,exact:true}).waitFor();
                assert.equal(await page.getByTitle('More actions',{exact:true}).count(),0);
              }
            }else{
              assert.equal(result.status(),200);committed(await deleted(seed),seed);
              await page.clock.fastForward(6000);committed(await snapshot(seed),seed);
              const missing=await context.request.get(`${proxy.origin}${prefix}/ACC/issues/${seed.identifier}`);
              assert.equal(missing.status(),404);
            }
            assert.equal(commitRequests().length,1,'Timer and explicit close commit at most once.');
          }
        }
        }
        for(const request of requests){assert.ok(!new URL(request.url()).pathname.split('/').includes('api'));assert.equal(request.headers().authorization,undefined);}
        assert.equal(await page.evaluate(()=>localStorage.getItem('lific_token')),null);
        assert.deepEqual(errors,[]);
      }finally{
        if(errors.length||consoleErrors.length)console.error(JSON.stringify({scenario,prefix,errors,consoleErrors}));
        try {
          releaseHeld?.();heldCommits?.releaseAll();
          if(visibilityOwner){
            try {for(const page of context.pages())await page.close();await context.clearCookies();}
            finally {await visibilityOwner.close();}
          }else if(context)await context.close();
        }finally{await proxy.close();}
      }
    });
  }finally{try{if(browser)await browser.close();}finally{input.close();process.stdin.destroy();}}
});
