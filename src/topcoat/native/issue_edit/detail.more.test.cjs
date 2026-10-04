// The original and native views consume the same production database.
// node <driver> <fixture-origin> <token> <browser-helper> <pinned-master-web>
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {tmpdir} = require('node:os');
const {mountedProxy, launchBrowser} = require(process.argv[4]);
const upstream = new URL(process.argv[2]), token = process.argv[3], snapshot = process.argv[5];
const output = path.join(require('node:os').tmpdir(),'lific-native-issue-more');
const fixtureTitle = 'Production issue initial title';

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

test('native issue More and confirmation match pinned master at every mount',async t=>{
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
    const clientSockets=new Set();
    vite.httpServer.on('connection',socket=>{clientSockets.add(socket);socket.once('close',()=>clientSockets.delete(socket));});
    vite.httpServer.once('close',()=>{for(const socket of clientSockets)socket.destroy();});
    vite.lificClientSockets=clientSockets;
    await vite.listen(); const originalOrigin = `http://127.0.0.1:${vite.httpServer.address().port}`;
    for (const prefix of ['', '/app', '/ACC']) for (const mode of ['desktop','phone']) for (const theme of ['light','dark']) {
      await t.test(`${prefix||'root'} ${mode} ${theme}`,async()=>{
        const proxy = await mountedProxy(upstream,prefix);
        const original = await setup(browser,originalOrigin,mode,theme,false);
        const native = await setup(browser,proxy.origin,mode,theme,true);
        const name = `${prefix.slice(1)||'root'}-${mode}-${theme}`, errors = [];
        const nativeRequests = [];
        native.context.on('request', request => nativeRequests.push({url:request.url(), authorization:request.headers().authorization}));
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
          assert.equal(await original.page.title(),'Lific','Pinned master keeps the application document title.');
          assert.equal(await native.page.title(),await original.page.title(),'Native initial document title matches pinned master.');
          const painted = async locator => locator.evaluate(element => {
            const rect=element.getBoundingClientRect(),style=getComputedStyle(element);
            return {x:rect.x,y:rect.y,width:rect.width,height:rect.height,
              fontSize:style.fontSize,lineHeight:style.lineHeight,fontWeight:style.fontWeight,
              paddingTop:style.paddingTop,paddingRight:style.paddingRight,
              paddingBottom:style.paddingBottom,paddingLeft:style.paddingLeft,
              color:style.color,backgroundColor:style.backgroundColor,
              borderRadius:style.borderRadius,borderWidth:style.borderWidth,borderColor:style.borderColor};
          });
          const same = (actual,expected,label) => {
            for (const key of ['x','y','width','height']) assert.ok(Math.abs(actual[key]-expected[key])<=1,
              `${label}.${key}: native ${actual[key]}, original ${expected[key]}`);
            for(const key of Object.keys(expected).filter(key=>!['x','y','width','height'].includes(key)))
              assert.equal(actual[key],expected[key],`${label}.${key}`);
          };
          const icon = locator => locator.evaluate(svg => ({
            width:svg.getBoundingClientRect().width,height:svg.getBoundingClientRect().height,
            color:getComputedStyle(svg).color,viewBox:svg.getAttribute('viewBox'),
            stroke:svg.getAttribute('stroke'),strokeWidth:svg.getAttribute('stroke-width'),
            shape:[...svg.children].map(node=>({tag:node.localName,attributes:Object.fromEntries([...node.attributes]
              .filter(attr=>!['class','style'].includes(attr.name)).map(attr=>[attr.name,attr.value]))})),
          }));
          const originalMore=original.page.getByTitle('More actions',{exact:true});
          const nativeMore=native.page.getByTitle('More actions',{exact:true});
          assert.equal(await originalMore.count(),1,'Pinned Maintainer More is present.');
          assert.equal(await nativeMore.count(),1,'Real native Maintainer More is present.');
          const more={original:await painted(originalMore),native:await painted(nativeMore)};
          fs.writeFileSync(path.join(output,`${name}-more-geometry.json`),JSON.stringify(more,null,2));
          same(more.native,more.original,'More');
          assert.equal(more.original.width,28);assert.equal(more.original.height,28);
          assert.deepEqual(await icon(nativeMore.locator('svg')),await icon(originalMore.locator('svg')));
          if(mode==='phone'){
            const target=locator=>locator.evaluate(element=>{
              const style=getComputedStyle(element,'::after');return {width:style.width,height:style.height,content:style.content};
            });
            assert.deepEqual(await target(nativeMore),await target(originalMore),'Coarse pointer retains original44px target without changing visual28px control.');
          }
          for(const session of [original,native])await session.page.getByTitle('More actions',{exact:true}).click();
          const item=session=>session.page.getByRole('button',{name:'Delete issue',exact:true});
          for(const [kind,session]of[['original',original],['native',native]])
            await session.page.screenshot({path:path.join(output,`${name}-${kind}-more-menu.png`),fullPage:true});
          const menu={original:await painted(item(original).locator('..')),native:await painted(item(native).locator('..'))};
          fs.writeFileSync(path.join(output,`${name}-more-menu-geometry.json`),JSON.stringify(menu,null,2));
          assert.equal(menu.original.width,180);same(menu.native,menu.original,'Delete menu');
          same(await painted(item(native)),await painted(item(original)),'Delete issue');
          assert.deepEqual(await icon(item(native).locator('svg')),await icon(item(original).locator('svg')));
          for(const session of [original,native])await item(session).click();
          const card=session=>session.page.getByText('Delete ACC-1?',{exact:true}).locator('..');
          for(const [kind,session]of[['original',original],['native',native]]){
            await session.page.getByText("This can't be undone.",{exact:true}).waitFor();
            await session.page.screenshot({path:path.join(output,`${name}-${kind}-more-confirm.png`),fullPage:true});
          }
          const confirmation={original:await painted(card(original)),native:await painted(card(native))};
          fs.writeFileSync(path.join(output,`${name}-more-confirm-geometry.json`),JSON.stringify(confirmation,null,2));
          assert.equal(confirmation.original.width,260);same(confirmation.native,confirmation.original,'Confirmation');
          for(const copy of ['Delete ACC-1?',"This can't be undone."])
            same(await painted(native.page.getByText(copy,{exact:true})),await painted(original.page.getByText(copy,{exact:true})),copy);
          for(const label of ['Delete','Cancel'])
            same(await painted(card(native).getByRole('button',{name:label,exact:true})),
              await painted(card(original).getByRole('button',{name:label,exact:true})),label);
          // Exercise genuine local menu state only. Parent tests own scheduling,
          // Undo and delayed commits; this paired slice never deletes the seed.
          for(const session of [original,native]){
            await card(session).getByRole('button',{name:'Cancel',exact:true}).click();
            assert.equal(await session.page.getByText('Delete ACC-1?',{exact:true}).isVisible(),false);
            assert.equal(await item(session).count(),0);
            await session.page.getByTitle('More actions',{exact:true}).click();await item(session).click();
            await session.page.getByTitle('More actions',{exact:true}).click();
            assert.equal(await session.page.getByText('Delete ACC-1?',{exact:true}).isVisible(),false);
            assert.equal(await item(session).count(),0);
            await session.page.getByTitle('More actions',{exact:true}).click();
            await session.page.getByRole('heading',{name:'Production markdown',exact:true}).click({position:{x:1,y:1}});
            assert.equal(await item(session).count(),0,'Outside click closes original/native menu.');
            await session.page.getByTitle('More actions',{exact:true}).click();await item(session).click();
            await session.page.getByRole('heading',{name:'Production markdown',exact:true}).click({position:{x:1,y:1}});
            assert.equal(await session.page.getByText('Delete ACC-1?',{exact:true}).isVisible(),false,'Outside click closes confirmation.');
          }
          // Genuine application actions produce both Toasters. Freeze only their
          // timeout boundary; always Undo before closing either real document.
          const notification=(session,message)=>session.page.getByRole('status').filter({has:session.page.getByText(message,{exact:true})});
          const toastPaint=async locator=>locator.evaluate(element=>{
            const r=element.getBoundingClientRect(),s=getComputedStyle(element);
            return {x:r.x,y:r.y,width:r.width,height:r.height,fontSize:s.fontSize,
              lineHeight:s.lineHeight,fontWeight:s.fontWeight,color:s.color,backgroundColor:s.backgroundColor,
              paddingTop:s.paddingTop,paddingRight:s.paddingRight,paddingBottom:s.paddingBottom,paddingLeft:s.paddingLeft,
              borderTopWidth:s.borderTopWidth,borderRightWidth:s.borderRightWidth,
              borderBottomWidth:s.borderBottomWidth,borderLeftWidth:s.borderLeftWidth,
              borderColor:s.borderColor,borderLeftColor:s.borderLeftColor,borderRadius:s.borderRadius,
              boxShadow:s.boxShadow,gap:s.gap,alignItems:s.alignItems,pointerEvents:s.pointerEvents};
          });
          const compareToast=async(message,phase,withUndo)=>{
            const cards={original:notification(original,message),native:notification(native,message)};
            for(const [kind,session]of[['original',original],['native',native]]) {
              await cards[kind].waitFor();
              await session.page.mouse.move(0,0);
              await session.page.screenshot({path:path.join(output,`${name}-${kind}-toast-${phase}.png`),fullPage:true});
            }
            const geometry={original:await toastPaint(cards.original),native:await toastPaint(cards.native)};
            fs.writeFileSync(path.join(output,`${name}-toast-${phase}-geometry.json`),JSON.stringify(geometry,null,2));
            same(geometry.native,geometry.original,`${phase} toast`);
            same(await toastPaint(cards.native.locator('..')),await toastPaint(cards.original.locator('..')),`${phase} toast stack`);
            same(await toastPaint(cards.native.locator('p')),await toastPaint(cards.original.locator('p')),`${phase} toast message`);
            // Pinned info notifications paint an Info16 before the paragraph
            // and X13 inside the exact24px dismiss control.
            assert.equal(await cards.original.locator(':scope > svg').count(),1);
            assert.equal(await cards.native.locator(':scope > svg').count(),1,'Native paints the original leading info icon.');
            assert.deepEqual(await icon(cards.native.locator(':scope > svg')),await icon(cards.original.locator(':scope > svg')));
            const originalDismiss=cards.original.getByRole('button',{name:'Dismiss notification',exact:true});
            const nativeDismiss=cards.native.getByRole('button',{name:'Dismiss notification',exact:true});
            const dismiss=await toastPaint(originalDismiss);assert.equal(dismiss.width,24);assert.equal(dismiss.height,24);
            same(await toastPaint(nativeDismiss),dismiss,`${phase} dismiss`);
            assert.deepEqual(await icon(nativeDismiss.locator('svg')),await icon(originalDismiss.locator('svg')));
            if(withUndo) same(await toastPaint(cards.native.getByRole('button',{name:'Undo',exact:true})),
              await toastPaint(cards.original.getByRole('button',{name:'Undo',exact:true})),`${phase} Undo`);
          };
          try {
          for(const session of [original,native]) {
            await session.page.clock.pauseAt(await session.page.evaluate(()=>Date.now()+1000));
            await session.page.getByTitle('More actions',{exact:true}).click();
            await item(session).click();
            await card(session).getByRole('button',{name:'Delete',exact:true}).click();
          }
            await compareToast('Deleted ACC-1','pending',true);
          } finally {
            for(const session of [original,native]) {
              const pending=notification(session,'Deleted ACC-1');
              const undo=pending.getByRole('button',{name:'Undo',exact:true});
              if(await undo.count())await undo.click();
            }
          }
          for(const session of [original,native])await session.page.getByRole('button',{name:fixtureTitle,exact:true}).waitFor();
          await compareToast('Restored ACC-1','restored',false);
          assert.equal(await native.page.title(),await original.page.title(),'Delete and Undo retain the original document title.');
          assert.ok(!nativeRequests.some(request => new URL(request.url).pathname.split('/').includes('api')), 'Native controls never call the JSON API.');
          assert.ok(!nativeRequests.some(request => request.authorization), 'Native requests use cookie credentials.');
          assert.equal(await native.page.evaluate(() => localStorage.getItem('lific_token')), null);
          assert.deepEqual(errors,[]);
        } finally {
          fs.writeFileSync(path.join(output,`${name}-errors.json`),JSON.stringify({errors,nativeRequests,requests:proxy.requests},null,2));
          await original.context.close(); await native.context.close(); await proxy.close();
        }
      });
    }
  } finally {
    // Track Vite client sockets too: proxySockets contains only upstream WS.
    for (const socket of proxySockets) socket.destroy();
    try {await browser.close();}
    finally {try {
      if(vite) {
        const closing=vite.close();
        for(const socket of vite.lificClientSockets||[])socket.destroy();
        vite.httpServer?.closeAllConnections();
        await closing;
      }
    } finally {if (referenceCache) fs.rmSync(referenceCache,{recursive:true,force:true});}}
  }
});
