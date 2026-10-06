// Paired browser proof against unmodified Main, using the same production database.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {tmpdir} = require('node:os');
const {pathToFileURL} = require('node:url');
const {execFileSync} = require('node:child_process');
const {launchBrowser, mountedProxy, settleScroll} = require('../browser_fixture.cjs');
const {installOriginalFonts} = require('../original_fonts_fixture.cjs');
const {prepareOriginalVite, closeOriginalVite} = require('../original_vite_fixture.cjs');
const [origin, token, snapshot, freshOrigin, closedOrigin, openOriginsJson] = process.argv.slice(2);
const pinned = '9683d38af8e1e6f9b076439fe90d9519109b2218';
const output = path.join(tmpdir(), 'lific-native-signup-browser');

async function session(browser, url, viewport, theme, original, title = 'Create your account.', backend = origin, systemColor = theme) {
  const context = await browser.newContext({viewport, colorScheme:systemColor, reducedMotion:'reduce', locale:'en-US',
    isMobile:viewport.width < 500, hasTouch:viewport.width < 500});
  if (original) await installOriginalFonts(context);
  await context.addInitScript(theme => {localStorage.setItem('lific_theme',theme); localStorage.setItem('lific_motion','reduced');
    window.signupRenderErrors=[]; document.addEventListener('topcoat:render-error',event=>window.signupRenderErrors.push(String(event.detail?.error)),true);
  }, theme);
  const page = await context.newPage(); page.setDefaultTimeout(15000);
  if (original && backend !== origin) await page.route('**/api/instance',async route=>{
    const response=await route.fetch({url:backend+'/api/instance'});await route.fulfill({response});
  });
  const errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});
  await page.goto(url);
  await page.getByRole('heading',{name:title,exact:true}).waitFor();
  return {context,page,errors};
}
async function screenshot(page,name) {
  await page.evaluate(()=>document.fonts.ready); await settleScroll(page);
  await page.screenshot({path:path.join(output,name),fullPage:true,animations:'disabled'});
}
async function validation(page) {
  const username=page.getByLabel('Username',{exact:true}), email=page.getByLabel('Email',{exact:true}), secret=page.getByLabel('Password',{exact:true});
  const submit=page.locator('form').getByRole('button',{name:'Create account',exact:true});
  assert.equal(await username.getAttribute('autocomplete'),'username');
  assert.equal(await email.getAttribute('autocomplete'),'email');
  assert.equal(await secret.getAttribute('autocomplete'),'new-password');
  assert.equal(await submit.isDisabled(),true);
  assert.equal(await page.locator('#signup-password-reqs').isVisible(),false);
  await username.fill('!');
  assert.equal(await page.getByText('Use at least 2 letters, numbers, dashes or underscores.',{exact:true}).isVisible(),false);
  await email.focus();
  await page.getByText('Use at least 2 letters, numbers, dashes or underscores.',{exact:true}).waitFor();
  assert.equal(await username.getAttribute('aria-invalid'),'true');
  assert.equal(await username.getAttribute('aria-describedby'),'signup-username-err');
  await email.fill('bad'); await secret.focus();
  await page.getByText('That does not look like an email address.',{exact:true}).waitFor();
  await username.fill(''); await email.focus(); await page.getByText('Pick a username.',{exact:true}).waitFor();
  await email.fill(''); await secret.focus(); await page.getByText('Enter your email.',{exact:true}).waitFor();
  // Dispatch the form submission itself: disabled buttons cannot exercise invalid-field focus.
  await page.locator('form').evaluate(form=>form.requestSubmit());
  await username.evaluate(element=>{if(document.activeElement!==element)throw new Error('Username must be first invalid focus');});
  await username.fill('\uFEFFvalid_name\u00A0');
  await page.locator('form').evaluate(form=>form.requestSubmit());
  await email.evaluate(element=>{if(document.activeElement!==element)throw new Error('Email must be second invalid focus');});
  await email.fill('valid@example.com');
  await page.locator('form').evaluate(form=>form.requestSubmit());
  await secret.evaluate(element=>{if(document.activeElement!==element)throw new Error('Password must be final invalid focus');});
  const strengths=[];
  for(const [password,label,enabled] of [['abc','Weak',false],['___','Weak',false],['abcdefgh','Weak',true],['Abcdefgh','Fair',true],['Abcdefg1','Strong',true],['________','Weak',true],['😀😀😀😀','Fair',true],['Ää______','Fair',true],['Ab1','Fair',false]]) {
    await secret.fill(password);
    const requirements=page.locator('#signup-password-reqs');
    await requirements.getByText(label,{exact:true}).waitFor();
    assert.equal(await submit.isDisabled(),!enabled);
    await requirements.evaluate(async element=>{
      // Register the browser's actual color transitions before awaiting their completion.
      for(const child of [element,...element.querySelectorAll('*')])getComputedStyle(child).color;
      await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
      for(let round=0;round<8;round++) {
        const animations=element.getAnimations({subtree:true}).filter(animation=>animation.playState==='running'||animation.pending);
        if(animations.length===0)return;
        await Promise.all(animations.map(animation=>animation.finished.catch(()=>{})));
        await new Promise(resolve=>requestAnimationFrame(resolve));
      }
      throw new Error('Password feedback animations did not settle.');
    });
    const meter=await requirements.locator(':scope > div > div > span').evaluateAll(elements=>elements.map(element=>getComputedStyle(element).backgroundColor));
    assert.equal(meter.length,3,'Strength meter has exactly three segments.');
    strengths.push({password,label,meter});
    for(const hint of ['At least 8 characters','A lowercase and uppercase letter','A number or symbol'])await requirements.getByText(hint,{exact:true}).waitFor();
  }
  await page.getByRole('button',{name:'Show password',exact:true}).click(); assert.equal(await secret.getAttribute('type'),'text');
  assert.equal(await page.getByRole('button',{name:'Hide password',exact:true}).getAttribute('aria-pressed'),'true');
  await page.getByRole('button',{name:'Hide password',exact:true}).click(); assert.equal(await secret.getAttribute('type'),'password');
  await secret.fill(''); await page.locator('#signup-password-reqs').waitFor({state:'hidden'});
  return strengths;
}
async function failureArtifacts(main,native,label,error,report) {
  for(const [side,current]of [['main',main],['native',native]])if(current) {
    await screenshot(current.page,`${label}-${side}-failure.png`).catch(()=>{});
    fs.writeFileSync(path.join(output,`${label}-${side}-failure.html`),await current.page.content().catch(()=>''));
  }
  fs.writeFileSync(path.join(output,`${label}-failure.json`),JSON.stringify({error:error.stack,
    main:{url:main?.page.url(),errors:main?.errors},native:{url:native?.page.url(),errors:native?.errors},report},null,2));
}
async function themeState(page,preference) {
  const button=page.getByRole('button',{name:`Cycle theme, current: ${preference}`,exact:true}).and(page.locator('button:visible'));
  await button.waitFor();
  assert.equal(await button.getAttribute('title'),`Theme: ${preference}`);
  assert.equal(await page.evaluate(()=>localStorage.getItem('lific_theme')),preference==='system'?null:preference);
  await page.locator('main').evaluate(async element=>{
    for(const child of [element,...element.querySelectorAll('*')])getComputedStyle(child).color;
    await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
    const animations=element.getAnimations({subtree:true}).filter(animation=>animation.playState==='running'||animation.pending);
    await Promise.all(animations.map(animation=>animation.finished.catch(()=>{})));
  });
  return page.evaluate(()=>{
    const requirements=document.querySelector('#signup-password-reqs');
    return {background:getComputedStyle(document.querySelector('main')).backgroundColor,
      input:getComputedStyle(document.querySelector('#signup-password')).backgroundColor,
      text:getComputedStyle(document.querySelector('h1')).color,
      strength:Array.from(requirements.querySelectorAll(':scope > div > div > span'),element=>getComputedStyle(element).backgroundColor),
      label:Array.from(requirements.querySelectorAll('span')).filter(element=>element.textContent.trim()==='Strong'&&getComputedStyle(element).display!=='none').map(element=>getComputedStyle(element).color),
      preference:localStorage.getItem('lific_theme')};
  });
}
async function themeCycles(browser,mainOrigin,report) {
  for(const [saved,systemColor]of [['light','dark'],['dark','light']]) {
    const proxy=await mountedProxy(new URL(origin),'');let main,native;
    const name=`theme-saved-${saved}-system-${systemColor}`;
    try {
      const viewport={width:1440,height:900};
      main=await session(browser,`${mainOrigin}/signup`,viewport,saved,true,'Create your account.',origin,systemColor);
      native=await session(browser,`${proxy.origin}/signup`,viewport,saved,false,'Create your account.',origin,systemColor);
      for(const current of [main,native])await current.page.getByLabel('Password',{exact:true}).fill('Abcdefg1');
      const entry={name,states:[]};report.push(entry);saveReport(report);
      let preference=saved;
      // Both opposing saved preferences are observed, then every case exercises light → dark → system → light.
      const targets=saved==='light'?['dark','system','light']:['system','light','dark','system','light'];
      for(const target of [saved,...targets]) {
        if(target!==preference)for(const current of [main,native]) {
          await current.page.getByRole('button',{name:`Cycle theme, current: ${preference}`,exact:true}).and(current.page.locator('button:visible')).click();
        }
        preference=target;
        const expected=await themeState(main.page,preference),actual=await themeState(native.page,preference);
        entry.states.push({preference,expected,actual});saveReport(report);
        assert.deepEqual(actual,expected,`${name}.${preference}: stored preference and actual painted theme`);
      }
      entry.passed=true;saveReport(report);
      assert.deepEqual(main.errors,[]);assert.deepEqual(native.errors,[]);
    }catch(error){await failureArtifacts(main,native,name,error,report);throw error;}
    finally{await main?.context.close();await native?.context.close();await proxy.close();}
  }
}
function saveReport(report) {fs.writeFileSync(path.join(output,'proof.json'),JSON.stringify(report,null,2));}
async function measure(page, title = 'Create your account.', closed = false) {
  await page.evaluate(()=>document.fonts.ready);
  const result={};
  const mascotWrapper=page.locator('main .pointer-events-none.flex.justify-end');
  const mascot=mascotWrapper.locator(':scope > *').first();
  const pairs=[['title',page.getByRole('heading',{name:title,exact:true})],
    ['mascot-wrapper',mascotWrapper],['mascot',mascot]];
  if(closed) {
    const explanation=page.getByText('New accounts on this instance are created by whoever runs it. Ask them to add you, then come back and sign in.',{exact:true});
    pairs.push(['closed-panel',explanation.locator('..')],['closed-explanation',explanation],
      ['closed-submit',page.getByRole('button',{name:'Go to sign in',exact:true})]);
  } else {
    pairs.push(['form',page.locator('form')],['username',page.getByLabel('Username',{exact:true})],
      ['email',page.getByLabel('Email',{exact:true})],['password',page.getByLabel('Password',{exact:true})],
      ['submit',page.locator('form').getByRole('button',{name:'Create account',exact:true})],
      ['getting-started',page.getByText('Getting started',{exact:true})]);
    for(const [index,text,status]of [[0,'Create your account','active'],[1,'Start your first project','todo'],[2,'Connect your AI tools','backlog']]) {
      const label=page.getByText(text,{exact:true}),row=label.locator('..');
      pairs.push([`step-${index}-row`,row],[`step-${index}-title`,label],[`step-${index}-status`,row.getByText(status,{exact:true})]);
    }
  }
  for(const [key,locator]of pairs) {
    result[key]=await locator.evaluate(element=>{const rect=element.getBoundingClientRect(),style=getComputedStyle(element);return {
      x:rect.x,y:rect.y,width:rect.width,height:rect.height,fontSize:style.fontSize,lineHeight:style.lineHeight,
      fontFamily:style.fontFamily,fontWeight:style.fontWeight,color:style.color,background:style.backgroundColor,opacity:style.opacity,
      borderRadius:style.borderRadius,paddingTop:style.paddingTop,paddingRight:style.paddingRight,
      paddingBottom:style.paddingBottom,paddingLeft:style.paddingLeft};});
  }
  result['mascot-mask']=await mascot.evaluate(async element=>{
    const style=getComputedStyle(element),mask=style.maskImage;
    const match=mask.match(/^url\(["']?(.*?)["']?\)$/);
    const src=match?.[1] || (element instanceof HTMLImageElement ? element.src : '');
    if(!src)throw new Error('Auth mascot has no loadable asset.');
    const image=new Image();image.src=src;await image.decode();
    if(image.naturalWidth!==567||image.naturalHeight!==562)throw new Error('Signup mascot must load the actual 567×562 asset.');
    return {masked:mask!=='none',maskPosition:style.maskPosition,maskSize:style.maskSize,
      maskRepeat:style.maskRepeat,ariaHidden:element.getAttribute('aria-hidden'),
      assetLoaded:image.complete&&image.naturalWidth===567&&image.naturalHeight===562};
  });
  return result;
}
function compareMeasurements(actual,expected,name) {
  for(const key of Object.keys(expected))for(const prop of Object.keys(expected[key])) {
    if(typeof expected[key][prop]==='number')assert.ok(Math.abs(actual[key][prop]-expected[key][prop])<=1,
      `${name}.${key}.${prop}: native ${actual[key][prop]}, Main ${expected[key][prop]}`);
    else assert.equal(actual[key][prop],expected[key][prop],`${name}.${key}.${prop}`);
  }
}

test('Signup matches pinned Main across mounts, desktop/mobile and both themes, then creates a real mounted session',async()=>{
  assert.ok(origin&&snapshot,'Production origin and pinned Main web directory are required.');
  for(const file of ['src/routes/Signup.svelte','src/lib/AuthShell.svelte','src/lib/Mascot.svelte'])assert.deepEqual(fs.readFileSync(path.join(snapshot,file)),execFileSync('git',['show',`${pinned}:web/${file}`]));
  fs.mkdirSync(output,{recursive:true});
  const cache=fs.mkdtempSync(path.join(tmpdir(),'lific-signup-vite-'));
  const {createServer}=await import(pathToFileURL(path.join(snapshot,'node_modules/vite/dist/node/index.js')).href);
  const vite=await createServer({root:snapshot,cacheDir:cache,configFile:path.join(snapshot,'vite.config.ts'),logLevel:'silent',server:{host:'127.0.0.1',port:0,strictPort:false,proxy:{'/api':{target:origin,changeOrigin:true},'/public/api':{target:origin,changeOrigin:true}}}});
  const browser=await launchBrowser(),report=[];
  try {
    await vite.listen();await prepareOriginalVite(vite);
    const mainOrigin=`http://127.0.0.1:${vite.httpServer.address().port}`;
    await themeCycles(browser,mainOrigin,report);
    assert.ok(freshOrigin && closedOrigin,'Fresh and closed real fixture origins are required.');
    for (const [state,backend,title] of [['fresh',freshOrigin,'Be the first.'],['closed',closedOrigin,'Signups are closed.']]) {
      for(const prefix of ['', '/app','/app/ACC'])for(const viewport of [{width:1440,height:900},{width:390,height:844}])for(const theme of ['light','dark']) {
        const proxy=await mountedProxy(new URL(backend),prefix);let main,native;
        try {
          main=await session(browser,`${mainOrigin}/signup`,viewport,theme,true,title,backend);
          native=await session(browser,`${proxy.origin}${prefix}/signup`,viewport,theme,false,title,backend);
          const name=`${state}-${prefix.replaceAll('/','_')||'root'}-${viewport.width}-${theme}`;
          await screenshot(main.page,`${name}-main.png`);await screenshot(native.page,`${name}-native.png`);
          const expected=await measure(main.page,title,state==='closed');
          const actual=await measure(native.page,title,state==='closed');
          const entry={name,expected,actual};report.push(entry);saveReport(report);
          for(const current of [main,native]) {
            const instanceName=current.page.getByText('Signup fixture',{exact:true});
            await instanceName.waitFor({state:'attached'});
            assert.equal(await instanceName.isVisible(),viewport.width>=640,'Instance title follows Main sm visibility breakpoint.');
            await current.page.getByText('Welcome to our shared workspace.',{exact:true}).waitFor();
            assert.equal(await current.page.locator('form').count(),state==='closed'?0:1);
            assert.equal(await current.page.getByText('Getting started',{exact:true}).count(),state==='closed'?0:1);
            if(state==='closed')await current.page.getByRole('button',{name:'Go to sign in',exact:true}).waitFor();
          }
          compareMeasurements(actual,expected,name);
          if(state==='closed') {
            await native.page.getByRole('button',{name:'Go to sign in',exact:true}).click();
            await native.page.waitForURL(`${proxy.origin}${prefix}/login`);
          }
          assert.deepEqual(native.errors,[]);assert.deepEqual(main.errors,[]);
          entry.passed=true;saveReport(report);
        } catch(error) {await failureArtifacts(main,native,`${state}-${prefix.replaceAll('/','_')||'root'}-${viewport.width}-${theme}`,error,report);throw error;
        } finally {await main?.context.close();await native?.context.close();await proxy.close();}
      }
    }
    const openOrigins=JSON.parse(openOriginsJson);
    assert.equal(openOrigins.length,12,'Each case has an independent real production limiter.');
    let caseIndex=0;
    for(const prefix of ['', '/app','/app/ACC'])for(const viewport of [{width:1440,height:900},{width:390,height:844}])for(const theme of ['light','dark']) {
      const backend=openOrigins[caseIndex];
      const proxy=await mountedProxy(new URL(backend),prefix);
      let main,native;
      try {
        main=await session(browser,`${mainOrigin}/signup`,viewport,theme,true,'Create your account.',backend);
        native=await session(browser,`${proxy.origin}${prefix}/signup`,viewport,theme,false);
        const name=`${prefix.replaceAll('/','_')||'root'}-${viewport.width}-${theme}`;
        await screenshot(main.page,`${name}-main.png`);await screenshot(native.page,`${name}-native.png`);
        const expected=await measure(main.page),actual=await measure(native.page);
        const entry={name,expected,actual};report.push(entry);saveReport(report);
        for(const current of [main,native]) {
          const instanceName=current.page.getByText('Signup fixture',{exact:true});
          await instanceName.waitFor({state:'attached'});
          assert.equal(await instanceName.isVisible(),viewport.width>=640,'Instance title follows Main sm visibility breakpoint.');
        }
        compareMeasurements(actual,expected,name);
        const mainFeedback=await validation(main.page),nativeFeedback=await validation(native.page);
        Object.assign(entry,{mainFeedback,nativeFeedback});saveReport(report);
        assert.deepEqual(nativeFeedback,mainFeedback,`${name}: exact password meter colors and segment count`);
        const page=native.page,requests=[],bodies=[];
        page.on('request',request=>requests.push({method:request.method(),url:request.url()}));
        await page.route(`${proxy.origin}${prefix}/__native_signup/sign_up`,async route=>{
          const response=await route.fetch();const body=await response.text();bodies.push(body);
          await route.fulfill({response,body});
        });
        await page.getByLabel('Username',{exact:true}).fill('existing');
        await page.getByLabel('Email',{exact:true}).fill(`duplicate${caseIndex}@example.com`);
        await page.getByLabel('Password',{exact:true}).fill('securepass123');
        assert.ok(!(await page.content()).includes('securepass123'),'Typed password is absent from serialized markup.');
        assert.ok(!page.url().includes('securepass123'));
        assert.ok((await native.context.cookies()).every(cookie=>!cookie.value.includes('securepass123')));
        await page.locator('form').getByRole('button',{name:'Create account',exact:true}).click();
        await page.locator('form').getByRole('alert').waitFor();
        assert.equal((await page.locator('form').getByRole('alert').textContent()).trim(),'an account with this username or email already exists');
        assert.equal((await native.context.cookies()).some(cookie=>cookie.name==='lific_token'),false);
        await page.locator('form').getByRole('button',{name:'Create account',exact:true}).waitFor();
        await page.getByLabel('Username',{exact:true}).fill(`browser_${caseIndex++}`);
        const started=Date.now();
        await page.locator('form').getByRole('button',{name:'Create account',exact:true}).click();
        const welcome=page.getByRole('button',{name:'Welcome aboard…',exact:true});await welcome.waitFor();
        assert.equal(await welcome.isDisabled(),true);
        await page.getByText('done',{exact:true}).waitFor();
        assert.equal(new URL(page.url()).pathname,`${prefix}/signup`);
        const successSeen=Date.now();
        await page.waitForURL(`${proxy.origin}${prefix}/`);
        assert.ok(Date.now()-successSeen>=550,'Welcome state stays mounted for the 750ms success delay.');
        assert.ok(Date.now()-started<15000);
        await page.locator('#native-home-greeting').waitFor();
        const cookie=(await native.context.cookies()).find(cookie=>cookie.name==='lific_token');
        assert.ok(cookie?.value);assert.equal(cookie.httpOnly,true);assert.equal(cookie.sameSite,'Lax');
        assert.equal(await page.evaluate(()=>document.cookie.includes('lific_token')),false);
        assert.equal(await page.evaluate(()=>localStorage.getItem('lific_token')),null);
        assert.equal(bodies.length,2,'Only duplicate and successful native procedure requests occur.');
        for(const body of bodies){assert.ok(!body.includes('securepass123'));assert.ok(!body.includes(cookie.value));}
        assert.deepEqual(requests.filter(request=>/(?:^|\/)api\//.test(new URL(request.url).pathname)),[],'No frontend REST transport.');
        await page.goto(`${proxy.origin}${prefix}/signup`);await page.waitForURL(`${proxy.origin}${prefix}/`);
        assert.deepEqual(native.errors,[]);assert.deepEqual(main.errors,[]);
        assert.deepEqual(await page.evaluate(()=>window.signupRenderErrors),[]);
        entry.passed=true;saveReport(report);
      } catch(error) {await failureArtifacts(main,native,`${prefix.replaceAll('/','_')||'root'}-${viewport.width}-${theme}`,error,report);throw error;
      } finally {await main?.context.close();await native?.context.close();await proxy.close();}
    }
    fs.writeFileSync(path.join(output,'proof.json'),JSON.stringify(report,null,2));
  } finally {await browser.close();await closeOriginalVite(vite);fs.rmSync(cache,{recursive:true,force:true});}
});
