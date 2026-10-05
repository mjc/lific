// Preserve pinned 9683d38 e2e/sidebar.ts:696 using real writes and failures.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const readline=require('node:readline');
const {mountedProxy,launchBrowser,cookie,observations,nativeContract,surface,action,appliedRequests,frozenWrite,holdApply,settle,called,bounded}=require('./sidebar.browser.fixture.cjs');
const upstream=new URL(process.argv[2]),token=process.argv[3],fixture=JSON.parse(process.argv[4]);
const input=readline.createInterface({input:process.stdin});let sequence=0;const pending=new Map();
input.on('line',line=>{const response=JSON.parse(line),resolve=pending.get(response.id);assert.ok(resolve);pending.delete(response.id);delete response.id;resolve(response);});
function control(action,fields={}){const id=++sequence;return new Promise(resolve=>{pending.set(id,resolve);process.stdout.write(`ORDER_CONTROL ${JSON.stringify({action,id,...fields})}\n`);});}
async function projected(tree,kind,phone,work){
  if(kind==='group')return tree.locator('button[data-sidebar-group-actions]').evaluateAll(nodes=>nodes.map(node=>node.getAttribute('aria-label')));
  const panel=tree.locator(`#group-${work}-${phone?'phone':'desktop'}`);
  return phone?panel.locator('button[data-native-project-trigger]').evaluateAll(nodes=>nodes.map(node=>node.getAttribute('aria-label').replace(/^Open /,'').replace(/ navigation$/,''))):panel.locator('a[title]').evaluateAll(nodes=>nodes.map(node=>node.title));
}
async function orderIs(tree,kind,phone,work,names){
  await tree.page().waitForFunction(({kind,phone,work,names})=>{
    const tree=document.querySelector(phone?'[data-native-mobile-root]':'aside[aria-label="Workspace sidebar"]');if(!tree)return false;
    const nodes=kind==='group'?tree.querySelectorAll('button[data-sidebar-group-actions]'):tree.querySelectorAll(`#group-${work}-${phone?'phone':'desktop'} ${phone?'button[data-native-project-trigger]':'a[title]'}`);
    const actual=[...nodes].map(node=>kind==='group'?node.getAttribute('aria-label'):phone?node.getAttribute('aria-label').replace(/^Open /,'').replace(/ navigation$/,''):node.title);
    return JSON.stringify(actual)===JSON.stringify(names);
  },{kind,phone,work,names});
}
function unchangedOther(before,after){assert.deepEqual(after.other_order,before.other_order);assert.deepEqual(after.other_groups,before.other_groups);assert.deepEqual(after.projects,before.projects,'Personal order does not mutate shared project records.');}
function unchangedMembership(before,after){assert.deepEqual(after.actor_groups.map(group=>[group.id,group.projects]),before.actor_groups.map(group=>[group.id,group.projects]));}
function eventIndex(kind){return kind==='group'?0:1;}
function canonical(db,kind){return kind==='group'?db.actor_groups.map(group=>group.id):db.actor_order;}
async function completed(page,url,operation,admission){
  // Observe rejection immediately; an action failure must not abandon the waiter.
  const finish=page.waitForResponse(response=>response.url()===url&&response.request().method()==='POST').then(response=>({response}),error=>({error}));
  try{
    await operation();if(admission)await admission();
    const outcome=await finish;if(outcome.error)throw outcome.error;
    assert.equal(outcome.response.status(),200);await outcome.response.finished();
  }finally{await finish;}
}
test(`personal group and grouped project order payloads rollback; auth ${fixture.auth_required?'required':'optional'}`,async t=>{
  const browser=await launchBrowser();
  try{for(const prefix of ['', '/app','/ACC'])for(const phone of [false,true])await t.test(`${prefix||'root'} ${phone?'phone':'desktop'}`,async caseT=>{
    await control('reset');const proxy=await mountedProxy(upstream,prefix),context=await browser.newContext({viewport:phone?{width:360,height:740}:{width:1000,height:760},isMobile:phone,hasTouch:phone,reducedMotion:'reduce'});
    const gates=[];
    try{
      await cookie(context,proxy.origin,token);const page=await context.newPage();page.setDefaultTimeout(5000);const seen=observations(page);
      assert.equal((await page.goto(`${proxy.origin}${prefix}/`)).status(),200);await page.locator('[data-native-home]').waitFor();
      const tree=await surface(page,phone),work=fixture.groups[0][0],groupTrigger=`Actions for ${phone?'':'group '}Work`;
      const originalGroups=[`Actions for ${phone?'':'group '}Work`,`Actions for ${phone?'':'group '}Personal`];
      assert.deepEqual(await projected(tree,'group',phone,work),originalGroups);assert.deepEqual(await projected(tree,'project',phone,work),['One','Two']);
      const applyUrl=`${proxy.origin}${prefix}/__native_sidebar/apply`,finishUrl=`${proxy.origin}${prefix}/__native_sidebar/finish`;
      for(const kind of ['group','project'])await caseT.test(`${prefix||'root'} ${phone?'phone':'desktop'} ${kind}`,async()=>{
        const trigger=kind==='group'?groupTrigger:'Actions for One',original=kind==='group'?originalGroups:['One','Two'];
        const initial=kind==='group'?fixture.groups[0]:fixture.ids,downIds=[initial[1],initial[0],...initial.slice(2)],variant=kind==='group'?'OrderGroups':'OrderProjects';
        let db=await control('inspect'),writes=appliedRequests(seen).length;
        // Exact original boundary, down/up payload and visible-order assertions.
        await tree.getByRole('button',{name:trigger,exact:true}).click();assert.equal(await page.getByRole('menuitem',{name:'Move up',exact:true}).isDisabled(),true);await page.keyboard.press('Escape');
        await completed(page,finishUrl,()=>action(page,tree,trigger,'Move down'),()=>called(seen,writes+1));await orderIs(tree,kind,phone,work,[...original].reverse());await settle(page,seen);
        assert.deepEqual(frozenWrite(appliedRequests(seen)[writes],fixture.account)[variant].ids,downIds);
        assert.deepEqual(await projected(tree,kind,phone,work),[...original].reverse());
        let after=await control('inspect');assert.deepEqual(canonical(after,kind),downIds);unchangedOther(db,after);
        if(kind==='project')unchangedMembership(db,after);
        const events=db.events.slice();events[eventIndex(kind)]++;assert.deepEqual(after.events,events,'One committed reorder publishes one owner-scoped event.');
        db=after;
        await completed(page,finishUrl,()=>action(page,tree,trigger,'Move up'),()=>called(seen,writes+2));await orderIs(tree,kind,phone,work,original);await settle(page,seen);
        assert.deepEqual(frozenWrite(appliedRequests(seen)[writes+1],fixture.account)[variant].ids,initial);assert.deepEqual(await projected(tree,kind,phone,work),original);
        after=await control('inspect');assert.deepEqual(canonical(after,kind),initial);unchangedOther(db,after);
        if(kind==='project')unchangedMembership(db,after);
        events[eventIndex(kind)]++;assert.deepEqual(after.events,events);db=after;
        // Reject the second rank write inside the real transaction, after its
        // earlier changes. Hold both dispatch and genuine terminal reply.
        await control('fault',{kind,enabled:true});const held=await holdApply(page,applyUrl,token,fixture.account);gates.push(held);
        try{
          await action(page,tree,trigger,'Move down');const entered=await bounded(held.entered,2000,'Real withheld apply admission');await called(seen,writes+3);if(entered.error)throw entered.error;
          await orderIs(tree,kind,phone,work,[...original].reverse());
          assert.deepEqual(frozenWrite(appliedRequests(seen)[writes+2],fixture.account)[variant].ids,downIds);
          assert.deepEqual(await projected(tree,kind,phone,work),[...original].reverse(),'Optimistic order is visible');
          assert.deepEqual(await control('inspect'),db,'Withheld real request has not mutated SQLite or published an event.');
          held.dispatch();const result=await bounded(held.fetched,5000,'Real failed apply reply');if(result.error)throw result.error;
          assert.equal(result.applied.catalog,null);assert.equal(typeof result.applied.error,'string');assert.ok(result.applied.error);
          assert.deepEqual(await control('inspect'),db,'The real failed transaction rolled back all earlier rank writes and emitted no event.');
          assert.deepEqual(await projected(tree,kind,phone,work),[...original].reverse(),'Optimistic rows stay visible until the actual failure bytes arrive.');
          await control('fault',{kind,enabled:false});
          await completed(page,finishUrl,async()=>{held.deliver();const error=await bounded(held.finished,5000,'Real failed apply delivery');if(error)throw error;});
          const alert=tree.locator('[role="alert"]:visible');await alert.waitFor();
          await orderIs(tree,kind,phone,work,original);
          assert.deepEqual(await projected(tree,kind,phone,work),original,'Failed write rolls visible order back');
          assert.deepEqual(await control('inspect'),db);
          assert.match(await alert.innerText(),new RegExp(`${kind==='group'?'Group':'Project'} order wasn't saved`));
          assert.equal(await alert.innerText(),`${kind==='group'?'Group':'Project'} order wasn't saved: ${result.applied.error}`,'Retain main\'s exact visible order failure text.');
        }finally{held.release();await control('fault',{kind,enabled:false});}
      });
      // Original canonical reload, other account and no-project-mutation assertions.
      const finalWrite=appliedRequests(seen).length+1;await completed(page,finishUrl,()=>action(page,tree,'Actions for One','Move down'),()=>called(seen,finalWrite));await orderIs(tree,'project',phone,work,['Two','One']);await settle(page,seen);
      await page.reload();await page.locator('[data-native-home]').waitFor();const restored=await surface(page,phone);
      await orderIs(restored,'project',phone,work,['Two','One']);assert.deepEqual(await projected(restored,'project',phone,work),['Two','One'],'Canonical personal order reloads from SQLite');
      const other=await browser.newContext({viewport:phone?{width:360,height:740}:{width:1000,height:760},isMobile:phone,hasTouch:phone,reducedMotion:'reduce'});
      try{
        await cookie(other,proxy.origin,fixture.other_token);const second=await other.newPage();second.setDefaultTimeout(5000);const otherSeen=observations(second);
        assert.equal((await second.goto(`${proxy.origin}${prefix}/`)).status(),200);await second.locator('[data-native-home]').waitFor();const otherTree=await surface(second,phone);
        assert.deepEqual(await projected(otherTree,'project',phone,fixture.groups[1][0]),['One','Two']);assert.equal(appliedRequests(otherSeen).length,0);
        await settle(second,otherSeen);await nativeContract(second,otherSeen);
      }finally{await other.close();}
      const final=await control('inspect');assert.deepEqual(final.actor_order,[fixture.ids[1],fixture.ids[0],...fixture.ids.slice(2)]);assert.deepEqual(final.other_order,fixture.ids);assert.deepEqual(final.events,[2,3]);
      await settle(page,seen);await nativeContract(page,seen);
    }finally{for(const held of gates)held.release();await context.close();await proxy.close();}
  });}finally{await browser.close();input.close();}
});
