// The original and native views consume the same production database.
// node <driver> <fixture-origin> <token> <browser-helper> <pinned-master-web>
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {mountedProxy, launchBrowser} = require(process.argv[4]);
const upstream = new URL(process.argv[2]), token = process.argv[3], snapshot = process.argv[5];
const output = path.join(require('node:os').tmpdir(),'lific-native-issue-detail');
const fixtureTitle = 'Production issue initial title';

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
  if (native) await context.addCookies([{name:'lific_token',value:token,url:origin,httpOnly:true,sameSite:'Lax'}]);
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
  const browser = await launchBrowser(); let vite;
  const proxySockets = new Set();
  try {
    const {createServer} = await import(pathToFileURL(path.join(snapshot,'node_modules/vite/dist/node/index.js')).href);
    const configure = proxy=>proxy.on('open',socket=>{
      proxySockets.add(socket); socket.once('close',()=>proxySockets.delete(socket));
    });
    vite = await createServer({root:snapshot,logLevel:'silent',configFile:path.join(snapshot,'vite.config.ts'),
      server:{host:'127.0.0.1',port:0,strictPort:false,proxy:{
        '/api':{target:upstream.origin,ws:true,configure},
        '/public/api':{target:upstream.origin,ws:true,configure},
      }}});
    await vite.listen(); const originalOrigin = `http://127.0.0.1:${vite.httpServer.address().port}`;
    for (const prefix of ['', '/app', '/ACC']) for (const mode of ['desktop','phone']) for (const theme of ['light','dark']) {
      await t.test(`${prefix||'root'} ${mode} ${theme}`,async()=>{
        const proxy = await mountedProxy(upstream,prefix);
        const original = await setup(browser,originalOrigin,mode,theme,false);
        const native = await setup(browser,proxy.origin,mode,theme,true);
        const name = `${prefix.slice(1)||'root'}-${mode}-${theme}`, errors = [];
        for (const [kind,session] of [['original',original],['native',native]]) {
          session.page.on('pageerror',error=>errors.push({kind,message:error.message}));
        }
        try {
          await original.page.goto(`${originalOrigin}/#/ACC/issues/ACC-1`);
          assert.equal((await native.page.goto(`${proxy.origin}${prefix}/ACC/issues/ACC-1`)).status(),200);
          for (const session of [original,native]) {
            await session.page.getByRole('button',{name:fixtureTitle,exact:true}).waitFor();
            await session.page.getByRole('heading',{name:'Production markdown',exact:true}).waitFor();
            await session.page.evaluate(()=>document.fonts.ready);
          }
          // Capture both actual documents before the first parity assertion.
          await original.page.screenshot({path:path.join(output,`${name}-original.png`),fullPage:true});
          await native.page.screenshot({path:path.join(output,`${name}-native.png`),fullPage:true});
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
              await session.page.getByRole('button',{name:'Close details',exact:true}).last().click();
            }
            assert.equal(await native.page.getByRole('button',{name:'Show details',exact:true}).getAttribute('aria-expanded'),'false');
          }
          assert.deepEqual(errors,[]);
        } finally {
          fs.writeFileSync(path.join(output,`${name}-errors.json`),JSON.stringify({errors,requests:proxy.requests},null,2));
          await original.context.close(); await native.context.close(); await proxy.close();
        }
      });
    }
  } finally {
    await browser.close(); for (const socket of proxySockets) socket.destroy(); if(vite) await vite.close();
  }
});
