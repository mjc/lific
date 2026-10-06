// Real stale sessions, mobile password controls and frozen drafts at every mount.
// The fixture supplies independent stale sessions: successful confirmation rotates only its presented cookie.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const path=require('node:path');
const {mountedProxy,launchBrowser}=require(path.join(process.cwd(),'src/topcoat/native/browser_fixture.cjs'));
const upstream=new URL(process.argv[2]), fixture=JSON.parse(process.argv[4]);
const transportFailure=process.argv[5]==='transport';

async function cookieValue(context){return (await context.cookies()).find(cookie=>cookie.name==='lific_token')?.value;}

test('mobile stale-cookie confirmation cancels, resets password, retries and commits only the frozen redraft at every mount',async t=>{
  const browser=await launchBrowser();
  try{
    for(const [index,prefix]of ['', '/app','/ACC'].entries())await t.test(prefix||'root',async()=>{
      const proxy=await mountedProxy(upstream,prefix);
      const context=await browser.newContext({viewport:{width:360,height:740},isMobile:true,hasTouch:true}),errors=[],requests=[],expectedTransportErrors=[];
      try{
        const token=fixture.tokens[index];
        await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
        const page=await context.newPage();page.setDefaultTimeout(15000);
        page.on('pageerror',error=>errors.push(error.message));
        page.on('console',message=>{
          if(message.type()!=='error')return;
          const url=message.location().url;
          if(transportFailure&&url&&new URL(url).pathname===`${prefix}/__native_project/confirm_and_create`&&
            /^Failed to load resource: net::ERR_(?:EMPTY_RESPONSE|CONNECTION_RESET|FAILED)$/.test(message.text()))expectedTransportErrors.push(message.text());
          else errors.push(message.text());
        });
        context.on('request',request=>requests.push(new URL(request.url()).pathname));
        assert.equal((await page.goto(`${proxy.origin}${prefix}/projects/new`)).status(),200);
        const name=page.locator('#project-name'),identifier=page.locator('#project-id'),description=page.locator('#project-desc');
        await name.fill(`Draft before reauth ${index}`);await identifier.fill(`RC${index}`);await description.fill('Freeze note');
        await page.locator('#native-project-lead-trigger').click();
        const menu=page.locator('#native-project-lead-menu');
        const leadName=menu.getByText(fixture.leadLabel,{exact:true});
        await leadName.locator('xpath=ancestor::button').click();
        const create=page.getByRole('button',{name:'Create project',exact:true});
        const password=page.getByPlaceholder('Current password',{exact:true});
        const card=page.locator('.native-project-create__reauth');
        const verify=card.getByRole('button',{name:'Verify and create',exact:true});
        const cancel=card.getByRole('button',{name:'Cancel',exact:true});
        await create.click();await password.waitFor({state:'visible'});
        assert.ok(await page.evaluate(()=>matchMedia('(pointer:coarse)').matches),'This is an actual coarse pointer browser context.');
        assert.equal(await password.evaluate(element=>getComputedStyle(element).fontSize),'16px','Password uses the same iOS zoom floor as the pinned master mobile form.');
        assert.ok(await verify.isDisabled());assert.ok(await create.isDisabled());
        await password.fill('Password discarded on cancellation');
        await name.fill(`Edited after prompt ${index}`);await identifier.fill(`RD${index}`);await description.fill(`Changed while pending ${index}`);
        await cancel.click();await password.waitFor({state:'hidden'});
        assert.equal(await password.inputValue(),'','Cancel clears the credential.');
        assert.equal(await name.inputValue(),`Edited after prompt ${index}`);
        assert.equal(await identifier.inputValue(),`RD${index}`);
        assert.equal(await description.inputValue(),`Changed while pending ${index}`);
        assert.ok(await create.isEnabled());
        assert.ok((await cookieValue(context))===token,'Cancellation does not rotate the real session.');
        await create.click();await password.waitFor({state:'visible'});
        let failedRequest;
        if(transportFailure){
          failedRequest=page.waitForEvent('requestfailed',request=>new URL(request.url()).pathname===`${prefix}/__native_project/confirm_and_create`);
          proxy.abortNextRequest('/__native_project/confirm_and_create');
          await password.fill('testpassword1');
        }else await password.fill('wrong password');
        await password.press('Enter');
        const feedback=card.getByRole('alert');await feedback.waitFor({state:'visible'});
        if(transportFailure){
          assert.deepEqual(proxy.abortedRequests,[`${prefix}/__native_project/confirm_and_create`]);
          assert.match((await failedRequest).failure().errorText,/^net::ERR_/,'The actual browser reports a failed socket request.');
          assert.equal((await feedback.textContent()).trim(),'Unable to verify. Your draft is still here. Try again.');
        }
        assert.ok((await feedback.textContent()).trim().length>0,transportFailure?'Actual transport failure is visible.':'Actual password rejection is visible.');
        assert.equal(await password.inputValue(),'','A rejected password is cleared.');
        assert.ok(await verify.isDisabled());assert.ok(await create.isDisabled());
        assert.ok((await cookieValue(context))===token,transportFailure?'Failed transport leaves the presented real session intact.':'Wrong password leaves the presented real session intact.');
        if(transportFailure){
          assert.ok(proxy.canceledTcpAttempts.length>=1,'At least one actual TCP attempt was canceled.');
          assert.ok(proxy.canceledTcpAttempts.every(attempt=>attempt.method==='POST'&&attempt.path===`${prefix}/__native_project/confirm_and_create`),'Only the armed confirmation operation is canceled, including transparent retries.');
          proxy.releaseRequestCancellation();
        }
        // Editing the live fields after the second prompt must not replace its captured draft.
        await name.fill(`Later editable ${index}`);await identifier.fill(`L${index}`);await description.fill(`Later draft ${index}`);
        await password.fill('testpassword1');
        const destination=page.waitForResponse(response=>response.request().isNavigationRequest()&&new URL(response.url()).pathname===`${prefix}/RD${index}/overview`);
        await Promise.all([page.waitForURL(url=>url.pathname===`${prefix}/RD${index}/overview`),verify.click()]);
        assert.equal((await destination).status(),200,'The committed destination is served by the production native router.');
        assert.equal(new URL(page.url()).origin,proxy.origin);
        assert.ok((await cookieValue(context))!==token,'Successful password confirmation rotates the presented cookie.');
        assert.ok(!(await page.content()).includes('data-lific-session-state'),'Destination has no parked browser controller.');
        await page.locator('.native-overview').waitFor({state:'visible'});
        assert.equal((await page.locator('.native-overview__hero .native-overview__name').textContent()).trim(),`Edited after prompt ${index}`,'The fresh real overview binds the committed project name.');
        assert.equal(requests.some(url=>url.split('/').includes('api')),false,'Typed native confirmation and destination use production native boundaries.');
        assert.deepEqual(errors,[]);
        if(transportFailure)assert.equal(expectedTransportErrors.length,1,'Only the armed confirmation TCP cancellation produces a console transport error.');
      }finally{await context.close();await proxy.close();}
    });
  }finally{await browser.close();}
});
