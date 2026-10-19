const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {tmpdir} = require('node:os');
const {assertOriginalSources} = require('../original_source_fixture.cjs');
const {pathToFileURL} = require('node:url');
const {launchBrowser, mountedProxy, settleScroll} = require('../browser_fixture.cjs');
const {installOriginalFonts} = require('../original_fonts_fixture.cjs');
const {prepareOriginalVite, closeOriginalVite} = require('../original_vite_fixture.cjs');
const [origin, token, snapshot] = process.argv.slice(2);
const output = path.join(tmpdir(), 'lific-native-insights-visual');

function measure(element) {
  const rect = element.getBoundingClientRect(), css = getComputedStyle(element);
  let x = rect.x, y = rect.y;
  for (let parent = element.parentElement; parent; parent = parent.parentElement) {x += parent.scrollLeft; y += parent.scrollTop;}
  return {x,y,width:rect.width,height:rect.height,fontFamily:css.fontFamily,fontSize:css.fontSize,
    fontWeight:css.fontWeight,lineHeight:css.lineHeight,color:css.color,background:css.backgroundColor,
    borderRadius:css.borderRadius,padding:css.padding,gap:css.gap};
}
function compare(actual, expected, label) {
  for (const key of ['x','y','width','height']) assert.ok(Math.abs(actual[key]-expected[key])<=1,`${label}.${key}: native ${actual[key]}, Main ${expected[key]}`);
  for (const key of Object.keys(expected).filter(key=>!['x','y','width','height'].includes(key))) assert.equal(actual[key],expected[key],`${label}.${key}`);
}
function targets(page, native) {
  const hero = page.getByRole('heading',{name:'Created vs. closed',exact:true}).locator('xpath=ancestor::section[1]');
  const content = hero.locator('..');
  const distribution = name => content.getByRole('heading',{name,exact:true}).locator('xpath=ancestor::section[1]');
  return {content,hero,heading:hero.getByRole('heading'),chart:hero.locator('svg[aria-label="Issues created vs closed per week"]'),
    status:distribution('Status'),priority:distribution('Priority'),module:distribution('Module'),actors:distribution('Top actors'),
    window:page.getByRole('button',{name:'12w',exact:true}),topbar:native?page.locator('.native-insights__topbar'):page.getByRole('button',{name:'12w',exact:true}).locator('..').locator('..').locator('..')};
}
async function session(browser, url, viewport, theme, native) {
  const context = await browser.newContext({viewport,colorScheme:theme,reducedMotion:'reduce',locale:'en-US',timezoneId:'America/Denver'});
  if (!native) await installOriginalFonts(context);
  await context.addCookies([{name:'lific_token',value:token,url,httpOnly:true,sameSite:'Lax'}]);
  await context.addInitScript(({token,theme,native})=>{localStorage.setItem('lific_theme',theme);localStorage.setItem('lific_motion','reduced');if(!native)localStorage.setItem('lific_token',token);},{token,theme,native});
  const page=await context.newPage();page.setDefaultTimeout(15000);return {context,page};
}

