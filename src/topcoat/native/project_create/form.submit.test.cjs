// Actual rendered event closures, production cookie and typed procedures.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const path=require('node:path');
const {mountedProxy,launchBrowser}=require(path.join(process.cwd(),'src/topcoat/native/browser_fixture.cjs'));
const upstream=new URL(process.argv[2]),token=process.argv[3];

test('rendered native create recovers an actual conflict then commits once and navigates at every mount',async t=>{
  const browser=await launchBrowser();
  try{
    for(const [index,prefix]of ['', '/app','/ACC'].entries())await t.test(prefix||'root',async()=>{
      const proxy=await mountedProxy(upstream,prefix);
      const context=await browser.newContext({viewport:{width:1000,height:760}}),errors=[],requests=[];
      try{
        await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
        const page=await context.newPage();page.setDefaultTimeout(15000);
        page.on('pageerror',error=>errors.push(error.message));
        page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});
        context.on('request',request=>requests.push(new URL(request.url()).pathname));
        assert.equal((await page.goto(`${proxy.origin}${prefix}/projects/new`)).status(),200);
        await page.locator('#project-name').fill(` Browser created ${index} `);
        await page.locator('#project-id').fill('ACC');
        await page.locator('#project-desc').fill(' Preserved browser description ');
        const create=page.getByRole('button',{name:'Create project',exact:true});
        const alert=page.locator('.native-project-create__actions [role=alert]');
        await create.click();
        await page.waitForFunction(()=>Boolean(document.querySelector('.native-project-create__actions [role=alert]')?.textContent.trim()));
        assert.ok((await alert.textContent()).length>0,'An actual duplicate response reaches the visible runtime error state.');
        assert.equal(new URL(page.url()).pathname,`${prefix}/projects/new`);
        assert.equal(await page.locator('#project-name').inputValue(),` Browser created ${index} `);
        assert.equal(await page.locator('#project-desc').inputValue(),' Preserved browser description ');
        assert.ok(await create.isEnabled(),'The actual async completion clears saving after refusal.');
        await page.locator('#project-id').fill(`BR${index}`);
        await Promise.all([
          page.waitForURL(url=>url.pathname===`${prefix}/BR${index}/overview`),
          create.click(),
        ]);
        assert.equal(new URL(page.url()).origin,proxy.origin);
        assert.equal(new URL(page.url()).pathname,`${prefix}/BR${index}/overview`);
        assert.ok(!(await page.content()).includes('data-lific-session-state'),'Destination is the mounted native overview.');
        assert.equal(requests.some(url=>url.split('/').includes('api')),false,'Generated form commands and destination use native production boundaries.');
        assert.deepEqual(errors,[]);
      }finally{await context.close();await proxy.close();}
    });
  }finally{await browser.close();}
});
