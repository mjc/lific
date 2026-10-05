// Preserve pinned 9683d38 e2e/sidebar.ts:283 on actual native routes.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {mountedProxy,launchBrowser,cookie,observations,nativeContract,attr,appliedRequests,settle}=require('./sidebar.browser.fixture.cjs');
const upstream=new URL(process.argv[2]),token=process.argv[3],fixture=JSON.parse(process.argv[4]);
test(`separate disclosure and Overview links; auth ${fixture.auth_required?'required':'optional'}`,async t=>{
  const browser=await launchBrowser();
  try{for(const prefix of ['', '/app','/ACC'])await t.test(prefix||'root',async()=>{
    const proxy=await mountedProxy(upstream,prefix),context=await browser.newContext({viewport:{width:1000,height:760},reducedMotion:'reduce'});
    try{
      await cookie(context,proxy.origin,token);const page=await context.newPage();page.setDefaultTimeout(5000);const seen=observations(page);
      const response=await page.goto(`${proxy.origin}${prefix}/`);assert.equal(response.status(),200);
      const html=await response.text();assert.ok(html.includes('native-home-shell'));assert.ok(!html.includes('data-lific-session-state'));
      await page.locator('[data-native-home]').waitFor();
      const aside=page.getByRole('complementary',{name:'Workspace sidebar',exact:true});
      assert.equal(await aside.locator('[data-native-sidebar-project]').count(),2);
      const documents=seen.requests.filter(request=>request.type==='document').length;
      await page.evaluate(()=>{window.nativeSidebarDisclosureOwner=document.querySelector('.native-home-shell');});
      // Exact original disclosure/navigation/independence predicates follow.
      await aside.getByRole('button',{name:'Expand One',exact:true}).click();
      await aside.getByRole('button',{name:'Collapse One',exact:true}).waitFor();
      assert.equal(new URL(page.url()).pathname,`${prefix}/`);
      await aside.getByRole('button',{name:'Expand Two',exact:true}).click();
      await aside.getByRole('button',{name:'Collapse Two',exact:true}).waitFor();
      await aside.locator('a[title="One"]').click();await page.waitForURL(`${proxy.origin}${prefix}/ACC/overview`);
      await page.locator('.native-overview').waitFor();
      await attr(aside.locator('a[title="One"]'),'aria-current','page');
      await attr(aside.locator(`#project-nav-${fixture.one}`),'hidden',null);
      await attr(aside.locator(`#project-nav-${fixture.two}`),'hidden',null);
      await aside.getByRole('button',{name:'Collapse One',exact:true}).click();
      await aside.getByRole('button',{name:'Expand One',exact:true}).waitFor();
      assert.equal(new URL(page.url()).pathname,`${prefix}/ACC/overview`);
      await attr(aside.locator(`#project-nav-${fixture.one}`),'hidden','');
      await attr(aside.locator(`#project-nav-${fixture.two}`),'hidden',null);
      assert.ok(await page.evaluate(()=>document.querySelector('.native-home-shell')===window.nativeSidebarDisclosureOwner),'Actual Overview navigation retains the native workspace owner.');
      assert.equal(seen.requests.filter(request=>request.type==='document').length,documents,'Disclosure continuity is observed during genuine native link navigation.');
      assert.equal(appliedRequests(seen).length,0,'Presentation disclosure does not issue a write receipt.');
      await settle(page,seen);await nativeContract(page,seen);
    }finally{await context.close();await proxy.close();}
  });}finally{await browser.close();}
});
