// The original and native views consume the same production database.
// node <driver> <fixture-origin> <token> <browser-helper> <pinned-master-web>
const {test} = require('node:test');
const {installOriginalFonts, captureOriginalFonts} = require('../original_fonts_fixture.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {tmpdir} = require('node:os');
const {mountedProxy, launchBrowser} = require(process.argv[4]);
const upstream = new URL(process.argv[2]), token = process.argv[3], snapshot = process.argv[5];
const fixtureTitle = 'Production issue initial title';
const typographyOnly = process.argv[6] === '--typography-only';
const output = path.join(require('node:os').tmpdir(), typographyOnly
  ? 'lific-native-issue-typography' : 'lific-native-issue-detail');

function measure(element) {
  const rect = element.getBoundingClientRect(), style = getComputedStyle(element);
  return {x:rect.x,y:rect.y,width:rect.width,height:rect.height,
    fontSize:style.fontSize,lineHeight:style.lineHeight,fontWeight:style.fontWeight,
    paddingTop:style.paddingTop,paddingRight:style.paddingRight,
    paddingBottom:style.paddingBottom,paddingLeft:style.paddingLeft,
    borderLeftWidth:style.borderLeftWidth};
}
function close(actual, expected, label) {
  for (const key of ['x','y','width','height']) assert.ok(Math.abs(actual[key]-expected[key])<=1,
    `${label}.${key}: native ${actual[key]}, original ${expected[key]}`);
  for (const key of Object.keys(expected).filter(key=>!['x','y','width','height'].includes(key))) {
    assert.equal(actual[key],expected[key],`${label}.${key}`);
  }
}
async function setup(browser, origin, mode, theme, native) {
  const viewport = mode==='phone'?{width:390,height:844}:{width:1440,height:900};
  const context = await browser.newContext({viewport,isMobile:mode==='phone',hasTouch:mode==='phone',
    colorScheme:theme,locale:'en-US',timezoneId:'America/Denver',reducedMotion:'reduce'});
  if (!native) await installOriginalFonts(context);
  await context.addCookies([{name:'lific_token',value:token,url:origin,httpOnly:true,sameSite:'Lax'}]);
  await context.addInitScript(({theme,token,native})=>{
    localStorage.setItem('lific_theme',theme); localStorage.setItem('lific_motion','reduced');
    if (!native) localStorage.setItem('lific_token',token);
  },{theme,token,native});
  const page = await context.newPage(); page.setDefaultTimeout(15000);
  await page.clock.setFixedTime('2026-10-03T16:00:00Z');
  return {context,page};
}

test('native issue document matches pinned master composition at every mount',async t=>{
  assert.ok(snapshot,'Pinned master web directory is mandatory; no reference fallback.');
  assert.ok(fs.existsSync(path.join(snapshot,'src/routes/IssueDetail.svelte')));
  fs.mkdirSync(output,{recursive:true});
  const browser = await launchBrowser(); let vite, referenceCache;
  const proxySockets = new Set();
  try {
    const {createServer} = await import(pathToFileURL(path.join(snapshot,'node_modules/vite/dist/node/index.js')).href);
    const configure = proxy=>proxy.on('open',socket=>{
      proxySockets.add(socket); socket.once('close',()=>proxySockets.delete(socket));
    });
    referenceCache = fs.mkdtempSync(path.join(tmpdir(), 'lific-pinned-vite-'));
    vite = await createServer({cacheDir:referenceCache,root:snapshot,logLevel:'silent',configFile:path.join(snapshot,'vite.config.ts'),
      server:{host:'127.0.0.1',port:0,strictPort:false,proxy:{
        '/api':{target:upstream.origin,ws:true,changeOrigin:false,configure},
        '/public/api':{target:upstream.origin,ws:true,changeOrigin:false,configure},
      }}});
    await vite.listen(); const originalOrigin = `http://127.0.0.1:${vite.httpServer.address().port}`;
    for (const prefix of ['', '/app', '/ACC']) for (const mode of ['desktop','phone']) for (const theme of ['light','dark']) {
      await t.test(`${prefix||'root'} ${mode} ${theme}`,async()=>{
        const proxy = await mountedProxy(upstream,prefix);
        const name = `${prefix.slice(1)||'root'}-${mode}-${theme}`, errors = [];
        let original, native;
        try {
          original = await setup(browser,originalOrigin,mode,theme,false);
          native = await setup(browser,proxy.origin,mode,theme,true);
          for (const [kind,session] of [['original',original],['native',native]]) {
            session.page.on('pageerror',error=>errors.push({kind,message:error.message}));
            session.page.on('console',message=>{
              if (message.type()==='error') errors.push({kind,message:message.text()});
            });
          }
          await original.page.goto(`${originalOrigin}/#/ACC/issues/ACC-1`);
          assert.equal((await native.page.goto(`${proxy.origin}${prefix}/ACC/issues/ACC-1`)).status(),200);
          for (const session of [original,native]) {
            await session.page.getByRole('button',{name:fixtureTitle,exact:true}).waitFor();
            await session.page.getByRole('heading',{name:'Production markdown',exact:true}).waitFor();
            await session.page.evaluate(()=>document.fonts.ready);
            if (session === original) await captureOriginalFonts(session.page,path.join(output,`${name}-original-fonts.json`));
          }
          // Capture both actual documents before the first parity assertion.
          await original.page.screenshot({path:path.join(output,`${name}-original.png`),fullPage:true});
          await native.page.screenshot({path:path.join(output,`${name}-native.png`),fullPage:true});
          const originalActivity = original.page.locator('section').filter({
            has: original.page.getByRole('heading',{name:'Activity',exact:true}),
          });
          await originalActivity.getByRole('heading',{name:'Activity',exact:true}).waitFor();
          assert.equal(await originalActivity.locator('ol > li').count(),4,
            'Pinned master renders the four real issue audits.');
          const mountSources=await native.page.locator('[data-topcoat-on\\:mount]').evaluateAll(elements=>elements.map(element=>({
            tag:element.tagName, classes:element.className, source:element.getAttribute('data-topcoat-on:mount'),
          })));
          fs.writeFileSync(path.join(output,`${name}-mounts.json`),JSON.stringify(mountSources,null,2));
          const nativeActivity = native.page.locator('[data-native-issue-activity]');
          assert.equal(await nativeActivity.count(),1,
            'Native issue detail renders the Activity panel.');
          assert.equal(await nativeActivity.locator('ol > li[data-activity-id]').count(),4,
            'Native issue detail renders the same four real issue audits.');
          const originalRows=originalActivity.locator('ol > li');
          const nativeRows=nativeActivity.locator('ol > li[data-activity-id]');
          const activityProof={rows:[]};
          const rowContent=locator=>locator.evaluate(element=>({
            text:element.innerText.replace(/\s+/g,' ').trim(),
            datetime:element.querySelector('time').getAttribute('datetime'),
            timestamp:element.querySelector('time').title,
            transport:element.querySelector('time').parentElement.title,
          }));
          for(let index=0;index<4;index++) {
            const reference=originalRows.nth(index).locator('div.text-body-sm');
            const current=nativeRows.nth(index).locator('.native-issue-activity__line');
            activityProof.rows.push({original:await rowContent(reference),native:await rowContent(current)});
          }
          fs.writeFileSync(path.join(output,`${name}-activity.json`),JSON.stringify(activityProof,null,2));
          for(const [index,row]of activityProof.rows.entries())
            assert.deepEqual(row.native,row.original,`Activity row ${index} ordered text, UTC timestamp, and transport tooltip`);
          const typography = locator => locator.evaluate(element => {
            const style=getComputedStyle(element);
            return {fontFamily:style.fontFamily,fontSize:style.fontSize,
              fontWeight:style.fontWeight,letterSpacing:style.letterSpacing};
          });
          const originalCrumbs=original.page.getByRole('navigation',{name:'Breadcrumb',exact:true});
          const nativeCrumbs=native.page.getByRole('navigation',{name:'Breadcrumb',exact:true});
          const pairs=[
            ['Activity heading',originalActivity.getByRole('heading',{name:'Activity',exact:true}),nativeActivity.getByRole('heading',{name:'Activity',exact:true})],
            ['Activity first row',originalRows.first().locator('div.text-body-sm'),nativeRows.first().locator('.native-issue-activity__line')],
            ['current identifier',originalCrumbs.locator('[aria-current="page"]'),nativeCrumbs.locator('[aria-current="page"]')],
            ['current identifier label',originalCrumbs.locator('[aria-current="page"] > span'),nativeCrumbs.locator('[aria-current="page"] > [data-label]')],
            ['issue title',original.page.getByRole('button',{name:fixtureTitle,exact:true}),native.page.getByRole('button',{name:fixtureTitle,exact:true})],
            ['header status label',original.page.getByTitle('Change status',{exact:true}).locator('span').first(),
              native.page.getByTitle('Change status',{exact:true}).locator(':scope > span[data-status]')],
            ['Preview control',original.page.getByRole('radio',{name:'Preview',exact:true}),native.page.getByRole('radio',{name:'Preview',exact:true})],
            ['Export control',original.page.getByRole('button',{name:'Export',exact:true}),native.page.getByRole('button',{name:'Export',exact:true})],
          ];
          if(mode==='desktop'){
            pairs.push(['project identifier',originalCrumbs.getByRole('link',{name:'ACC',exact:true}),nativeCrumbs.getByRole('link',{name:'ACC',exact:true})]);
            pairs.push(['Issues label',originalCrumbs.getByRole('link',{name:'Issues',exact:true}),nativeCrumbs.getByRole('link',{name:'Issues',exact:true})]);
          }
          // Persist every actual computed rule before the first font or geometry
          // assertion. The same platform/browser evaluates both documents.
          const fonts={originalEnvironment:await original.page.evaluate(()=>({userAgent:navigator.userAgent,fonts:document.fonts.status})),
            nativeEnvironment:await native.page.evaluate(()=>({userAgent:navigator.userAgent,fonts:document.fonts.status})),values:{}};
          for(const [label,reference,current]of pairs){
            assert.equal(await reference.count(),1,`Pinned ${label} is uniquely identified.`);
            assert.equal(await current.count(),1,`Native ${label} is uniquely identified.`);
            fonts.values[label]={original:await typography(reference),native:await typography(current)};
          }
          fs.writeFileSync(path.join(output,`${name}-typography.json`),JSON.stringify(fonts,null,2));
          const masterMono=fonts.values['current identifier'].original;
          assert.ok(masterMono.fontFamily.includes('Cascadia Code')&&masterMono.fontFamily.includes('Fira Code'),
            'Pinned font-mono includes both named platform fallbacks.');
          assert.equal(masterMono.fontSize,'13px','Pinned breadcrumb uses text-body-sm.');
          assert.equal(masterMono.fontWeight,'500','Pinned breadcrumb uses font-medium.');
          for(const [label,value]of Object.entries(fonts.values))
            assert.deepEqual(value.native,value.original,`${label} computed font family/size/weight/letter spacing`);
          if (typographyOnly) {
            assert.deepEqual(errors,[], 'Actual documents render without browser errors.');
            return;
          }
          const titleOriginal = original.page.getByRole('button',{name:fixtureTitle,exact:true});
          const titleNative = native.page.getByRole('button',{name:fixtureTitle,exact:true});
          const geometry = async (session,nativeView) => {
            const page = session.page;
            const selectors = nativeView ? {
              title:'.native-issue-editor__title:not([hidden])',content:'.native-issue-editor__content',
              document:'.native-issue-editor',metadata:'.native-issue-editor__fields',
              panel:'.native-home-panel',topbar:'.native-home-topbar',description:'.native-issue-editor__preview.tc-markdown',
            } : {
              title:'main button.text-title',content:'main [class*="sm:px-8"]',
              document:'main [class*="max-w-[1120px]"]',metadata:'main aside',
              panel:'main',description:'main .prose',
            };
            const result = {};
            for (const [label,selector] of Object.entries(selectors)) {
              const locator = page.locator(selector).first();
              result[label] = await locator.count() ? await locator.evaluate(measure) : null;
            }
            const crumb = page.getByRole('navigation',{name:'Breadcrumb',exact:true});
            result.breadcrumb = await crumb.count() ? await crumb.evaluate(measure) : null;
            const mode = page.getByRole('radiogroup',{name:'Content view mode'});
            result.mode = await mode.count() ? await mode.evaluate(measure) : null;
            result.horizontalOverflow = await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth);
            return result;
          };
          const recorded = {original:await geometry(original,false),native:await geometry(native,true)};
          fs.writeFileSync(path.join(output,`${name}.json`),JSON.stringify(recorded,null,2));
          close(await nativeActivity.evaluate(measure),await originalActivity.evaluate(measure),'Activity panel');
          close(await nativeActivity.locator('.native-issue-activity__header').evaluate(measure),
            await originalActivity.locator(':scope > div').first().evaluate(measure),'Activity header');
          for(let index=0;index<4;index++) {
            close(await nativeRows.nth(index).evaluate(measure),await originalRows.nth(index).evaluate(measure),`Activity row ${index}`);
            close(await nativeRows.nth(index).locator('time').evaluate(measure),
              await originalRows.nth(index).locator('time').evaluate(measure),`Activity row ${index} painted timestamp`);
          }
          const originalChange=originalActivity.getByRole('button',{name:'show change',exact:true});
          const nativeChange=nativeActivity.getByRole('button',{name:'show change',exact:true});
          assert.equal(await originalChange.count(),1);
          assert.equal(await nativeChange.count(),1);
          await originalChange.click(); await nativeChange.click();
          const originalValues=originalRows.filter({has:original.page.getByRole('button',{name:'hide change',exact:true})}).locator('div[class*="max-w-[640px]"]');
          const nativeValues=nativeActivity.locator('.native-issue-activity__values');
          await originalValues.waitFor({state:'visible'}); await nativeValues.waitFor({state:'visible'});
          assert.deepEqual(await nativeValues.locator(':scope > div').allTextContents(),
            await originalValues.locator(':scope > div').allTextContents(),'Expanded Activity old and new values');
          const painted=locator=>locator.evaluate(element=>({color:getComputedStyle(element).color,
            background:getComputedStyle(element).backgroundColor,border:getComputedStyle(element).borderColor}));
          for(let index=0;index<2;index++) {
            const current=nativeValues.locator(':scope > div').nth(index);
            const reference=originalValues.locator(':scope > div').nth(index);
            close(await current.evaluate(measure),await reference.evaluate(measure),`Expanded Activity value ${index}`);
            assert.deepEqual(await painted(current),await painted(reference),`Expanded Activity value ${index} colors`);
          }
          await original.page.screenshot({path:path.join(output,`${name}-original-change.png`),fullPage:true});
          await native.page.screenshot({path:path.join(output,`${name}-native-change.png`),fullPage:true});
          await originalActivity.getByRole('button',{name:'hide change',exact:true}).click();
          await nativeActivity.getByRole('button',{name:'hide change',exact:true}).click();
          assert.equal(await nativeValues.isVisible(),false,'Activity change closes again');
          close(recorded.native.title,recorded.original.title,'issue title');
          const crumbOriginal = original.page.getByRole('navigation',{name:'Breadcrumb',exact:true});
          const crumbNative = native.page.getByRole('navigation',{name:'Breadcrumb',exact:true});
          assert.equal(await crumbNative.count(),1,'Issue topbar owns a real Breadcrumb navigation.');
          close(await crumbNative.evaluate(measure),await crumbOriginal.evaluate(measure),'breadcrumb');
          // Svelte's template whitespace does not change the displayed identity.
          assert.equal((await crumbNative.locator('[aria-current="page"]').textContent()).trim(),
            (await crumbOriginal.locator('[aria-current="page"]').textContent()).trim());
          if (mode==='desktop') {
            assert.equal(await crumbNative.getByRole('link',{name:'ACC',exact:true}).getAttribute('href'),`${prefix}/ACC/overview`);
            assert.equal(await crumbNative.getByRole('link',{name:'Issues',exact:true}).getAttribute('href'),`${prefix}/ACC/issues`);
          } else {
            assert.equal(await crumbNative.getByRole('link').count(),0,'Phone hides duplicated project/list scope.');
          }
          close(await native.page.getByRole('radiogroup',{name:'Content view mode'}).evaluate(measure),
            await original.page.getByRole('radiogroup',{name:'Content view mode'}).evaluate(measure),'mode toggle');
          assert.equal(await native.page.getByRole('radio',{name:'Preview',exact:true}).getAttribute('aria-checked'),'true');
          const originalProse = original.page.locator('main .prose').first();
          const nativeProse = native.page.locator('.native-issue-editor__preview.tc-markdown');
          close(await nativeProse.evaluate(measure),await originalProse.evaluate(measure),'description');
          assert.equal(await native.page.locator('[data-native-issue-seq]').isVisible(),false,'Sequence remains machine-readable without proof UI.');
          for (const session of [original,native]) assert.equal(await session.page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
          const originalAside = original.page.locator('main aside');
          const nativeAside = native.page.getByRole('complementary',{name:'Issue fields',exact:true});
          if (mode==='phone') {
            for (const session of [original,native]) await session.page.getByRole('button',{name:'Show details',exact:true}).click();
          }
          close(await nativeAside.evaluate(measure),await originalAside.evaluate(measure),'metadata panel');
          const metadataLabels = await originalAside.locator('.issue-meta-field-label').allTextContents();
          assert.deepEqual(metadataLabels,['Status','Priority','Module','Labels','Waiting on','Created','Updated']);
          assert.deepEqual(await nativeAside.locator('h2').allTextContents(),metadataLabels);
          if (mode==='phone') {
            for (const [kind,session] of [['original',original],['native',native]]) {
              await session.page.screenshot({path:path.join(output,`${name}-${kind}-details.png`),fullPage:true});
              await session.page.getByRole('button',{name:'Close details',exact:true}).last().click({position:{x:1,y:1}});
            }
            assert.equal(await native.page.getByRole('button',{name:'Show details',exact:true}).getAttribute('aria-expanded'),'false');
          }
          assert.equal(proxy.requests.filter(request=>new URL(request.path,proxy.origin).pathname.replace(prefix,'').startsWith('/api/')).length,0,
            'Native issue document and Activity actions make no frontend REST calls.');
          assert.deepEqual(errors,[]);
        } finally {
          try {
            fs.writeFileSync(path.join(output,`${name}-errors.json`),JSON.stringify({errors,requests:proxy.requests},null,2));
          } finally {
            try {if (original) await original.context.close();}
            finally {try {if (native) await native.context.close();}
              finally {await proxy.close();}}
          }
        }
      });
    }
  } finally {
    for (const socket of proxySockets) socket.destroy();
    try {await browser.close();}
    finally {try {if (vite) await vite.close();}
      finally {if (referenceCache) fs.rmSync(referenceCache,{recursive:true,force:true});}}
  }
});
