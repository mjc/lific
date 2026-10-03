// Ports the real-server assertions in main's smoke.ts, issue-waits.ts and duplicate-relations.ts.
const {test, before, after, assert, expect} = require('./harness');
const {startFixture} = require('../../../acceptance/server');
let fixture;
before(async()=>{fixture=await startFixture();});
after(async()=>{await fixture?.close();});
async function create(route,body){const r=await fixture.api(route,{method:'POST',body});assert.ok(r.ok,`${route}: ${r.status} ${await r.clone().text()}`);return r.json();}
async function issue(title){return create('/issues',{project_id:fixture.project.id,title,status:'todo'});}
function day(offset){const d=new Date();d.setDate(d.getDate()+offset);return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;}
async function open(route){const page=await fixture.newPage();page.setDefaultTimeout(3000);page.setDefaultNavigationTimeout(15000);page._portErrors=[];page.on('pageerror',error=>page._portErrors.push(error.message));await page.goto(fixture.url(route));return page;}
async function waitIssue(){const value=await issue('Waiting smoke issue');const page=await open(`/ACC/issues/${value.identifier}`);await page.locator('[data-wait-create]').waitFor();return {value,page,form:page.locator('[data-wait-create]')};}
async function addPerson(form){await form.locator('[name=kind]').selectOption('user');await form.locator('[name=user]').fill(fixture.credentials.identity);await form.locator('[name=note]').fill('Decide the scope');await form.getByRole('button',{name:'Add wait',exact:true}).click();}
async function addDates(form){await form.locator('[name=kind]').selectOption('date');await form.locator('[name=from]').fill(day(-4));await form.locator('[name=until]').fill(day(-1));await form.locator('[name=note]').fill('State filing office');await form.getByRole('button',{name:'Add wait',exact:true}).click();}
async function rows(id){const r=await fixture.api(`/issues/${id}/waits`);assert.equal(r.status,200);return r.json();}
test('smoke: real login creates bearer and cookie and reaches the app',async()=>{
 const page=await fixture.newPage({authenticated:false});try{
 page.setDefaultTimeout(3000);page.setDefaultNavigationTimeout(15000);await page.goto(fixture.url('/login'));await page.getByLabel('Username or email').fill(fixture.credentials.identity);await page.locator('input[name=password]').fill(fixture.credentials.password);await page.getByRole('button',{name:'Log in',exact:true}).click();await page.waitForURL(fixture.url('/'));
 assert.match(await page.evaluate(()=>localStorage.getItem('lific_token')),/^lific_sess_/);assert.ok((await page.context().cookies()).some(c=>c.name==='lific_token'&&c.httpOnly));
 }finally{try{assert.deepEqual(page._portErrors||[],[]);}finally{await page.context().close();}}
});
test('smoke: every seeded main route renders content without boundary, console or page errors',async t=>{
 await create(`/issues/${fixture.issue.id}/comments`,{content:'First smoke comment'});
 const routes=[['/',['Acceptance project']],['/ACC/overview',['Acceptance project']],['/ACC/issues',['Acceptance issue']],
 [`/ACC/issues/${fixture.issue.identifier}`,['Acceptance issue','First smoke comment']],[`/ACC/pages/${fixture.page.id}`,['Acceptance page']],
 ['/ACC/board',['Acceptance issue']],['/ACC/graph',[]],['/ACC/files',[]],['/settings',['Settings']]];
 for(const [route,content]of routes)await t.test(route,async()=>{const page=await fixture.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});try{
 await page.goto(fixture.url(route));await page.waitForLoadState('networkidle');const text=(await page.locator('body').innerText())+' '+(await page.locator('input:visible').evaluateAll(nodes=>nodes.map(n=>n.value).join(' ')));assert.ok(text.trim());assert.ok(!text.includes('Something went wrong'));for(const expected of content)assert.ok(text.includes(expected),`${route} missing ${expected}`);assert.deepEqual(errors,[]);
 }finally{try{assert.deepEqual(page._portErrors||[],[]);}finally{await page.context().close();}}});
});
test('smoke: deep-link Back synthesizes parent and renders the seeded issue (LIF-434)',async()=>{
 const page=await open(`/ACC/issues/${fixture.issue.identifier}`);try{await page.locator('[data-topcoat-issue-detail]').waitFor();await page.goBack();await expect(page).toHaveURL(/\/app\/ACC\/(issues|board)$/,{timeout:3000});await expect(page.locator('body')).toContainText('Acceptance issue');}finally{try{assert.deepEqual(page._portErrors||[],[]);}finally{await page.context().close();}}
});
test('issue waits: add a person with note through detail UI (LIF-485)',async()=>{
 const {value,page,form}=await waitIssue();try{await addPerson(form);await expect.poll(async()=> (await rows(value.id)).length).toBe(1);const item=page.locator('[data-wait-list] li').filter({hasText:'Decide the scope'});await expect(item).toContainText(`@${fixture.credentials.identity}`);await expect(item).toContainText('Decide the scope');}finally{try{assert.deepEqual(page._portErrors||[],[]);}finally{await page.context().close();}}
});
test('issue waits: add expired date window and show overdue headline and note (LIF-485)',async()=>{
 const {value,page,form}=await waitIssue();try{await addDates(form);await expect.poll(async()=> (await rows(value.id)).length).toBe(1);assert.equal((await rows(value.id))[0].state,'overdue');const item=page.locator('[data-wait-list] li').filter({hasText:'State filing office'});await expect(item).toContainText('Overdue since');await expect(item).toContainText('State filing office');}finally{try{assert.deepEqual(page._portErrors||[],[]);}finally{await page.context().close();}}
});
for(const layout of ['issues','board'])test(`issue waits: ${layout} indicator names both waits and distinguishes overdue (LIF-485)`,async()=>{
 const {value,page,form}=await waitIssue();try{await addPerson(form);await expect.poll(async()=> (await rows(value.id)).length).toBe(1);await addDates(form);await expect.poll(async()=> (await rows(value.id)).length).toBe(2);await page.goto(fixture.url(`/ACC/${layout}`));const row=page.locator(`[data-issue-id="${value.id}"]`);await row.waitFor();const chips=row.locator('.tc-issues__wait');await expect(chips).toHaveCount(1);await expect(chips).toContainText(/overdue/i);const label=(await chips.allTextContents()).join(' ');assert.ok(label.includes(`@${fixture.credentials.identity}`),label);assert.ok(label.includes('Overdue since'),label);const accessible=(await chips.evaluateAll(nodes=>nodes.map(n=>n.getAttribute('aria-label')||'').join(' ')));assert.ok(accessible.includes(`@${fixture.credentials.identity}`),accessible);
 }finally{try{assert.deepEqual(page._portErrors||[],[]);}finally{await page.context().close();}}
});
test('issue waits: clearing person leaves date blocker (LIF-485)',async()=>{
 const {value,page,form}=await waitIssue();try{await addPerson(form);await expect.poll(async()=> (await rows(value.id)).length).toBe(1);await addDates(form);await expect.poll(async()=> (await rows(value.id)).length).toBe(2);const waits=await rows(value.id),person=waits.find(w=>w.kind==='user');await page.locator(`[data-wait-id="${person.id}"] [data-wait-clear]`).click();await expect.poll(async()=> (await rows(value.id)).map(w=>w.kind)).toEqual(['date']);await expect(page.locator(`[data-wait-id="${person.id}"]`)).toHaveCount(0);await expect(page.locator('[data-wait-list] li')).toHaveCount(1);
 }finally{try{assert.deepEqual(page._portErrors||[],[]);}finally{await page.context().close();}}
});
test('duplicate relations: forward navigation and reverse link on desktop and mobile',async()=>{
 const source=await issue('Duplicate source'),target=await issue('Canonical target');await create('/issues/link',{source:source.identifier,target:target.identifier,relation_type:'duplicate'});
 const page=await open(`/ACC/issues/${source.identifier}`);try{const forward=page.locator('[data-relation-list] li').filter({hasText:'Duplicate of'});await forward.getByRole('link',{name:target.identifier,exact:true}).click();await expect(page).toHaveURL(new RegExp(`${target.identifier}$`));const reverse=page.locator('[data-relation-list] li').filter({hasText:'Duplicated by'});await expect(reverse.getByRole('link',{name:source.identifier,exact:true})).toBeVisible();await page.setViewportSize({width:390,height:844});await expect(reverse.getByRole('link',{name:source.identifier,exact:true})).toBeVisible();}finally{try{assert.deepEqual(page._portErrors||[],[]);}finally{await page.context().close();}}
});
