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
const output = path.join(require('node:os').tmpdir(),'lific-native-issue-detail');
const fixtureTitle = 'Production issue initial title';

async function setup(browser, origin, mode, theme, native) {
  const viewport = mode==='phone'?{width:390,height:844}:{width:1440,height:900};
  const context = await browser.newContext({viewport,isMobile:mode==='phone',hasTouch:mode==='phone',
    colorScheme:theme,locale:'en-US',timezoneId:'America/Denver',reducedMotion:'reduce'});
  if (!native) await installOriginalFonts(context);
  if (native) await context.addCookies([{name:'lific_token',value:token,url:origin,httpOnly:true,sameSite:'Lax'}]);
  await context.addInitScript(({theme,token,native})=>{
    localStorage.setItem('lific_theme',theme); localStorage.setItem('lific_motion','reduced');
    if (!native) localStorage.setItem('lific_token',token);
  },{theme,token,native});
  const page = await context.newPage(); page.setDefaultTimeout(15000);
  await page.clock.setFixedTime('2026-10-03T16:00:00Z');
  return {context,page};
}

test('native issue decorations match pinned master at every mount',async t=>{
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
        '/api':{target:upstream.origin,ws:true,configure},
        '/public/api':{target:upstream.origin,ws:true,configure},
      }}});
    await vite.listen(); const originalOrigin = `http://127.0.0.1:${vite.httpServer.address().port}`;
    for (const prefix of ['', '/app', '/ACC']) for (const mode of ['desktop']) for (const theme of ['light','dark']) {
      await t.test(`${prefix||'root'} ${mode} ${theme}`,async caseTest=>{
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
            if (session === original) await captureOriginalFonts(session.page,path.join(output,`${name}-original-fonts.json`));
          }
          await original.page.screenshot({path:path.join(output,`${name}-decorations-original.png`),fullPage:true});
          await native.page.screenshot({path:path.join(output,`${name}-decorations-native.png`),fullPage:true});
          // Compare real production values before any missing-action geometry.
          const field=(session,nativeView,label)=>{
            const aside=nativeView?session.page.getByRole('complementary',{name:'Issue fields',exact:true}):session.page.locator('main aside');
            const heading=nativeView?session.page.getByRole('heading',{name:label,exact:true}):session.page.locator('.issue-meta-field-label').filter({hasText:new RegExp(`^${label}$`)});
            return aside.locator(nativeView?'section':'.issue-meta-field').filter({has:heading}).first();
          };
          const color=locator=>locator.evaluate(element=>getComputedStyle(element).color);
          const icon=async locator=>{
            assert.equal(await locator.count(),1,'The actual value has one decorative SVG.');
            return locator.evaluate(svg=>{
              const rect=svg.getBoundingClientRect(),style=getComputedStyle(svg);
              return {width:rect.width,height:rect.height,color:style.color,
                viewBox:svg.getAttribute('viewBox'),stroke:svg.getAttribute('stroke'),strokeWidth:svg.getAttribute('stroke-width'),
                shape:[...svg.children].map(node=>({tag:node.localName,attributes:Object.fromEntries([...node.attributes]
                  .filter(attribute=>!['class','style'].includes(attribute.name)).map(attribute=>[attribute.name,attribute.value]))}))};
            });
          };
          const decoratedIcon = async (value, nativeView) => {
            const glyph = value.locator(nativeView ? '.native-issue-detail__decoration svg:visible' : 'svg:visible');
            if (nativeView) assert.equal(await glyph.count(),1,'Exactly one current decoration paints.');
            return icon(glyph.first());
          };
          const masterHeader=original.page.getByTitle('Change status',{exact:true});
          const nativeHeader=native.page.getByTitle('Change status',{exact:true});
          const masterStatus=field(original,false,'Status'),nativeStatus=field(native,true,'Status');
          const masterPriority=field(original,false,'Priority'),nativePriority=field(native,true,'Priority');
          assert.equal((await masterStatus.locator('button').first().textContent()).trim().toLowerCase(),'active');
          assert.equal((await masterPriority.locator('button').first().textContent()).trim(),'Medium');
          await caseTest.test(`${name} header Active dot`,async()=>{
            const expected=await decoratedIcon(masterHeader,false);
            assert.equal(expected.width,13,'Pinned StatusIcon header size is 13.');
            assert.deepEqual(await decoratedIcon(nativeHeader,true),expected);
          });
          await caseTest.test(`${name} metadata Active dot`,async()=>{
            const expected=await decoratedIcon(masterStatus.locator('button').first(),false);
            assert.equal(expected.width,14,'Pinned StatusIcon metadata size is 14.');
            assert.deepEqual(await decoratedIcon(nativeStatus.locator('button').first(),true),expected);
          });
          await caseTest.test(`${name} metadata Medium glyph and accent`,async()=>{
            const expected=await decoratedIcon(masterPriority.locator('button').first(),false);
            assert.equal(expected.width,14,'Pinned PriorityIcon metadata size is 14.');
            assert.deepEqual(await decoratedIcon(nativePriority.locator('button').first(),true),expected);
            const masterText=masterPriority.getByText('Medium',{exact:true}),nativeText=nativePriority.locator('[data-native-issue-priority]');
            assert.equal(await color(nativeText),await color(masterText),'Priority label uses the same actual accent as the pinned original.');
          });
          for(const label of ['Module','Labels'])await caseTest.test(`${name} ${label} None color`,async()=>{
            const master=field(original,false,label).getByText('None',{exact:true});
            const current=field(native,true,label).getByText('None',{exact:true});
            assert.equal(await color(current),await color(master),`${label} empty value uses pinned text-faint rather than primary text.`);
          });
          assert.equal(await native.page.getByRole('button',{name:fixtureTitle,exact:true}).count(),1,'Initial issue HTML remains populated.');
          assert.equal(await native.page.getByRole('heading',{name:'Production markdown',exact:true}).count(),1);
          const nativeCrumb=native.page.getByRole('navigation',{name:'Breadcrumb'});
          assert.equal(await nativeCrumb.getByRole('link',{name:'ACC',exact:true}).getAttribute('href'),`${prefix}/ACC/overview`);
          assert.equal(await nativeCrumb.getByRole('link',{name:'Issues',exact:true}).getAttribute('href'),`${prefix}/ACC/issues`);
          assert.equal(await native.page.evaluate(()=>localStorage.getItem('lific_token')),null);
          assert.deepEqual(errors,[]);
        } finally {
          fs.writeFileSync(path.join(output,`${name}-errors.json`),JSON.stringify({errors,requests:proxy.requests},null,2));
          await original.context.close(); await native.context.close(); await proxy.close();
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