test('native Insights matches Main cards, week selection and hovered chart without frontend API requests',async()=>{
  assert.ok(snapshot,'Pinned Main reference directory is required');
  assertOriginalSources(snapshot, ['src/routes/Insights.svelte','src/lib/insights/TrendChart.svelte','src/lib/insights/DistributionList.svelte','src/lib/insights/ActorList.svelte','src/lib/insights/curve.ts']);
  fs.mkdirSync(output,{recursive:true});
  const {createServer}=await import(pathToFileURL(path.join(snapshot,'node_modules/vite/dist/node/index.js')).href);
  const cache=fs.mkdtempSync(path.join(tmpdir(),'lific-insights-vite-'));
  const sockets=new Set();
  const configure=proxy=>proxy.on('open',socket=>{sockets.add(socket);socket.once('close',()=>sockets.delete(socket));});
  const vite=await createServer({root:snapshot,cacheDir:cache,configFile:path.join(snapshot,'vite.config.ts'),logLevel:'silent',server:{host:'127.0.0.1',port:0,strictPort:false,proxy:{'/api':{target:origin,ws:true,configure},'/public/api':{target:origin,ws:true,configure}}}});
  const browser=await launchBrowser(), report=[];
  try {
    await vite.listen();await prepareOriginalVite(vite);
    const originalOrigin=`http://127.0.0.1:${vite.httpServer.address().port}`;
    for (const prefix of ['', '/app', '/ACC']) for (const viewport of [{width:1440,height:900},{width:390,height:844}]) for (const theme of ['light','dark']) {
      const proxy=await mountedProxy(new URL(origin),prefix);
      const main=await session(browser,originalOrigin,viewport,theme,false);
      const native=await session(browser,proxy.origin,viewport,theme,true);
      const errors=[],requests=[];
      native.page.on('pageerror',error=>errors.push(error.message));
      native.page.on('request',request=>requests.push(new URL(request.url()).pathname));
      const name=`${prefix.replaceAll('/','_')||'root'}-${viewport.width}-${theme}`;
      const evidence={name,main:{},native:{}};report.push(evidence);
      try {
        assert.equal((await main.page.goto(`${originalOrigin}/#/ACC/insights`)).status(),200);
        assert.equal((await native.page.goto(`${proxy.origin}${prefix}/ACC/insights`)).status(),200);
        const sides=[['main',main,false],['native',native,true]];
        for(const [kind,item,isNative] of sides){await item.page.getByRole('heading',{name:'Created vs. closed',exact:true}).waitFor();await item.page.evaluate(()=>document.fonts.ready);item.targets=targets(item.page,isNative);await settleScroll(item.page);await item.page.mouse.move(0,0);
          for(const [key,target] of Object.entries(item.targets)) evidence[kind][key]=await target.evaluate(measure);
          await item.page.screenshot({path:path.join(output,`${name}-${kind}.png`),fullPage:true});
        }
        for(const key of Object.keys(evidence.main))compare(evidence.native[key],evidence.main[key],`${name}.${key}`);
        for (const window of [4,26,52,12]) {
          for(const [kind,item] of sides){await item.page.getByRole('button',{name:`${window}w`,exact:true}).click();await item.targets.hero.getByText(`last ${window} weeks`,{exact:true}).waitFor();
            const chart=item.targets.chart;
            assert.equal(await chart.locator('circle').count(),window*2,`${kind} dense week dots`);
            if(kind==='native')assert.equal(await item.page.getByRole('button',{name:`${window}w`,exact:true}).getAttribute('aria-pressed'),'true');
          }
        }
        const mainPaths=await main.targets.chart.locator('path').evaluateAll(nodes=>nodes.map(node=>node.getAttribute('d')));
        const nativePaths=await native.targets.chart.locator('path').evaluateAll(nodes=>nodes.map(node=>node.getAttribute('d')));
        assert.equal(nativePaths.length,mainPaths.length);
        for(let i=0;i<mainPaths.length;i++){
          const parse=s=>s.match(/-?\d+(?:\.\d+)?(?:e[+-]?\d+)?/gi).map(Number),a=parse(nativePaths[i]),b=parse(mainPaths[i]);assert.equal(a.length,b.length);
          for(let j=0;j<a.length;j++)assert.ok(Math.abs(a[j]-b[j])<1e-8,'Main spline geometry');
        }
        const mainChartContainer=main.targets.chart.locator('..');
        await mainChartContainer.locator('[role="presentation"]').last().hover();
        const mainTooltip=mainChartContainer.locator('[class~="pointer-events-none"]');
        await mainTooltip.waitFor({state:'visible'});
        await native.page.locator('[data-trend-week="11"]').hover();
        const nativeTooltip=native.page.locator('[data-trend-tooltip="11"]');
        await nativeTooltip.waitFor({state:'visible'});
        evidence.main.hover=await mainTooltip.evaluate(measure);
        evidence.native.hover=await nativeTooltip.evaluate(measure);
        compare(evidence.native.hover,evidence.main.hover,`${name}.hover`);
        for(let row=0;row<3;row++) {
          const mainRow=mainTooltip.locator('p').nth(row),nativeRow=nativeTooltip.locator('p').nth(row);
          compare(await nativeRow.evaluate(measure),await mainRow.evaluate(measure),`${name}.hover.row${row}`);
        }
        for(let count=0;count<2;count++) {
          compare(await nativeTooltip.locator('span').nth(count).evaluate(measure),await mainTooltip.locator('span').nth(count).evaluate(measure),`${name}.hover.count${count}`);
        }
        await main.page.screenshot({path:path.join(output,`${name}-main-hover.png`),fullPage:true});
        await native.page.screenshot({path:path.join(output,`${name}-native-hover.png`),fullPage:true});
        assert.ok((await native.page.locator('[data-trend-tooltip="11"]').textContent()).includes('Created 2'));
        assert.equal(await native.targets.chart.locator('circle[r="3.5"]').count(),2);
        await native.page.getByRole('button',{name:'12w',exact:true}).focus();
        await native.page.keyboard.press('Tab');
        await native.page.mouse.move(0,0);
        await native.page.locator('[data-trend-tooltip="11"]').waitFor({state:'hidden'});
        assert.equal(await native.page.locator('.native-insights-chart__sr-only tbody tr').count(),12);
        if(prefix==='' && viewport.width===1440 && theme==='light'){
          const fault=action=>native.page.evaluate(async action=>{const response=await fetch('/__native_insights_test/fault',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify([action])});if(!response.ok)throw new Error('Fixture fault rejected');},action);
          await native.page.evaluate(()=>{window.insightsRenderFailures=0;window.addEventListener('topcoat:render-error',()=>window.insightsRenderFailures++);});
          await fault('transient');
          try {
            await native.page.getByRole('button',{name:'26w',exact:true}).click();
            await native.page.waitForFunction(()=>window.insightsRenderFailures>0);
            assert.equal(await native.page.getByRole('button',{name:'26w',exact:true}).getAttribute('aria-pressed'),'true');
            assert.ok(await native.targets.hero.getByText('last 12 weeks',{exact:true}).isVisible(),'Transient selection preserves previous cards');
          } finally {await fault('restore');}
          await native.page.getByRole('button',{name:'4w',exact:true}).click();
          await native.targets.hero.getByText('last 4 weeks',{exact:true}).waitFor();
          await native.page.getByRole('button',{name:'12w',exact:true}).click();
          await native.targets.hero.getByText('last 12 weeks',{exact:true}).waitFor();
        }
        // Real navigation retains the shared sidebar owner and resets the page window.
        await native.page.evaluate(()=>{window.insightsOwnerWitness='retained';});
        await native.page.locator('.native-insights__breadcrumb a').click();
        await native.page.getByRole('heading',{name:'Needs attention',exact:true}).waitFor();
        if(viewport.width<768){await native.page.getByRole('button',{name:'Open navigation',exact:true}).click();if(!await native.page.getByRole('dialog').getByRole('link',{name:'Insights',exact:true}).count())await native.page.getByRole('dialog').getByRole('button',{name:'Open Visible project navigation',exact:true}).click();await native.page.getByRole('dialog').getByRole('link',{name:'Insights',exact:true}).click();}else{await native.page.locator('.native-home-sidebar').locator('a[href$="/ACC/insights"]').click();}
        await native.page.getByRole('heading',{name:'Created vs. closed',exact:true}).waitFor();
        assert.equal(await native.page.evaluate(()=>window.insightsOwnerWitness),'retained');
        assert.equal(await native.page.getByRole('button',{name:'12w',exact:true}).getAttribute('aria-pressed'),'true');
        // Paired empty and unavailable-project frames use the real backend.
        for(const [project,title] of [['EMP','Nothing to chart yet'],['HIDE',"Couldn't load insights"]]){
          await main.page.goto(`${originalOrigin}/#/`+project+'/insights');
          await native.page.goto(`${proxy.origin}${prefix}/`+project+'/insights');
          const original=main.page.getByText(title,{exact:true}),actual=native.page.getByText(title,{exact:true});
          await original.waitFor();await actual.waitFor();
          compare(await actual.evaluate(measure),await original.evaluate(measure),`${name}.${project}.title`);
          await main.page.screenshot({path:path.join(output,`${name}-${project}-main.png`),fullPage:true});
          await native.page.screenshot({path:path.join(output,`${name}-${project}-native.png`),fullPage:true});
        }
        assert.deepEqual(errors,[]);
        assert.deepEqual(requests.filter(url=>/(?:^|\/)api\//.test(url)),[],'Native Insights uses framework transport and direct Rust reads');
      } finally {await main.context.close();await native.context.close();await proxy.close();}
    }
    const revoked=await session(browser,origin,{width:1440,height:900},'light',true);
    try {
      await revoked.page.goto(origin+'/ACC/insights');
      await revoked.page.getByRole('heading',{name:'Created vs. closed',exact:true}).waitFor();
      await revoked.page.evaluate(async()=>{const response=await fetch('/__native_insights_test/fault',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(['revoke'])});if(!response.ok)throw new Error('Revocation fixture rejected');});
      await revoked.page.getByRole('button',{name:'26w',exact:true}).click();
      await revoked.page.getByText("Couldn't load insights",{exact:true}).waitFor();
      assert.equal(await revoked.page.locator('svg[aria-label="Issues created vs closed per week"]').count(),0,'Revoked membership clears previous metrics');
      await revoked.page.screenshot({path:path.join(output,'revoked-native.png'),fullPage:true});
    } finally {await revoked.context.close();}
  } finally {
    fs.writeFileSync(path.join(output,'geometry.json'),JSON.stringify(report,null,2));
    await browser.close();for(const socket of sockets)socket.destroy();await closeOriginalVite(vite);fs.rmSync(cache,{recursive:true,force:true});
  }
});
