// Port of e2e/palette-search.ts: real executable, persisted seed, actual FTS.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {startFixture}=require('../../../acceptance/server.js');
const WARMUP_ISSUES=7,BODY_ONLY_TOKEN='zarquontoken',MIXED_TOKEN='mixedtoken';
async function create(fixture,path,body){const r=await fixture.api(path,{method:'POST',body});assert.ok(r.ok,await r.clone().text());return r.json();}
const rows=page=>page.locator('[data-palette-results] [role=option]').allTextContents();
async function open(page){await page.keyboard.press('Control+k');const input=page.locator('[data-palette-input]');await input.waitFor({state:'visible'});return input;}
async function warm(page,f,project='PAL'){await page.goto(f.url(`/${project}/issues`));await page.waitForFunction(n=>document.querySelectorAll('[data-issue-id]').length>=n,project==='PAL'?7:1);}
test('main palette search preserves local-first real-app contracts',{skip:!process.env.PLAYWRIGHT_EXECUTABLE_PATH,timeout:120000},async t=>{
 const f=await startFixture();let page;const pageErrors=[],consoleErrors=[];
 try{
  const project=await create(f,'/projects',{name:'Palette',identifier:'PAL'});
  const demo=await create(f,'/projects',{name:'Demo',identifier:'DEMO'});
  await create(f,'/issues',{project_id:demo.id,title:'Demo issue'});
  for(let i=1;i<=WARMUP_ISSUES;i++)await create(f,'/issues',{project_id:project.id,title:`Palette warmup ${i}`,description:`Seeded for the palette search check, number ${i}.`,status:'active'});
  await create(f,'/issues',{project_id:project.id,title:'Cold storage note',description:`${'lorem ipsum dolor sit amet '.repeat(14)}\n\n${BODY_ONLY_TOKEN} appears only in the body.`,status:'backlog'});
  await create(f,'/pages',{project_id:project.id,title:'Palette warmup handbook',content:'# Palette warmup handbook\n\nHow the warm read model is searched.'});
  await create(f,'/issues',{project_id:project.id,title:`${MIXED_TOKEN} local issue`,description:'Found in memory, by title.',status:'active'});
  await create(f,'/pages',{project_id:project.id,title:`${MIXED_TOKEN} local page`,content:`# ${MIXED_TOKEN} local page\n\nFound in memory, by title.`});
  await create(f,'/issues',{project_id:project.id,title:'Buried reference note',description:`${'lorem ipsum dolor sit amet '.repeat(14)}\n\n${MIXED_TOKEN} lives only in this body.`,status:'active'});
  page=await f.newPage();page.setDefaultTimeout(5000);await page.setViewportSize({width:1440,height:900});
  page.on('pageerror',e=>pageErrors.push(String(e)));page.on('console',m=>{if(m.type()==='error')consoleErrors.push(m.text());});
  let searchRequests=0;page.on('request',r=>{if(new URL(r.url()).pathname===`${f.prefix}/api/search`)searchRequests++;});
  await t.test('1 local hits render before network; FTS suppressed; page survives issue flood',async()=>{
   await warm(page,f);const input=await open(page),before=searchRequests,t0=Date.now();await input.fill('warmup');
   await page.waitForFunction(()=>[...document.querySelectorAll('[data-palette-results] [role=option]')].filter(el=>el.innerText.includes('Palette warmup')).length>=6,{},{timeout:2000});
   assert.equal(searchRequests-before,0,'local results must render before any FTS request');assert.ok(Date.now()-t0<2000);
   await page.waitForTimeout(700);assert.equal(searchRequests-before,0,'FTS must be skipped when local hits >=5');
   const visible=await rows(page);assert.equal(visible.filter(r=>r.includes('Palette warmup handbook')).length,1);assert.ok(visible.filter(r=>/Palette warmup \d/.test(r)&&r.includes('PAL-')).length>=6);
  });
  await t.test('2 exact reference resolves one row and Enter navigates actual issue',async()=>{
   await warm(page,f);const input=await open(page);await input.fill('PAL-3');await page.waitForTimeout(700);const visible=await rows(page);
   assert.equal(visible.filter(r=>r.includes('PAL-3')).length,1);assert.equal(new Set(visible).size,visible.length);
   await input.press('Enter');await page.waitForURL(f.url('/PAL/issues/PAL-3'));assert.match(await page.locator('body').innerText(),/Palette warmup 3/);
  });
  await t.test('3 body-only token reaches real server FTS',async()=>{
   await warm(page,f);const input=await open(page),before=searchRequests;await input.fill(BODY_ONLY_TOKEN);
   await page.getByRole('option').filter({hasText:'Cold storage note'}).waitFor();assert.ok(searchRequests>before);
  });
  await t.test('3b all local issue/page rows precede server rows and server section boundary',async()=>{
   await warm(page,f);await(await open(page)).fill(MIXED_TOKEN);await page.getByRole('option').filter({hasText:'Buried reference note'}).waitFor();
   const visible=await rows(page),issue=visible.findIndex(r=>r.includes(`${MIXED_TOKEN} local issue`)),document=visible.findIndex(r=>r.includes(`${MIXED_TOKEN} local page`)),server=visible.findIndex(r=>r.includes('Buried reference note'));
   assert.ok(issue>=0&&document>=0&&server>=0);assert.ok(issue<server&&document<server,'server result must follow all local rows');
   const structure=await page.locator('[data-palette-results]').evaluate(el=>[...el.children].map(e=>e.textContent));const boundary=structure.findIndex(text=>text.includes('(server'));
   assert.ok(boundary>=0);assert.ok(structure.slice(0,boundary).some(text=>text.includes(`${MIXED_TOKEN} local page`)));assert.ok(!structure.slice(boundary).some(text=>text.includes(`${MIXED_TOKEN} local issue`)||text.includes(`${MIXED_TOKEN} local page`)));
  });
  await page.route('**/api/search*',async route=>{await new Promise(resolve=>setTimeout(resolve,1500));await route.continue().catch(()=>{});});
  await t.test('4a delayed real FTS cannot repaint a newer query',async()=>{
   await warm(page,f);const input=await open(page);await input.fill(BODY_ONLY_TOKEN);await page.waitForTimeout(300);await input.fill('warmup');await page.waitForTimeout(2500);
   const visible=await rows(page);assert.ok(!visible.some(r=>r.includes('Cold storage note')));assert.ok(visible.some(r=>r.includes('Palette warmup ')));
  });
  await t.test('4b project navigation reissues search without duplicated or old local rows',async()=>{
   await warm(page,f);await(await open(page)).fill(BODY_ONLY_TOKEN);await page.waitForTimeout(300);const before=searchRequests;
   // Topcoat uses a new document for navigation. Reopen the same query there.
   await warm(page,f,'DEMO');await(await open(page)).fill(BODY_ONLY_TOKEN);await page.waitForTimeout(2500);
   assert.ok(searchRequests>before);const visible=await rows(page);assert.equal(visible.filter(r=>r.includes('Cold storage note')).length,1);assert.equal(new Set(visible).size,visible.length);
  });
  await t.test('4c close and reopen rejects an earlier session response',async()=>{
   await warm(page,f);await(await open(page)).fill(BODY_ONLY_TOKEN);await page.waitForTimeout(300);await page.keyboard.press('Escape');await page.locator('[data-palette-input]').waitFor({state:'hidden'});await page.waitForTimeout(1800);
   await open(page);await page.waitForTimeout(300);assert.ok(!(await rows(page)).some(r=>r.includes('Cold storage note')));
  });
  await page.unroute('**/api/search*');
  await t.test('5 phone local-first results, no overflow and visible close control',async()=>{
   await page.setViewportSize({width:390,height:844});await warm(page,f);const before=searchRequests;await(await open(page)).fill('warmup');
   await page.waitForFunction(()=>[...document.querySelectorAll('[data-palette-results] [role=option]')].filter(el=>el.innerText.includes('Palette warmup')).length>=6,{},{timeout:2000});
   await page.waitForTimeout(700);assert.equal(searchRequests-before,0);assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await page.locator('[data-palette-close]').click();await page.locator('[data-palette-input]').waitFor({state:'hidden'});
  });
  await t.test('no uncaught browser or console errors',()=>{assert.deepEqual(pageErrors,[]);assert.deepEqual(consoleErrors,[]);});
 }finally{await f.close();}
});
