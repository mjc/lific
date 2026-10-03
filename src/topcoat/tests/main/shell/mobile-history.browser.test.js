// Remaining original component lifecycle/history assertions against mobile.js.
const {test}=require('node:test');const assert=require('node:assert/strict');
const fs=require('node:fs');const path=require('node:path');const vm=require('node:vm');const {createServer}=require('node:http');
const {root,read}=require('./source.js');
test('main mobile original lifecycle and hostile history cases',{skip:!process.env.PLAYWRIGHT_EXECUTABLE_PATH,timeout:60000},async t=>{
 const text=read('shell/assets/mobile.test.js');const begin=text.indexOf('function fixture('),end=text.indexOf('\n\ntest(',begin);
 const catalog={generation:1,projects:[{id:1,identifier:'ONE',name:'One',emoji:'1'},{id:2,identifier:'TWO',name:'Two'}],groups:[{id:1,name:'Work',sort_order:0,project_ids:[1,2]}]};
 const fixture=vm.runInNewContext(text.slice(begin,end)+';fixture',{catalog,css:read('shell/assets/mobile.css')});
 const server=createServer((req,res)=>{if(req.url==='/mobile.js'){res.setHeader('Content-Type','text/javascript');res.end(read('shell/assets/mobile.js'));return;}
  res.setHeader('Content-Type','text/html');res.end(fixture().replace('<script src="/mobile.js"></script>',`<button id="direct">Direct</button><script>
   document.querySelector('#direct').onclick=()=>lificMobileNavigation.openAt('ONE');
   const mount=()=>{const script=document.createElement('script');script.src='/mobile.js';document.body.append(script);};
   if(sessionStorage.getItem('hold-mobile-mount')){sessionStorage.removeItem('hold-mobile-mount');const panel=document.querySelector('[data-mobile-navigation]');panel.remove();window.finishLoading=()=>{document.body.append(panel);mount();};}else mount();
  </script>`));});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const origin=`http://127.0.0.1:${server.address().port}`;
 const {chromium}=await import(path.resolve(__dirname,'../../../../../e2e/node_modules/playwright/index.mjs'));
 const browser=await chromium.launch({headless:true,executablePath:process.env.PLAYWRIGHT_EXECUTABLE_PATH});const errors=[];
 const page=await browser.newPage({viewport:{width:390,height:844}});page.setDefaultTimeout(4000);page.on('pageerror',error=>errors.push(error.message));
 const depth=n=>page.waitForFunction(n=>history.state?.lificMobileNav?.depth===n,n);
 const closed=()=>page.waitForFunction(()=>document.querySelector('[data-mobile-navigation]').inert);
 const active=()=>page.evaluate(()=>document.activeElement?.getAttribute('aria-label')||document.activeElement?.id||document.activeElement?.textContent?.trim());
 let serial=0;async function fresh(){await page.goto(origin+'/ONE/issues?case='+serial++);await page.waitForFunction(()=>!!window.lificMobileNavigation);}
 try{
  await t.test('direct project trigger restores its own focus after both drawer entries unwind',async()=>{
   await fresh();await page.locator('#direct').click();await depth(2);await page.keyboard.press('Escape');await depth(1);await page.keyboard.press('Escape');await depth(0);await closed();assert.equal(await active(),'direct');
   await page.locator('[data-mobile-root] a[href="/"]').evaluate(el=>el.focus());assert.equal(await active(),'direct');
  });
  await t.test('programmatic focus cannot escape either active pane',async()=>{
   await fresh();await page.locator('#open').click();await depth(1);await page.locator('#open').evaluate(el=>el.focus());assert.equal(await page.evaluate(()=>document.querySelector('[data-mobile-root]').contains(document.activeElement)),true);
   await page.locator('[data-mobile-project-trigger="ONE"]').click();await depth(2);await page.locator('[data-mobile-root] a[href="/"]').evaluate(el=>el.focus());assert.equal(await active(),'Projects');
  });
  await t.test('delayed catalog mount adopts a restored project entry without pushing history',async()=>{
   await fresh();await page.locator('#open').click();await depth(1);await page.locator('[data-mobile-project-trigger="ONE"]').click();await depth(2);
   const before=await page.evaluate(()=>({state:history.state,length:history.length}));await page.evaluate(()=>sessionStorage.setItem('hold-mobile-mount','1'));await page.reload();await page.waitForFunction(()=>typeof finishLoading==='function');assert.equal(await page.locator('[data-mobile-navigation]').count(),0);
   await page.evaluate(()=>finishLoading());await page.getByRole('heading',{name:'One',exact:true}).waitFor();assert.equal(await page.locator('[data-mobile-project]').getAttribute('inert'),null);assert.equal(await active(),'Projects');assert.deepEqual(await page.evaluate(()=>({state:history.state,length:history.length})),before);
  });
  await t.test('rapid repeated and reversed traversals restore the same project without navigation requests',async()=>{
   await fresh();await page.locator('#open').click();await depth(1);await page.locator('[data-mobile-project-trigger="ONE"]').click();await depth(2);const owned=page.url();await page.evaluate(()=>dispatchEvent(new CustomEvent('lific:navigate',{detail:{href:'/another-route'}})));await closed();await page.goBack();await depth(2);
   await page.evaluate(()=>{history.back();history.back();});await depth(0);await closed();await page.evaluate(()=>{history.forward();history.forward();});await depth(2);assert.equal(page.url(),owned);assert.equal(await page.locator('[data-mobile-project]').getAttribute('inert'),null);assert.deepEqual(await page.evaluate(()=>navigationRequests),[{href:'/another-route',baseDepth:2}]);
   await page.evaluate(()=>new Promise(resolve=>{let events=0;const onPop=()=>{if(++events===1)history.back();else{removeEventListener('popstate',onPop);resolve();}};addEventListener('popstate',onPop);history.forward();}));
   assert.equal(page.url(),owned);assert.equal(await page.locator('[data-mobile-project]').getAttribute('inert'),null);
  });
  await t.test('rejects every malformed or foreign original namespace field on reload',async()=>{
   for(const invalid of [{version:2},{session:'not-a-session'},{depth:99},{depth:'1'},{project:'ONE'},{depth:2,project:'../outside'},{href:'https://elsewhere.invalid/'}]){
    await fresh();await page.locator('#open').click();await depth(1);await page.evaluate(invalid=>history.replaceState({...history.state,lificMobileNav:{...history.state.lificMobileNav,...invalid}},''),invalid);await page.reload();await page.waitForFunction(()=>!!window.lificMobileNavigation);await closed();assert.equal(await page.locator('[data-mobile-navigation]').getAttribute('data-open'),'false');
    await page.locator('#open').click();await depth(1);await page.locator('[data-mobile-close]').first().click();await depth(0);await closed();await page.goForward();await depth(1);
   }
  });
  assert.deepEqual(errors,[]);
 }finally{await browser.close();await new Promise(resolve=>server.close(resolve));}
});
