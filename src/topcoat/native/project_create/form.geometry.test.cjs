// Paired actual ProjectNew documents against unchanged master 9683d38.
// Both use one disposable production database; no application response/CSS stubs.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {tmpdir} = require('node:os');
const {originalHead: referenceHead, assertOriginalSources} = require('../original_source_fixture.cjs');
const {pathToFileURL} = require('node:url');
const {installOriginalFonts, captureOriginalFonts} = require('../original_fonts_fixture.cjs');
const {prepareOriginalVite, closeOriginalVite} = require('../original_vite_fixture.cjs');
const {mountedProxy, launchBrowser} = require(process.argv[4]);
const [origin, token, , snapshot, mode] = process.argv.slice(2);
const pendingConfirmation = mode === 'reauth';
const upstream = new URL(origin);
const output = process.env.LIFIC_PROJECT_FORM_VISUAL_OUTPUT || path.join(tmpdir(), pendingConfirmation ? 'lific-native-project-form-reauth-visual' : 'lific-native-project-form-visual');

function measure(element, confirmation = false) {
  const rect=element.getBoundingClientRect(), style=getComputedStyle(element);
  const base={x:rect.x,y:rect.y,width:rect.width,height:rect.height,
    fontFamily:style.fontFamily,fontSize:style.fontSize,fontWeight:style.fontWeight,lineHeight:style.lineHeight,
    color:style.color,backgroundColor:style.backgroundColor,borderColor:style.borderColor,borderRadius:style.borderRadius,
    paddingTop:style.paddingTop,paddingRight:style.paddingRight,paddingBottom:style.paddingBottom,paddingLeft:style.paddingLeft};
  return confirmation ? {...base,boxShadow:style.boxShadow,outlineColor:style.outlineColor,
    outlineStyle:style.outlineStyle,outlineWidth:style.outlineWidth,marginTop:style.marginTop,
    marginBottom:style.marginBottom,gap:style.gap} : base;
}
// Tailwind ring layers include fully transparent shadows that paint nothing.
// Keep raw styles in evidence and compare every visible shadow without tolerance.
function paintedShadow(value) {
  return value.split(/,(?![^()]*\))/).map(part=>part.trim())
    .filter(part=>!/^rgba\(\s*\d+,\s*\d+,\s*\d+,\s*0(?:\.0+)?\)\s/.test(part)).join(', ');
}
function equivalent(actual, expected, label) {
  for (const key of ['x','y','width','height']) assert.ok(Math.abs(actual[key]-expected[key])<=1,
    `${label}.${key}: native ${actual[key]}, pinned original ${expected[key]}`);
  for (const key of Object.keys(expected).filter(key=>!['x','y','width','height'].includes(key))) {
    assert.equal(key==='boxShadow'?paintedShadow(actual[key]):actual[key],
      key==='boxShadow'?paintedShadow(expected[key]):expected[key],`${label}.${key}`);
  }
}
function selectors(page, native) {
  const form=page.locator('#project-name').locator('..').locator('..');
  const lead=native?page.locator('#native-project-lead-trigger'):form.locator('label').filter({hasText:/^Lead$/}).locator('..').getByRole('button').first();
  const group=native?page.locator('#native-project-group-trigger'):form.locator('label').filter({hasText:/^Group$/}).locator('..').getByRole('button').first();
  const icon=native?page.locator('#native-project-icon-trigger'):form.getByTitle('Choose icon',{exact:true});
  const create=page.getByRole('button',{name:'Create project',exact:true});
  const back=native?page.getByRole('link',{name:/^(?:← )?Back$/}):page.getByRole('button',{name:'Back',exact:true});
  const cancel=native?page.locator('.native-project-create__actions').getByRole('link',{name:'Cancel',exact:true}):page.getByRole('button',{name:'Cancel',exact:true});
  return {form,lead,group,icon,create,back,cancel,
    name:page.locator('#project-name'),identifier:page.locator('#project-id'),description:page.locator('#project-desc'),
    preview:native?page.locator('.native-project-create__preview'):form.locator('span').filter({hasText:/^PRJ-1$/}),
    topbar:native?page.locator('.native-project-create__topbar'):back.locator('..').locator('..'),
    nameLabel:form.locator('label').filter({hasText:/^Name$/}),
    identifierLabel:form.locator('label').filter({hasText:/^Identifier$/}),
    leadLabel:form.locator('label').filter({hasText:/^Lead$/}),
    iconLabel:form.locator('label').filter({hasText:/^Icon$/}),
    descriptionLabel:form.locator('label').filter({hasText:/^Description(?:\s*optional)?$/}),
  };
}
async function contextFor(browser, url, viewport, theme, native) {
  const context=await browser.newContext({viewport,colorScheme:theme,reducedMotion:'reduce',locale:'en-US',timezoneId:'America/Denver',
    isMobile:viewport.width===360,hasTouch:viewport.width===360});
  if (!native) await installOriginalFonts(context);
  await context.addCookies([{name:'lific_token',value:token,url,httpOnly:true,sameSite:'Lax'}]);
  await context.addInitScript(({theme,token,native})=>{
    localStorage.setItem('lific_theme',theme);localStorage.setItem('lific_motion','reduced');
    if (!native) localStorage.setItem('lific_token',token);
  },{theme,token,native});
  const page=await context.newPage();page.setDefaultTimeout(15000);
  return {context,page};
}

