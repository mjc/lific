// Real native ProjectNew controls through the production server and framework.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const {mountedProxy, launchBrowser} = require(path.join(process.cwd(), 'src/topcoat/native/browser_fixture.cjs'));
const upstream = new URL(process.argv[2]), token = process.argv[3];

test('native ProjectNew picker, selects, touched prefix, and mounted destinations', async t => {
  const browser = await launchBrowser();
  try {
    for (const prefix of ['', '/app', '/ACC']) {
      await t.test(prefix || 'root', async () => {
        const proxy = await mountedProxy(upstream, prefix);
        const context = await browser.newContext({viewport:{width:1000,height:760}});
        try {
          await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
          const page = await context.newPage(), errors=[], requests=[];
          page.on('pageerror', error=>errors.push(error.message));
          page.on('console', message=>{if(message.type()==='error')errors.push(message.text());});
          context.on('request', request=>requests.push(new URL(request.url()).pathname));
          const response = await page.goto(`${proxy.origin}${prefix}/projects/new`);
          assert.equal(response.status(),200);
          const html=await response.text();
          const scripts=[...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"/g)].map(match=>match[1]);
          assert.equal(scripts.length,1); assert.ok(scripts[0].includes('__topcoat-runtime'));
          assert.ok(!html.includes('project-settings.js') && !html.includes('data-lific-session-state'));
          const create = page.getByRole('button',{name:'Create project',exact:true});
          assert.ok(await create.isDisabled());
          await page.locator('#project-name').fill('Half-Life 3');
          await page.waitForFunction(()=>document.querySelector('#project-id')?.value==='HALFL');
          assert.equal(await page.locator('.native-project-create__preview').textContent(),'HALFL-1');
          assert.ok(await create.isEnabled());
          const lead=page.locator('#native-project-lead-trigger');
          assert.ok(await lead.locator('.native-project-select__avatar').isHidden(), 'No lead has no avatar.');
          await lead.focus(); await lead.press('ArrowDown');
          assert.ok(await page.locator('#native-project-lead-menu').isVisible(), JSON.stringify({errors,expanded:await lead.getAttribute('aria-expanded'),hidden:await page.locator('#native-project-lead-menu').getAttribute('hidden')}));
          const firstLead=page.locator('#native-project-lead-menu [role=option]').nth(1);
          const firstLeadName=await firstLead.locator('.native-project-select__member-name').textContent();
          await lead.press('ArrowDown');
          assert.equal(await lead.locator('.native-project-select__selected>span').nth(1).textContent(),firstLeadName);
          assert.equal(await firstLead.getAttribute('aria-selected'),'true');
          await lead.press('Escape');
          assert.ok(await lead.evaluate(element=>document.activeElement===element));
          assert.ok(!await page.locator('#native-project-lead-menu').isVisible());
          await lead.click();
          const rect=await page.locator('#native-project-lead-menu').boundingBox();
          assert.ok(rect.x>=8 && rect.x+rect.width<=992);
          await page.locator('#native-project-lead-menu').evaluate(element=>element.dispatchEvent(new Event('scroll',{bubbles:true})));
          assert.ok(await page.locator('#native-project-lead-menu').isVisible());
          await page.locator('.native-project-create-page').evaluate(element=>element.dispatchEvent(new Event('scroll',{bubbles:true})));
          assert.ok(!await page.locator('#native-project-lead-menu').isVisible());
          const group=page.locator('#native-project-group-trigger'); assert.equal(await group.count(),1);
          await group.click(); await page.getByRole('option',{name:'Personal projects',exact:true}).click();
          assert.ok((await group.textContent()).includes('Personal projects'));
          await page.locator('#native-project-icon-trigger').click();
          const panel=page.locator('.native-project-picker__panel');
          await page.locator('.native-project-picker-choice').first().waitFor();
          assert.ok(await page.locator('.native-project-picker-choice').count()<=96);
          await page.locator('#native-project-icon-search').fill('folder');
          await page.waitForFunction(()=>[...document.querySelectorAll('.native-project-picker-choice')].every(element=>element.title.toLowerCase().includes('folder')));
          await panel.getByRole('button',{name:'Emoji',exact:true}).click();
          assert.equal(await page.locator('#native-project-icon-search').inputValue(),'folder','Tab change retains source search.');
          await page.locator('#native-project-icon-search').fill('logo');
          await panel.getByRole('button',{name:'lific logo',exact:true}).click();
          assert.ok(!await panel.isVisible());
          await page.locator('#native-project-icon-trigger').click();
          await panel.getByRole('button',{name:'Remove icon',exact:true}).click();
          assert.ok(!await panel.isVisible());
          await page.locator('#native-project-icon-trigger').click(); await page.keyboard.press('Escape');
          assert.ok(await page.locator('#native-project-icon-trigger').evaluate(element=>document.activeElement===element));
          assert.ok(!await panel.isVisible());
          await page.setViewportSize({width:360,height:740});
          await page.locator('#native-project-icon-trigger').click();
          const mobile=await panel.boundingBox(); assert.ok(mobile.x>=12 && mobile.x+mobile.width<=348);
          await page.keyboard.press('Escape');
          const cancel=page.locator('.native-project-create__actions').getByRole('link',{name:'Cancel',exact:true});
          assert.equal(await cancel.getAttribute('href'),`${prefix}/settings`);
          // Load the actual Unicode font subset before measuring offline controls.
          await page.locator('#project-name').evaluate(async element=>{
            const style=getComputedStyle(element);
            const font=`${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
            await document.fonts.load(font,'ß é1 🦀ſﬃ');
            await document.fonts.ready;
            const assertFontLoaded=document.fonts.check(font,'ß é1 🦀ſﬃ');
            if(!assertFontLoaded)throw new Error('Unicode input font did not load.');
          });
          const networkBefore=requests.length;
          await context.setOffline(true);
          await page.locator('#project-name').fill('ß é1 🦀ſﬃ');
          assert.equal(await page.locator('#project-id').inputValue(),'SS1SF','Prefix stays local with network unavailable.');
          await page.locator('#project-name').fill('');
          assert.equal(await page.locator('#project-id').inputValue(),'SS1SF');
          await page.locator('#project-id').fill('OWN');
          await page.locator('#project-name').fill('New name');
          assert.equal(await page.locator('#project-id').inputValue(),'OWN','Manual edit freezes automatic identifier.');
          await page.locator('#project-name').fill('\uFEFF');
          assert.ok(await create.isDisabled(), 'ECMAScript BOM whitespace disables Save.');
          await page.locator('#project-name').fill('\u0085');
          assert.ok(await create.isEnabled(), 'ECMAScript NEL is a nonempty Name.');
          await page.locator('#project-id').fill('\uFEFFabc\uFEFF');
          assert.equal(await page.locator('.native-project-create__preview').textContent(),'ABC-1');
          await page.locator('#project-id').fill('\u0085abc\u0085');
          assert.equal(await page.locator('.native-project-create__preview').textContent(),'\u0085ABC\u0085-1');
          assert.equal(requests.length,networkBefore,'Name/identifier controls make no requests: '+JSON.stringify({newRequests:requests.slice(networkBefore),errors}));
          assert.equal(requests.some(path=>path.split('/').includes('api')),false);
          assert.deepEqual(errors,[]);
        } finally {await context.close(); await proxy.close();}
      });
    }
  } finally {await browser.close();}
});
