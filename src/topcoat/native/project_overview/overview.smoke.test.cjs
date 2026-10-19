// Actual native overview, real cookie roles and typed procedure/TCP boundaries.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const path=require('node:path');
const {mountedProxy,launchBrowser,settleScroll}=require(path.join(process.cwd(),'src/topcoat/native/browser_fixture.cjs'));
const upstream=new URL(process.argv[2]),adminToken=process.argv[3],fixture=JSON.parse(process.argv[4]);

async function mutate(page,endpoint,action){
  const pending=page.waitForResponse(response=>new URL(response.url()).pathname.endsWith(endpoint));
  await action();assert.ok((await pending).ok(),`Actual ${endpoint} responds successfully.`);
}
async function cookieContext(browser,origin,token,viewport){
  const context=await browser.newContext({viewport,isMobile:viewport.width===360,hasTouch:viewport.width===360});
  await context.addCookies([{name:'lific_token',value:token,url:origin,httpOnly:true,sameSite:'Lax'}]);
  return context;
}

test('native overview real inline controls, roles, local drafts and disclosures at every mount',async t=>{
 const browser=await launchBrowser();
 try{
  for(const [index,prefix]of ['', '/app','/ACC'].entries())await t.test(prefix||'root',async()=>{
   const proxy=await mountedProxy(upstream,prefix);
   const context=await cookieContext(browser,proxy.origin,adminToken,{width:1000,height:760});
   const errors=[],requests=[];
   let phase='initial navigation';
   function collectErrors(page,role){
    page.on('pageerror',error=>errors.push({role,phase,url:page.url(),stack:error.stack||error.message}));
    page.on('console',message=>{if(message.type()==='error')errors.push({role,phase,url:page.url(),message:message.text(),location:message.location()});});
   }
   try{
    const page=await context.newPage();page.setDefaultTimeout(15000);
    collectErrors(page,'admin');
    context.on('request',request=>requests.push(new URL(request.url()).pathname));
    assert.equal((await page.goto(`${proxy.origin}${prefix}/ACC/overview`)).status(),200);
    const root=page.locator('.native-overview');await root.waitFor({state:'visible'});
    const scripts=await page.locator('script[src]').evaluateAll(elements=>elements.map(element=>element.src));
    assert.equal(scripts.length,1);assert.ok(scripts[0].includes('__topcoat-runtime'));
    assert.equal(await page.locator('[data-lific-session-state]').count(),0);
    assert.equal(await root.locator('[data-native-overview-attention]').count(),1);
    await root.getByRole('link',{name:/Visible active initial work/}).waitFor();
    assert.ok(!(await root.textContent()).includes('Private hidden initial work'));
    phase='project name edits';
    const title=root.locator('.native-overview__name');await title.click();
    const name=root.getByRole('textbox',{name:'Project name',exact:true});await name.fill('Canceled overview name');await name.press('Escape');
    assert.notEqual((await title.textContent()).trim(),'Canceled overview name');
    await title.click();await name.fill(` Overview saved ${index} `);
    await mutate(page,'/__native_overview/save_field',()=>name.press('Enter'));
    await page.waitForFunction(expected=>document.querySelector('.native-overview__name')?.textContent.trim()===expected,`Overview saved ${index}`);
    phase='description edits';
    const description=root.locator('.native-overview__description-button');await description.click();
    const text=root.getByRole('textbox',{name:'Project description',exact:true});await text.fill(` Preserved overview description ${index} `);
    await text.press('Enter');assert.ok(await text.isVisible(),'Ordinary Enter remains a multiline description edit.');
    await text.fill(` Preserved overview description ${index} `);
    await mutate(page,'/__native_overview/save_field',()=>text.press('Control+Enter'));
    await text.waitFor({state:'hidden'});
    await page.waitForFunction(expected=>document.querySelector('.native-overview__description-button')?.textContent.includes(expected),`Preserved overview description ${index}`);
    assert.ok((await description.textContent()).includes(`Preserved overview description ${index}`));
    phase='group assignment';
    const group=root.getByRole('combobox',{name:'Sidebar group',exact:true});
    await mutate(page,'/__native_overview/assign_group',()=>group.selectOption(String(fixture.group)));
    assert.equal(await group.inputValue(),String(fixture.group));
    phase='import local drafts';
    const importer=root.locator('[data-native-overview-import]');
    const repo=importer.getByPlaceholder('owner/name',{exact:true}),token=importer.getByPlaceholder('ghp_…',{exact:true});
    await repo.fill('local/draft');await token.fill('Fixture token draft, never sent');
    const preview=importer.getByRole('button',{name:'Preview import',exact:true});
    await page.waitForFunction(()=>document.querySelector('[data-native-overview-import] button')?.disabled===false);
    assert.ok(await preview.isEnabled());
    const retainedRepo=await repo.elementHandle();
    phase='member select and danger drafts';
    const members=root.locator('.native-overview__members');
    const person=members.locator(`#native-overview-member-person-${fixture.project}-trigger`);await person.waitFor();
    const retainedPerson=await person.elementHandle();
    const danger=root.locator('.native-overview__danger');await danger.getByRole('button',{name:'Danger zone',exact:true}).click();
    const rekey=danger.getByRole('textbox',{name:'New project identifier',exact:true});await rekey.fill('HOLD');
    phase='label mutation';
    const labels=root.locator('.native-overview__labels'),newLabel=labels.getByRole('textbox',{name:'New label name',exact:true});
    await newLabel.fill(`Smoke label ${index}`);
    await mutate(page,'/__native_overview/label',()=>labels.locator('.native-overview__label-create').getByRole('button',{name:'Add',exact:true}).click());
    await labels.getByRole('button',{name:`Smoke label ${index}`,exact:true}).waitFor();
    assert.equal(await newLabel.inputValue(),'','Actual completion clears only the new label draft.');
    assert.equal(await repo.inputValue(),'local/draft');assert.equal(await token.inputValue(),'Fixture token draft, never sent');
    assert.ok(await retainedRepo.evaluate(element=>element.isConnected),'Label mutation retains the actual import control owner.');
    assert.ok(await retainedPerson.evaluate(element=>element.isConnected),'Label mutation retains the actual select owner.');
    assert.equal(await rekey.inputValue(),'HOLD','Label mutation retains the actual identifier draft.');
    phase='member menu disclosure';
    await person.scrollIntoViewIfNeeded();await settleScroll(page);await person.click();const personMenu=members.locator(`#native-overview-member-person-${fixture.project}-menu`);await personMenu.waitFor({state:'visible'});
    assert.ok((await personMenu.getByRole('option').allTextContents()).some(value=>value.includes('non_member')));
    await person.press('Escape');assert.ok(await person.evaluate(element=>document.activeElement===element));
    assert.ok(await personMenu.isHidden());
    phase='delete disclosure';
    await danger.getByRole('button',{name:'Delete this project',exact:true}).click();
    const confirmation=danger.getByRole('textbox',{name:'Confirm project identifier',exact:true,includeHidden:true});
    const permanently=danger.getByRole('button',{name:'Delete permanently',exact:true});
    await confirmation.fill('acc');assert.ok(await permanently.isDisabled());await confirmation.fill('ACC');assert.ok(await permanently.isEnabled());
    await danger.getByRole('button',{name:'Cancel',exact:true}).click();await confirmation.waitFor({state:'hidden'});assert.equal(await confirmation.inputValue(),'');
    phase='publication';
    const publishing=root.locator('[data-native-overview-publish]'),publish=publishing.getByRole('button',{name:'Publish issues',exact:true});
    assert.ok(await publish.isDisabled());await publishing.getByRole('checkbox').check();
    await mutate(page,'/__native_overview/publish',()=>publish.click());
    const turnOff=publishing.getByRole('button',{name:'Turn off public access',exact:true});await turnOff.waitFor({state:'visible'});
    assert.ok((await publishing.locator('.native-overview-publish__address code').textContent()).endsWith(`${prefix}/public/ACC`));
    await mutate(page,'/__native_overview/publish',()=>turnOff.click());await publish.waitFor({state:'visible'});assert.ok(await publish.isDisabled());
    phase='archive acknowledgement';
    const archive=root.locator('.native-overview__archive');await archive.waitFor();
    const download=archive.getByRole('button',{name:'Download project archive',exact:true});assert.ok(await download.isDisabled());
    await archive.getByRole('checkbox').check();assert.ok(await download.isEnabled());await archive.getByRole('checkbox').uncheck();assert.ok(await download.isDisabled());
    assert.equal(requests.some(url=>url.split('/').includes('api')),false,'Native UI uses genuine native procedures and shards.');
    assert.deepEqual(errors,[]);
    phase='read-only viewer navigation';
    const viewer=await cookieContext(browser,proxy.origin,fixture.viewer,{width:360,height:740});
    try{
     const readonly=await viewer.newPage();readonly.setDefaultTimeout(15000);collectErrors(readonly,'viewer');
     assert.equal((await readonly.goto(`${proxy.origin}${prefix}/ACC/overview`)).status(),200);
     const document=readonly.locator('.native-overview');await document.locator('h1.native-overview__name').waitFor();
     assert.equal((await document.locator('h1.native-overview__name').textContent()).trim(),`Overview saved ${index}`);
     assert.ok(await document.getByText('Read-only',{exact:true}).isVisible());
     assert.equal(await document.locator('.native-overview__name-input:visible,.native-overview__danger,[data-native-overview-publish],[data-native-overview-import],.native-overview__members,.native-overview__archive').count(),0);
     assert.equal(await document.getByRole('textbox',{name:'New label name',exact:true}).count(),0);
     await document.getByText(`Smoke label ${index}`,{exact:true}).waitFor({state:'visible'});
     assert.ok(await document.getByText(`Smoke label ${index}`,{exact:true}).isVisible());
     assert.deepEqual(errors,[]);
    }finally{await viewer.close();}
   }finally{await context.close();await proxy.close();}
  });
 }finally{await browser.close();}
});