test('native ProjectNew matches actual pinned master form and controls on desktop and mobile in both themes',async t=>{
  assert.ok(snapshot,'Pinned master web directory is mandatory.');
  const referenceFiles=['src/routes/ProjectNew.svelte','src/lib/ProjectForm.svelte','src/lib/Select.svelte','src/lib/IconPicker.svelte'];
  assertOriginalSources(snapshot, referenceFiles);
  fs.mkdirSync(output,{recursive:true});
  const browser=await launchBrowser();let vite,cache;
  const sockets=new Set(), report=[];
  try {
    const {createServer}=await import(pathToFileURL(path.join(snapshot,'node_modules/vite/dist/node/index.js')).href);
    cache=fs.mkdtempSync(path.join(tmpdir(),'lific-project-form-vite-'));
    const configure=proxy=>proxy.on('open',socket=>{sockets.add(socket);socket.once('close',()=>sockets.delete(socket));});
    vite=await createServer({cacheDir:cache,root:snapshot,logLevel:'silent',configFile:path.join(snapshot,'vite.config.ts'),
      server:{host:'127.0.0.1',port:0,strictPort:false,proxy:{
        '/api':{target:upstream.origin,ws:true,changeOrigin:false,configure},
        '/public/api':{target:upstream.origin,ws:true,changeOrigin:false,configure},
      }}});
    await vite.listen();await prepareOriginalVite(vite);
    const originalOrigin=`http://127.0.0.1:${vite.httpServer.address().port}`;
    for (const prefix of ['', '/app', '/ACC']) for (const viewport of [{width:1000,height:760},{width:360,height:740}]) for (const theme of ['light','dark']) {
      await t.test(`${prefix||'root'} ${viewport.width} ${theme}`,async()=>{
        const proxy=await mountedProxy(upstream,prefix), errors=[];
        const name=`project-new-${prefix.slice(1)||'root'}-${viewport.width===360?'mobile':'desktop'}-${theme}${pendingConfirmation?'-reauth':''}`;
        let original,native;
        const evidence={name,referenceHead,viewport,theme,phases:{},errors,refusals:[],consoleErrors:[]};report.push(evidence);
        try {
          original=await contextFor(browser,originalOrigin,viewport,theme,false);
          native=await contextFor(browser,proxy.origin,viewport,theme,true);
          for (const [kind,session] of [['svelte',original],['topcoat',native]]) {
            session.page.on('pageerror',error=>errors.push({kind,message:error.message}));
            session.page.on('console',message=>{if(message.type()==='error'){const error={kind,message:message.text(),url:message.location().url};errors.push(error);evidence.consoleErrors.push(error);}});
          }
          // Only an explicitly performed real refusal can credit its browser resource error.
          const performRefusal=async(kind,session,action,path,status,body)=>{
            if(kind!=='svelte'){await action();return;}
            const pending=session.page.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname===path);
            await action();const response=await pending;
            assert.equal(response.status(),status,`Expected real refusal ${path}`);
            assert.deepEqual(await response.json(),body,`Exact real refusal body ${path}`);
            evidence.refusals.push({kind,url:response.url(),status,body});
          };
          const roster=original.page.waitForResponse(response=>new URL(response.url()).pathname==='/api/users' && response.ok());
          const groups=original.page.waitForResponse(response=>new URL(response.url()).pathname==='/api/project-groups' && response.ok());
          await original.page.goto(`${originalOrigin}/#/projects/new`);
          assert.equal((await native.page.goto(`${proxy.origin}${prefix}/projects/new`)).status(),200);
          await Promise.all([roster,groups]);
          const sides=[['svelte',original,selectors(original.page,false)],['topcoat',native,selectors(native.page,true)]];
          for (const [kind,session,controls] of sides) {
            await controls.name.waitFor();await session.page.evaluate(()=>document.fonts.ready);
            await controls.name.fill('Project visual form');await controls.identifier.fill('PRJ');
            await controls.description.fill('Keep the typed description.\nA second line for layout.');
            await controls.create.waitFor({state:'visible'});
            if(kind==='svelte')await captureOriginalFonts(session.page,path.join(output,`${name}-fonts.json`));
          }
          const capture=async(phase,targets,confirmation=false)=>{
            evidence.phases[phase]={};
            for(const [kind,session,controls] of sides){
              await session.page.evaluate(async()=>{
                await Promise.all(document.getAnimations().filter(animation=>
                  Number.isFinite(animation.effect.getComputedTiming().endTime))
                  .map(animation=>animation.finished.catch(()=>{})));
              });
              await session.page.screenshot({path:path.join(output,`${name}-${phase}-${kind}.png`),fullPage:true});
              const measured={};for(const [key,locator]of Object.entries(targets?targets(kind,session,controls):controls))measured[key]=await locator.evaluate(measure,confirmation);
              evidence.phases[phase][kind]=measured;
            }
            fs.writeFileSync(path.join(output,`${name}-geometry.json`),JSON.stringify(evidence,null,2));
          };
          // Capture each actual state before comparing; failed requirements keep paired artifacts.
          await capture('filled');
          for(const [kind,session,controls]of sides)await controls.lead.click();
          const menu=kind=>kind==='svelte'?sides[0][2].lead.locator('..').locator('div.fixed'):native.page.locator('#native-project-lead-menu');
          for(const [kind]of sides)await menu(kind).waitFor({state:'visible'});
          await capture('lead-menu',(kind)=>({menu:menu(kind)}));
          evidence.leadOptions={};
          for(const [kind]of sides)evidence.leadOptions[kind]=await menu(kind).getByRole(kind==='svelte'?'button':'option').allTextContents();
          for(const [kind]of sides)await menu(kind).getByRole(kind==='svelte'?'button':'option').nth(1).click();
          await capture('lead-selected',(kind,session,controls)=>({lead:controls.lead}));
          for(const [kind,session,controls]of sides)await controls.icon.click();
          const panel=kind=>kind==='svelte'?original.page.getByPlaceholder(/^Search (?:1,900\+ icons|emojis)\.\.\.$/).locator('..').locator('..'):native.page.locator('.native-project-picker__panel');
          for(const [kind]of sides)await panel(kind).locator('button[title]').first().waitFor();
          await capture('icon-picker',(kind)=>({panel:panel(kind),firstIcon:panel(kind).locator('button[title]').first(),search:panel(kind).locator('input'),iconsTab:panel(kind).getByRole('button',{name:'Icons',exact:true})}));
          for(const [kind,session,controls]of sides){
            await panel(kind).locator('input').fill('logo');
            await panel(kind).getByRole('button',{name:'Emoji',exact:true}).click();
            await panel(kind).getByTitle('lific logo',{exact:true}).click();
            await controls.icon.locator('svg,img').first().waitFor();
          }
          await capture('icon-selected',(kind,session,controls)=>({icon:controls.icon}));
          if(pendingConfirmation){
            const confirmationTargets=(kind,session)=>{
              const password=session.page.getByPlaceholder('Current password',{exact:true});
              const card=password.locator('..'),copy=card.locator('div').first();
              return {wrap:card.locator('..'),card,copy,lock:copy.locator('svg'),message:copy.locator('p'),password,
                verify:card.getByRole('button',{name:'Verify and create',exact:true}),cancel:card.getByRole('button',{name:'Cancel',exact:true})};
            };
            for(const [kind,session,controls]of sides){
              await performRefusal(kind,session,()=>controls.create.click(),'/api/projects',403,{error:'recent authentication required'});
              const targets=confirmationTargets(kind,session);
              await targets.password.waitFor({state:'visible'});
              assert.ok(await targets.verify.isDisabled());assert.ok(await controls.create.isDisabled());
              // Compare actual user-focused password controls on both implementations.
              await targets.password.focus();await session.page.evaluate(()=>document.fonts.ready);
            }
            await capture('reauth-empty',confirmationTargets,true);
            for(const [kind,session]of sides)await confirmationTargets(kind,session).password.fill('wrong password');
            await capture('reauth-password',confirmationTargets,true);
            // Four actual failures fit the unchanged production 5-attempt budget.
            // Capture error wrapping in both dimensions/themes without exhausting one user's limiter.
            if((prefix===''&&viewport.width===1000&&theme==='light')||(prefix==='/ACC'&&viewport.width===360&&theme==='dark')){
              for(const [kind,session]of sides){
                const targets=confirmationTargets(kind,session);
                await performRefusal(kind,session,()=>targets.password.press('Enter'),'/api/auth/me/refresh',400,{error:'incorrect password'});
                await targets.card.getByRole('alert').waitFor({state:'visible'});
                assert.equal(await targets.password.inputValue(),'','A real rejected password is cleared.');
                assert.ok(await targets.verify.isDisabled());
              }
              await capture('reauth-rejected',(kind,session)=>({...confirmationTargets(kind,session),feedback:confirmationTargets(kind,session).card.getByRole('alert')}),true);
            }
            for(const [kind,session,controls]of sides){
              const targets=confirmationTargets(kind,session);
              await controls.name.fill('Editable after pending cancellation');
              await targets.password.fill('Discard this credential');
              await targets.cancel.click();await targets.password.waitFor({state:'hidden'});
              assert.equal(await controls.name.inputValue(),'Editable after pending cancellation');
              assert.ok(await controls.create.isEnabled());
              // Start a fresh real prompt to prove cancellation discarded the previous password.
              await performRefusal(kind,session,()=>controls.create.click(),'/api/projects',403,{error:'recent authentication required'});await targets.password.waitFor({state:'visible'});
              assert.equal(await targets.password.inputValue(),'');assert.ok(await targets.verify.isDisabled());
              await targets.password.focus();
            }
            await capture('reauth-redraft',confirmationTargets,true);
            for(const [kind,session]of sides){
              const targets=confirmationTargets(kind,session);await targets.cancel.click();await targets.password.waitFor({state:'hidden'});
            }
          }
          fs.writeFileSync(path.join(output,`${name}-geometry.json`),JSON.stringify(evidence,null,2));
          // Strict paired dimensions and computed styling; no pixel threshold can hide missing controls.
          for(const phase of Object.keys(evidence.phases))for(const key of Object.keys(evidence.phases[phase].svelte))
            equivalent(evidence.phases[phase].topcoat[key],evidence.phases[phase].svelte[key],`${name}.${phase}.${key}`);
          const normalize=rows=>rows.map(row=>row.replace(/[✓\s]+/g,' ').trim());
          assert.deepEqual(normalize(evidence.leadOptions.topcoat),normalize(evidence.leadOptions.svelte),'Same real lead roster, initials, admin badges and dates.');
          assert.ok(await sides[0][2].create.isEnabled());assert.ok(await sides[1][2].create.isEnabled());
          for(const [,session]of sides)assert.ok(await session.page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'No horizontal document overflow.');
          for(const refusal of evidence.refusals){
            const reason=refusal.status===403?'Forbidden':'Bad Request';
            const message=`Failed to load resource: the server responded with a status of ${refusal.status} (${reason})`;
            const index=errors.findIndex(error=>error.kind===refusal.kind&&error.url===refusal.url&&error.message===message);
            assert.ok(index>=0,`Verified refusal has its matching browser console error: ${refusal.url}`);
            errors.splice(index,1);
          }
          assert.equal(evidence.refusals.length,pendingConfirmation?(2+((prefix===''&&viewport.width===1000&&theme==='light')||(prefix==='/ACC'&&viewport.width===360&&theme==='dark')?1:0)):0,'Every expected refusal is accounted for, and ordinary visual mode expects none.');
          assert.deepEqual(errors,[]);
        }finally{
          for(const [kind,session]of [['svelte',original],['topcoat',native]])if(session){
            await session.page.screenshot({path:path.join(output,`${name}-final-${kind}.png`),fullPage:true,timeout:3000}).catch(()=>{});
            await session.context.close();
          }
          fs.writeFileSync(path.join(output,`${name}-geometry.json`),JSON.stringify(evidence,null,2));
          await proxy.close();
        }
      });
    }
  }finally{
    fs.writeFileSync(path.join(output,'geometry.json'),JSON.stringify(report,null,2));
    for(const socket of sockets)socket.destroy();if(vite)await closeOriginalVite(vite);
    if(cache)fs.rmSync(cache,{recursive:true,force:true});await browser.close();
  }
});
