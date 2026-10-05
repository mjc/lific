// Actual pinned Svelte and native overview documents use the same disposable DB.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {tmpdir}=require('node:os');
const {execFileSync}=require('node:child_process');
const {pathToFileURL}=require('node:url');
const {installOriginalFonts,captureOriginalFonts}=require('../original_fonts_fixture.cjs');
const {prepareOriginalVite,closeOriginalVite}=require('../original_vite_fixture.cjs');
const {mountedProxy,launchBrowser}=require(process.argv[4]);
const [origin,token,,snapshot]=process.argv.slice(2);
const referenceHead='9683d38af8e1e6f9b076439fe90d9519109b2218';
const output=process.env.LIFIC_PROJECT_OVERVIEW_VISUAL_OUTPUT||path.join(tmpdir(),'lific-native-project-overview-visual');
function measure(element){
  const rect=element.getBoundingClientRect(),style=getComputedStyle(element);
  // Undo actual ancestor scrolling, retaining the complete page/header/section offsets.
  let sx=0,sy=0;for(let parent=element.parentElement;parent;parent=parent.parentElement){sx+=parent.scrollLeft;sy+=parent.scrollTop;}
  return {x:rect.x+sx,y:rect.y+sy,width:rect.width,height:rect.height,fontFamily:style.fontFamily,fontSize:style.fontSize,
    fontWeight:style.fontWeight,lineHeight:style.lineHeight,color:style.color,backgroundColor:style.backgroundColor,
    borderColor:style.borderColor,borderRadius:style.borderRadius,paddingTop:style.paddingTop,paddingRight:style.paddingRight,
    paddingBottom:style.paddingBottom,paddingLeft:style.paddingLeft,gap:style.gap,letterSpacing:style.letterSpacing,
    boxShadow:style.boxShadow,outlineColor:style.outlineColor,outlineStyle:style.outlineStyle,outlineWidth:style.outlineWidth};
}
function interactionWitness(element){
  const nodes=[];
  for(let node=element;node&&nodes.length<4;node=node.parentElement){
    const rect=node.getBoundingClientRect(),style=getComputedStyle(node);
    const target=document.elementFromPoint(rect.x+rect.width/2,rect.y+rect.height/2);
    nodes.push({tag:node.tagName,classes:node.className,rect:rect.toJSON(),hover:node.matches(':hover'),focus:node.matches(':focus'),focusVisible:node.matches(':focus-visible'),display:style.display,alignItems:style.alignItems,lineHeight:style.lineHeight,fontFamily:style.fontFamily,fontSize:style.fontSize,borderColor:style.borderColor,transition:style.transition,boxShadow:style.boxShadow,centerTarget:target?.outerHTML,centerInside:!!target&&node.contains(target)});
  }
  const ancestors=[];for(let node=element.parentElement;node;node=node.parentElement)if(node.scrollTop||node.scrollLeft)ancestors.push({tag:node.tagName,classes:node.className,top:node.scrollTop,left:node.scrollLeft});
  return {nodes,active:document.activeElement?.outerHTML,ancestors};
}
function paintedShadow(value){return value.split(/,(?![^()]*\))/).map(part=>part.trim()).filter(part=>!/^rgba\(\s*\d+,\s*\d+,\s*\d+,\s*0(?:\.0+)?\)\s/.test(part)).join(', ');}
function equivalent(actual,expected,label){
  for(const key of ['x','y','width','height'])assert.ok(Math.abs(actual[key]-expected[key])<=1,`${label}.${key}: native ${actual[key]}, pinned Svelte ${expected[key]}`);
  for(const key of Object.keys(expected).filter(key=>!['x','y','width','height'].includes(key)))assert.equal(key==='boxShadow'?paintedShadow(actual[key]):actual[key],key==='boxShadow'?paintedShadow(expected[key]):expected[key],`${label}.${key}`);
}
const section=(root,name)=>root.getByRole('heading',{name}).locator('xpath=ancestor::section[1]');
function controls(page,native){
  const column=native?page.locator('.native-overview__column'):page.getByRole('heading',{name:'Needs attention',exact:true}).locator('..').locator('..').locator('..');
  const hero=native?column.locator('.native-overview__hero'):column.locator(':scope > section').first();
  const labels=section(column,/^Labels/),publish=section(column,'Public view'),archive=section(column,'Project archive'),members=section(column,/^Members(?:\s+\d+)?$/),importer=section(column,'Import from GitHub');
  const danger=native?column.locator('.native-overview__danger'):column.getByRole('button',{name:'Danger zone',exact:true}).locator('..');
  const name=hero.getByRole('button',{name:'Visible project',exact:true});
  const description=hero.getByRole('button',{name:/Add a description/});
  const identifier=native?hero.locator('.native-overview__identifier'):hero.getByRole('button',{name:'Copy ACC',exact:true});
  const exportButton=page.getByRole('button',{name:'Export',exact:true});
  const topbar=native?page.locator('.native-overview__topbar'):exportButton.locator('..').locator('..');
  const person=native?members.locator('[id^="native-overview-member-person-"][id$="-trigger"]'):members.getByRole('button',{name:/Choose a person/});
  return {column,hero,name,description,identifier,icon:native?hero.getByRole('button',{name:'Choose icon',exact:true}):hero.getByTitle('Choose icon',{exact:true}),topbar,exportButton,
    attention:section(column,'Needs attention'),group:native?column.locator('.native-overview__group'):column.getByText('Sidebar group',{exact:true}).locator('xpath=ancestor::section[1]'),
    labels,publish,archive,members,importer,danger,person,
    repo:importer.getByPlaceholder('owner/name',{exact:true}),token:importer.getByPlaceholder('ghp_…',{exact:true}),
    preview:importer.getByRole('button',{name:'Preview import',exact:true}),
    publishCheckbox:publish.getByRole('checkbox'),publishAcknowledgement:publish.getByRole('checkbox').locator('..'),archiveCheckbox:archive.getByRole('checkbox'),archiveAcknowledgement:archive.getByRole('checkbox').locator('..'),
    publishButton:publish.getByRole('button',{name:'Publish issues',exact:true}),archiveButton:archive.getByRole('button',{name:'Download project archive',exact:true})};
}
async function settleScroll(page){await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));}
async function session(browser,url,viewport,theme,native){
  const context=await browser.newContext({viewport,colorScheme:theme,reducedMotion:'reduce',locale:'en-US',timezoneId:'America/Denver',isMobile:viewport.width===360,hasTouch:viewport.width===360});
  if(!native)await installOriginalFonts(context);
  await context.addCookies([{name:'lific_token',value:token,url,httpOnly:true,sameSite:'Lax'}]);
  await context.addInitScript(({theme,token,native})=>{localStorage.setItem('lific_theme',theme);localStorage.setItem('lific_motion','reduced');if(!native)localStorage.setItem('lific_token',token);},{theme,token,native});
  const page=await context.newPage();page.setDefaultTimeout(15000);return {context,page};
}
test('real native Overview matches unchanged master identity, sections, drafts and disclosures in all mounts, dimensions and themes',async t=>{
  assert.ok(snapshot,'Pinned master web directory required.');fs.mkdirSync(output,{recursive:true});
  const referenceRoot=fs.existsSync(path.join(path.dirname(snapshot),'.git'))?path.dirname(snapshot):process.cwd();
  for(const file of ['src/routes/ProjectSettings.svelte','src/lib/IconPicker.svelte','src/lib/ProjectIcon.svelte','src/lib/ProgressRing.svelte','src/lib/CopyIdButton.svelte','src/lib/ColorPicker.svelte','src/lib/LabelManager.svelte','src/lib/ProjectMembers.svelte','src/lib/PublishPanel.svelte','src/lib/ArchiveTransferPanel.svelte','src/lib/ImportPanel.svelte','src/lib/Select.svelte'])assert.deepEqual(fs.readFileSync(path.join(snapshot,file)),execFileSync('git',['-C',referenceRoot,'show',`${referenceHead}:web/${file}`]),`Unmodified reference ${file}`);
  const browser=await launchBrowser(),upstream=new URL(origin),sockets=new Set(),report=[];let vite,cache;
  try{
    const {createServer}=await import(pathToFileURL(path.join(snapshot,'node_modules/vite/dist/node/index.js')).href);
    cache=fs.mkdtempSync(path.join(tmpdir(),'lific-overview-vite-'));
    const configure=proxy=>proxy.on('open',socket=>{sockets.add(socket);socket.once('close',()=>sockets.delete(socket));});
    vite=await createServer({cacheDir:cache,root:snapshot,logLevel:'silent',configFile:path.join(snapshot,'vite.config.ts'),server:{host:'127.0.0.1',port:0,strictPort:false,proxy:{'/api':{target:upstream.origin,ws:true,changeOrigin:false,configure},'/public/api':{target:upstream.origin,ws:true,changeOrigin:false,configure}}}});
    await vite.listen();await prepareOriginalVite(vite);const originalOrigin=`http://127.0.0.1:${vite.httpServer.address().port}`;
    for(const prefix of ['', '/app','/ACC'])for(const viewport of [{width:1000,height:760},{width:360,height:740}])for(const theme of ['light','dark'])await t.test(`${prefix||'root'} ${viewport.width} ${theme}`,async()=>{
      const proxy=await mountedProxy(upstream,prefix),name=`overview-${prefix.slice(1)||'root'}-${viewport.width===360?'mobile':'desktop'}-${theme}`,errors=[];
      const evidence={name,referenceHead,viewport,theme,phases:{},errors,projectWrites:[],sectionCaptures:[],sidebarScope:'Shared sidebar remains visible in uncropped viewport captures; its separate parity suite owns sidebar assertions.'};report.push(evidence);
      let original,native;
      try{
        original=await session(browser,originalOrigin,viewport,theme,false);native=await session(browser,proxy.origin,viewport,theme,true);
        const sides=[['svelte',original],['topcoat',native]];
        for(const [kind,item]of sides){item.page.on('request',request=>{const url=new URL(request.url());if((kind==='svelte'&&request.method()==='PATCH'&&/^\/api\/projects\/\d+$/.test(url.pathname))||(kind==='topcoat'&&request.method()==='POST'&&url.pathname.endsWith('/__native_overview/save_field')))evidence.projectWrites.push({kind,method:request.method(),url:request.url(),body:request.postData()});});item.page.on('pageerror',error=>errors.push({kind,message:error.message}));item.page.on('console',message=>{if(message.type()==='error')errors.push({kind,message:message.text(),url:message.location().url});});}
        assert.equal((await original.page.goto(`${originalOrigin}/#/ACC/overview`)).status(),200);
        assert.equal((await native.page.goto(`${proxy.origin}${prefix}/ACC/overview`)).status(),200);
        for(const [kind,item]of sides){evidence.readiness={kind,phase:'construct controls',url:item.page.url(),headings:await item.page.getByRole('heading').allTextContents(),buttons:await item.page.getByRole('button').allTextContents()};item.controls=controls(item.page,kind==='topcoat');const c=item.controls;evidence.readiness.phase='name';await c.name.waitFor();evidence.readiness.phase='archive';await c.archiveButton.waitFor();evidence.readiness.phase='person';await c.person.waitFor();evidence.readiness.phase='fonts';await item.page.evaluate(()=>document.fonts.ready);assert.ok(await c.publishButton.isDisabled());assert.ok(await c.archiveButton.isDisabled());assert.ok(await c.preview.isDisabled());}
        await captureOriginalFonts(original.page,path.join(output,`${name}-fonts.json`));
        const capture=async(phase,targetFactory)=>{
          evidence.phases[phase]={};
          for(const [kind,item]of sides){
            const targets=targetFactory(item.controls,kind,item.page),values={};
            for(const [key,target]of Object.entries(targets)){assert.equal(await target.count(),1,`${kind}.${phase}.${key} owns one real control`);await target.waitFor({state:'visible'});assert.ok(await target.isVisible(),`${kind}.${phase}.${key} visible`);values[key]=await target.evaluate(measure);}
            evidence.phases[phase][kind]=values;
            // Preserve app/sidebar context in every phase. Separately scroll and capture each actual section.
            await item.page.screenshot({path:path.join(output,`${name}-${phase}-${kind}.png`),fullPage:true});
            if(phase==='ready')for(const sectionName of ['hero','attention','group','labels','publish','archive','members','importer','danger']){const target=item.controls[sectionName];await target.scrollIntoViewIfNeeded();await settleScroll(item.page);const filename=`${name}-${phase}-${sectionName}-${kind}.png`;await item.page.screenshot({path:path.join(output,filename),fullPage:true});evidence.sectionCaptures.push({kind,section:sectionName,file:filename});}
          }
        };
        await capture('ready',c=>Object.fromEntries(['column','hero','name','description','identifier','icon','topbar','exportButton','attention','group','labels','publish','archive','members','importer','danger','repo','token','preview','publishButton','archiveButton','person','publishCheckbox','publishAcknowledgement','archiveCheckbox','archiveAcknowledgement'].map(key=>[key,c[key]])));
        evidence.memberLayout={};for(const [kind,item]of sides)evidence.memberLayout[kind]=await item.controls.members.evaluate(section=>{
          const describe=node=>{const rect=node.getBoundingClientRect(),style=getComputedStyle(node);return {tag:node.tagName,classes:node.className,rect:rect.toJSON(),fontFamily:style.fontFamily,fontSize:style.fontSize,lineHeight:style.lineHeight,letterSpacing:style.letterSpacing,marginBottom:style.marginBottom,borderTopWidth:style.borderTopWidth,borderTopColor:style.borderTopColor,previous:node.previousElementSibling?.outerHTML.slice(0,240)};};
          const heading=section.querySelector(':scope > div');
          return {heading:describe(heading),headingChildren:[...heading.children].map(describe),rows:[...section.querySelectorAll('button[aria-label^="Remove "]')].map(button=>describe(button.parentElement))};
        });
        evidence.acknowledgementLayout={};for(const [kind,item]of sides){evidence.acknowledgementLayout[kind]={};for(const key of ['publish','archive'])evidence.acknowledgementLayout[kind][key]=await item.controls[key].getByRole('checkbox').evaluate(input=>{
          const nodes=[input,input.parentElement,input.nextElementSibling].filter(Boolean);
          return nodes.map(node=>{const rect=node.getBoundingClientRect(),style=getComputedStyle(node);return {tag:node.tagName,rect:rect.toJSON(),fontFamily:style.fontFamily,fontSize:style.fontSize,lineHeight:style.lineHeight,color:style.color,marginTop:style.marginTop,marginRight:style.marginRight,marginBottom:style.marginBottom,marginLeft:style.marginLeft,gap:style.gap};});
        });}
        for(const [kind,item]of sides){await item.controls.name.click();const field=kind==='topcoat'?item.controls.hero.getByRole('textbox',{name:'Project name',exact:true}):item.controls.hero.locator('input');await field.fill('Uncommitted visual name');await field.focus();}
        await capture('name-focused',(c,kind)=>({hero:c.hero,input:kind==='topcoat'?c.hero.getByRole('textbox',{name:'Project name',exact:true}):c.hero.locator('input')}));
        for(const [kind,item]of sides){const field=kind==='topcoat'?item.controls.hero.getByRole('textbox',{name:'Project name',exact:true}):item.controls.hero.locator('input');// A visual draft is restored through its real control before Escape: master blur saves even after Escape.
          await field.fill('Visible project');await field.press('Escape');await item.controls.name.waitFor();await item.controls.description.click();const textarea=item.controls.hero.locator('textarea');await textarea.fill('Uncommitted description.\nSecond line retains wrapping.');await textarea.focus();}
        await capture('description-focused',c=>({hero:c.hero,input:c.hero.locator('textarea')}));
        for(const [,item]of sides){await item.controls.hero.locator('textarea').fill('');await item.controls.hero.locator('textarea').press('Escape');await item.controls.description.waitFor();await item.controls.danger.getByRole('button',{name:'Danger zone',exact:true}).click();await item.controls.danger.getByRole('button',{name:'Delete this project',exact:true}).waitFor();}
        await capture('danger-open',(c,kind)=>({danger:c.danger,lead:c.danger.locator('select'),identifier:kind==='topcoat'?c.danger.getByRole('textbox',{name:'New project identifier',exact:true}):c.danger.getByPlaceholder('ACC',{exact:true})}));
        for(const [,item]of sides){await item.controls.danger.getByRole('button',{name:'Delete this project',exact:true}).click();const confirmation=item.controls.danger.getByPlaceholder('ACC',{exact:true}).last();await confirmation.fill('acc');assert.ok(await item.controls.danger.getByRole('button',{name:'Delete permanently',exact:true}).isDisabled());await confirmation.fill('ACC');assert.ok(await item.controls.danger.getByRole('button',{name:'Delete permanently',exact:true}).isEnabled());}
        await capture('delete-confirmed',c=>({danger:c.danger,confirmation:c.danger.getByPlaceholder('ACC',{exact:true}).last(),remove:c.danger.getByRole('button',{name:'Delete permanently',exact:true}),cancel:c.danger.getByRole('button',{name:'Cancel',exact:true})}));
        for(const [,item]of sides){await item.controls.danger.getByRole('button',{name:'Cancel',exact:true}).click();await item.controls.danger.getByRole('button',{name:'Danger zone',exact:true}).click();await item.controls.repo.fill('visual/specimen');await item.controls.token.fill('Unsubmitted credential draft');await item.controls.repo.focus();await item.page.waitForFunction(()=>{const inputs=[...document.querySelectorAll('input')];const repo=inputs.find(input=>input.placeholder==='owner/name');return repo&&[...repo.closest('section').querySelectorAll('button')].some(button=>button.textContent.includes('Preview import')&&!button.disabled);});}
        await capture('import-draft',c=>({importer:c.importer,repo:c.repo,token:c.token,preview:c.preview,issues:c.importer.locator('select').nth(0),open:c.importer.locator('select').nth(1),closed:c.importer.locator('select').nth(2)}));
        for(const [,item]of sides){await item.controls.publish.getByRole('checkbox').check();await item.controls.archive.getByRole('checkbox').check();assert.ok(await item.controls.publishButton.isEnabled());assert.ok(await item.controls.archiveButton.isEnabled());}
        await capture('acknowledged',c=>({publish:c.publish,publishButton:c.publishButton,archive:c.archive,archiveButton:c.archiveButton,publishCheckbox:c.publishCheckbox,publishAcknowledgement:c.publishAcknowledgement,archiveCheckbox:c.archiveCheckbox,archiveAcknowledgement:c.archiveAcknowledgement}));
        for(const [,item]of sides){await item.controls.person.scrollIntoViewIfNeeded();await settleScroll(item.page);await item.controls.person.click();}
        await capture('person-menu',(c,kind)=>({members:c.members,trigger:c.person,menu:kind==='topcoat'?c.members.locator('[id^="native-overview-member-person-"][id$="-menu"]'):c.person.locator('..').locator(':scope > div')}));
        evidence.personInteraction={};for(const [kind,item]of sides)evidence.personInteraction[kind]=await item.controls.person.evaluate(interactionWitness);
        for(const [,item]of sides){const menu=item.controls.person.locator('..').locator(':scope > div');if(await menu.count())assert.ok((await menu.textContent()).includes('non_member'));await item.controls.attention.getByRole('heading',{name:'Needs attention',exact:true}).click();await item.controls.icon.click();}
        await capture('icon-picker',(c,kind,page)=>({hero:c.hero,trigger:c.icon,search:page.getByPlaceholder('Search 1,900+ icons...',{exact:true}),panel:kind==='topcoat'?page.locator('.native-project-picker__panel'):page.getByPlaceholder('Search 1,900+ icons...',{exact:true}).locator('..').locator('..')}));
        evidence.iconInteraction={};for(const [kind,item]of sides)evidence.iconInteraction[kind]=await item.controls.icon.evaluate(interactionWitness);
        for(const [kind,item]of sides){
          if(viewport.width===360){
            const panel=kind==='topcoat'?item.page.locator('.native-project-picker__panel'):item.page.getByPlaceholder('Search 1,900+ icons...',{exact:true}).locator('..').locator('..');
            // Actual mobile receipts show popup x12/y64. The outer header corner is clear.
            // Assert the hit target before a genuine pointer click; never force through the overlay.
            const hit=await panel.evaluate(panel=>{const target=document.elementFromPoint(2,2);return {present:!!target,inside:!!target&&panel.contains(target),interactive:!!target?.closest('a,button,input,textarea,select,[role=button],[contenteditable=true]'),tag:target?.tagName,classes:target?.className};});
            evidence[`${kind}IconDismissHit`]=hit;assert.ok(hit.present);assert.equal(hit.inside,false,'Dismiss pointer is outside the actual icon panel');assert.equal(hit.interactive,false,'Dismiss pointer does not invoke another control');
            await item.page.mouse.click(2,2);
          }else await item.controls.topbar.getByText('Overview',{exact:true}).click();
          await item.page.getByPlaceholder('Search 1,900+ icons...',{exact:true}).waitFor({state:'hidden'});await item.controls.labels.getByRole('button',{name:/^Color:/}).first().click();
        }
        await capture('label-color',(c,kind)=>({labels:c.labels,trigger:c.labels.getByRole('button',{name:/^Color:/}).first(),hex:c.labels.getByPlaceholder('hex',{exact:true}),set:c.labels.getByRole('button',{name:'Set',exact:true})}));
        // All views are captured before strict comparisons, retaining diagnostic screenshots for every discrepancy.
        for(const [phase,views]of Object.entries(evidence.phases))for(const [key,value]of Object.entries(views.svelte))equivalent(views.topcoat[key],value,`${name}.${phase}.${key}`);
        assert.deepEqual(evidence.projectWrites,[],'Pure visual drafts restored through actual controls do not submit a project write. Native changed-draft Escape cancellation is tested independently in production smoke.');
        assert.deepEqual(errors,[]);
        assert.ok(!proxy.requests.some(request=>new URL(request.path,proxy.origin).pathname.split('/').includes('api')),'Native page and drafts use typed production boundaries.');
      }catch(error){evidence.failure={message:error.message,stack:error.stack};for(const [kind,item]of [['svelte',original],['topcoat',native]])if(item){evidence[`${kind}AtFailure`]={url:item.page.url(),headings:await item.page.getByRole('heading').allTextContents(),buttons:await item.page.getByRole('button').allTextContents()};await item.page.screenshot({path:path.join(output,`${name}-failure-${kind}.png`),fullPage:true}).catch(()=>{});}throw error;}finally{fs.writeFileSync(path.join(output,`${name}-geometry.json`),JSON.stringify(evidence,null,2));for(const item of [original,native])if(item)await item.context.close();await proxy.close();}
    });
  }finally{fs.writeFileSync(path.join(output,'geometry.json'),JSON.stringify(report,null,2));for(const socket of sockets)socket.destroy();if(vite)await closeOriginalVite(vite);if(cache)fs.rmSync(cache,{recursive:true,force:true});await browser.close();}
});
