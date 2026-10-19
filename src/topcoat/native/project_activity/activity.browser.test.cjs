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

const [origin, token, snapshot = process.env.LIFIC_MAIN_FRONTEND] = process.argv.slice(2);
const output = path.join(tmpdir(), 'lific-native-project-activity-visual');
const actorName = actor => actor.display_name || actor.username || 'system';

function measure(element) {
  const rect = element.getBoundingClientRect(), css = getComputedStyle(element);
  let x = rect.x, y = rect.y;
  for (let parent = element.parentElement; parent; parent = parent.parentElement) {
    x += parent.scrollLeft;
    y += parent.scrollTop;
  }
  return {x, y, width:rect.width, height:rect.height, fontFamily:css.fontFamily,
    fontSize:css.fontSize, fontWeight:css.fontWeight, lineHeight:css.lineHeight,
    color:css.color, background:css.backgroundColor, borderRadius:css.borderRadius,
    padding:css.padding, gap:css.gap,
    ...(element instanceof SVGElement ? {fill:css.fill, stroke:css.stroke,
      strokeWidth:css.strokeWidth} : {})};
}
function compare(actual, expected, label) {
  for (const key of ['x', 'y', 'width', 'height']) {
    assert.ok(Math.abs(actual[key] - expected[key]) <= 1,
      `${label}.${key}: native ${actual[key]}, Main ${expected[key]}`);
  }
  for (const key of Object.keys(expected).filter(key => !['x', 'y', 'width', 'height'].includes(key))) {
    assert.equal(actual[key], expected[key], `${label}.${key}`);
  }
}
async function api(url) {
  const response = await fetch(origin + url, {headers:{authorization:`Bearer ${token}`}});
  assert.equal(response.status, 200, url);
  return response.json();
}
async function session(browser, url, viewport, theme, native) {
  const context = await browser.newContext({viewport, colorScheme:theme, reducedMotion:'reduce',
    locale:'en-US', timezoneId:'America/Denver'});
  if (!native) await installOriginalFonts(context);
  await context.addCookies([{name:'lific_token', value:token, url, httpOnly:true, sameSite:'Lax'}]);
  await context.addInitScript(({token, theme, native}) => {
    localStorage.setItem('lific_theme', theme);
    localStorage.setItem('lific_motion', 'reduced');
    if (!native) localStorage.setItem('lific_token', token);
  }, {token, theme, native});
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  return {context, page, native};
}
function targets(item) {
  const {page, native} = item;
  const rail = native ? page.locator('.native-project-activity__actors')
    : page.locator('aside').filter({has:page.getByText('Actors', {exact:true})});
  const content = rail.locator('..');
  const feed = native ? page.locator('.native-project-activity__feed') : content.locator(':scope > div').first();
  const toggles = native ? feed.locator('.native-project-activity__row-toggle')
    : feed.locator('[role="button"][tabindex="0"]');
  const topbar = native ? page.locator('.native-project-activity__topbar')
    : page.locator('div.flex.gap-3.px-6.py-2.w-full').filter({has:page.getByText('Activity', {exact:true})});
  return {rail, content, feed, toggles, topbar,
    actors:rail.locator('button[title*="most via"]'),
    count:topbar.locator(':scope > div').first().locator(':scope > span').last()};
}
const record = card => card.locator(':scope > div').nth(1);
const cardAt = (item, index) => item.targets.toggles.nth(index).locator('..');
async function expectRows(item, count) {
  await assert.doesNotReject(() => item.targets.toggles.nth(count - 1).waitFor());
  await item.page.waitForFunction(({native, count}) => {
    const selector = native ? '.native-project-activity__row-toggle'
      : 'aside button[title*="most via"]';
    const first = document.querySelector(selector);
    if (!first) return false;
    if (native) return document.querySelectorAll(selector).length === count;
    const rail = first.closest('aside');
    return rail.parentElement.querySelectorAll('[role="button"][tabindex="0"]').length === count;
  }, {native:item.native, count});
  assert.equal(await item.targets.toggles.count(), count);
}
async function expandedCount(item) {
  return item.native ? item.targets.feed.locator('[data-activity-expanded="true"]').count()
    : item.targets.feed.locator('div[class~="pb-3.5"]').count();
}
async function resetScroll(item) {
  await item.page.evaluate(() => {
    for (const node of document.querySelectorAll('*')) {
      if (node.scrollTop) node.scrollTop = 0;
      if (node.scrollLeft) node.scrollLeft = 0;
    }
  });
  await settleScroll(item.page);
  await item.page.mouse.move(0, 0);
}
async function capture(item, name) {
  await item.page.evaluate(() => document.fonts.ready);
  await settleScroll(item.page);
  await item.page.screenshot({path:path.join(output, name), fullPage:true, animations:'disabled'});
}

async function paired(main, native, name, state, report, extra = {}) {
  await resetScroll(main);
  await resetScroll(native);
  const entry = {name, state, main:{}, native:{}};
  report.push(entry);
  for (const key of ['content', 'feed', 'rail', 'topbar']) {
    entry.main[key] = await main.targets[key].evaluate(measure);
    entry.native[key] = await native.targets[key].evaluate(measure);
  }
  for (const [key, [expected, actual]] of Object.entries(extra)) {
    entry.main[key] = await expected.evaluate(measure);
    entry.native[key] = await actual.evaluate(measure);
  }
  for (const [side, item] of [['main', main], ['native', native]]) {
    entry[side].days = await item.targets.feed.locator('div.sticky').evaluateAll(nodes =>
      nodes.map(node => [...node.querySelectorAll(':scope > span')].map(span => span.textContent.trim())));
    entry[side].rowChildren = await item.targets.toggles.first().evaluate(element =>
      [...element.children].map(child => {
        const rect = child.getBoundingClientRect(), css = getComputedStyle(child);
        return {tag:child.tagName, class:child.getAttribute('class'), width:rect.width,
          height:rect.height, display:css.display, lineHeight:css.lineHeight,
          margin:css.margin, padding:css.padding, border:css.border, boxSizing:css.boxSizing,
          children:[...child.children].map(node => ({tag:node.tagName,
            height:node.getBoundingClientRect().height, display:getComputedStyle(node).display}))};
      }));
  }
  await capture(main, `${name}-${state}-main.png`);
  await capture(native, `${name}-${state}-native.png`);
  assert.deepEqual(entry.native.days, entry.main.days, `${name}.${state}.day labels and counts`);
  for (const key of ['content', 'feed', 'rail', 'topbar', ...Object.keys(extra)]) {
    compare(entry.native[key], entry.main[key], `${name}.${state}.${key}`);
  }
}

test('native project Activity matches Main history, actor filters, expansion, pagination and refresh', async () => {
  assert.ok(snapshot, 'Pass the external pinned Main web directory as the third argument.');
  assertOriginalSources(snapshot, ['src/routes/ProjectActivity.svelte', 'src/lib/linediff.ts']);
  fs.mkdirSync(output, {recursive:true});
  const projects = await api('/api/projects');
  const project = projects.find(project => project.identifier === 'ACC');
  assert.ok(project, 'The shared fixture exposes ACC.');
  const {createServer} = await import(pathToFileURL(path.join(snapshot, 'node_modules/vite/dist/node/index.js')).href);
  const cache = fs.mkdtempSync(path.join(tmpdir(), 'lific-activity-vite-'));
  const sockets = new Set();
  const configure = proxy => {
    proxy.on('proxyReqWs', request => request.setHeader('origin', origin));
    proxy.on('open', socket => {
      sockets.add(socket); socket.once('close', () => sockets.delete(socket));
    });
  };
  const vite = await createServer({root:snapshot, cacheDir:cache,
    configFile:path.join(snapshot, 'vite.config.ts'), logLevel:'silent',
    server:{host:'127.0.0.1', port:0, strictPort:false, proxy:{'/api':{target:origin, changeOrigin:true, ws:true, configure},
      '/public/api':{target:origin, changeOrigin:true, ws:true, configure}}}});
  const browser = await launchBrowser(), report = [];
  try {
    await vite.listen();
    await prepareOriginalVite(vite);
    const mainOrigin = `http://127.0.0.1:${vite.httpServer.address().port}`;
    for (const prefix of ['', '/app', '/app/ACC']) {
      for (const viewport of [{width:1440, height:900}, {width:390, height:844}]) {
        for (const theme of ['light', 'dark']) {
          const reset = await fetch(origin + '/__native_project_activity_test/refresh', {
            method:'POST', headers:{cookie:`lific_token=${token}`, origin, 'content-type':'application/json'},
            body:JSON.stringify({reset:true}),
          });
          assert.equal(reset.status, 200);
          assert.equal((await reset.json()).total, 70, 'Each matrix case starts with the same real history.');
          const initial = await api(`/api/projects/${project.id}/activity?limit=500&offset=0`);
          const actors = await api(`/api/projects/${project.id}/activity/actors`);
          assert.equal(initial.items.length, 70, 'The fixture exercises exactly one additional page.');
          assert.ok(actors.some(actor => actor.actor_user_id === null), 'System actor bucket is seeded.');
          assert.ok(actors.some(actor => actor.is_bot), 'Agent actor is seeded.');
          const loaded = initial.items.slice(0, 50);
          const proxy = await mountedProxy(new URL(origin), prefix);
          const main = await session(browser, mainOrigin, viewport, theme, false);
          const native = await session(browser, proxy.origin, viewport, theme, true);
          const errors = [], requests = [];
          const mainErrors = [], mainWebsocketEvents = [];
          main.page.on('pageerror', error => mainErrors.push(error.message));
          main.page.on('console', message => {
            if (message.type() === 'error') mainErrors.push(message.text());
          });
          main.page.on('websocket', socket => {
            const url = socket.url();
            mainWebsocketEvents.push({event:'created', url});
            socket.on('framereceived', frame => mainWebsocketEvents.push({event:'received', url,
              payload:String(frame.payload).slice(0, 4000)}));
            socket.on('framesent', frame => mainWebsocketEvents.push({event:'sent', url,
              payload:String(frame.payload).slice(0, 4000)}));
            socket.on('close', () => mainWebsocketEvents.push({event:'close', url}));
            socket.on('socketerror', error => mainWebsocketEvents.push({event:'error', url, error:String(error)}));
          });
          const websocketEvents = [], pendingRequests = new Set();
          await native.page.addInitScript(() => {
            window.activityRenderErrors = [];
            document.addEventListener('topcoat:render-error', event => {
              window.activityRenderErrors.push({path:event.detail?.path, message:String(event.detail?.error || '')});
            }, true);
          });
          native.page.on('websocket', socket => {
            const url = socket.url();
            websocketEvents.push({event:'open', url});
            socket.on('framereceived', frame => websocketEvents.push({event:'received', url,
              payload:String(frame.payload).slice(0, 4000)}));
            socket.on('framesent', frame => websocketEvents.push({event:'sent', url,
              payload:String(frame.payload).slice(0, 4000)}));
            socket.on('close', () => websocketEvents.push({event:'close', url}));
            socket.on('socketerror', error => websocketEvents.push({event:'error', url, error:String(error)}));
          });
          native.page.on('request', request => pendingRequests.add(request));
          native.page.on('requestfinished', request => pendingRequests.delete(request));
          native.page.on('requestfailed', request => pendingRequests.delete(request));
          native.page.on('pageerror', error => errors.push(error.message));
          native.page.on('console', message => {if (message.type() === 'error') errors.push(message.text());});
          native.page.on('request', request => requests.push({method:request.method(), path:new URL(request.url()).pathname}));
          const name = `${prefix.replaceAll('/', '_') || 'root'}-${viewport.width}-${theme}`;
          try {
            await main.page.goto(`${mainOrigin}/#/ACC/activity`);
            assert.equal((await native.page.goto(`${proxy.origin}${prefix}/ACC/activity`)).status(), 200);
            for (const item of [main, native]) {
              await item.page.getByText('Actors', {exact:true}).waitFor();
              await item.page.evaluate(() => document.fonts.ready);
              item.targets = targets(item);
              await expectRows(item, 50);
              assert.equal((await item.targets.count.textContent()).trim(), '50+');
              assert.equal(await item.targets.actors.count(), actors.length);
              assert.deepEqual(await item.targets.actors.evaluateAll(nodes => nodes.map(node => node.title)),
                actors.map(actor => `Show only ${actorName(actor)} · most via ${actor.top_transport}`));
            }
            await paired(main, native, name, 'initial', report, {
              firstRow:[cardAt(main, 0), cardAt(native, 0)],
              toggle:[main.targets.toggles.first(), native.targets.toggles.first()],
              summary:[main.targets.toggles.first().locator(':scope > div'),
                native.targets.toggles.first().locator(':scope > div')],
              firstActor:[main.targets.actors.first(), native.targets.actors.first()],
              topbarChevron:[main.targets.topbar.locator('svg').first(), native.targets.topbar.locator('svg').first()],
              entityIcon:[main.targets.toggles.first().locator('svg').first(),
                native.targets.toggles.first().locator('svg').first()],
            });

            const diffIndex = loaded.findIndex(item => item.old_value?.includes('OLD_DIFF_MARKER'));
            const fullIndex = loaded.findIndex(item => item.old_value === 'OLD_FULL_MARKER');
            assert.ok(diffIndex >= 0 && fullIndex >= 0, 'Seed the diff and full-value marker events in the first page.');
            for (const item of [main, native]) {
              await item.targets.toggles.nth(diffIndex).click();
              const details = record(cardAt(item, diffIndex));
              await details.getByText('When', {exact:true}).waitFor();
              await details.getByText('OLD_DIFF_MARKER', {exact:true}).waitFor();
              await details.getByText('NEW_DIFF_MARKER', {exact:true}).waitFor();
              assert.ok(await details.getByText(/\d+ unchanged lines/).count() >= 2, 'Both long context runs fold.');
              const actorIndex = actors.findIndex(actor => actor.actor_user_id === loaded[diffIndex].actor_user_id);
              assert.ok((await details.textContent()).includes(`${actorIndex + 1}${actorIndex === 0 ? 'st' : actorIndex === 1 ? 'nd' : actorIndex === 2 ? 'rd' : 'th'} most active`));
              assert.ok((await details.textContent()).includes('UTC'));
              assert.ok((await details.textContent()).includes(`via ${loaded[diffIndex].transport}`));
            }
            const mainDiff = record(cardAt(main, diffIndex)).locator('div[class~="max-h-[320px]"]');
            const nativeDiff = native.page.locator('.native-project-activity__diff');
            const diffRows = nodes => nodes.map(node => {
              const spans = Array.from(node.querySelectorAll(':scope > span'));
              return spans.length === 3 ? {fold:spans[1].textContent.trim()}
                : {marker:spans[0].textContent.trim(), text:spans[1].textContent};
            });
            assert.deepEqual(await nativeDiff.locator(':scope > div').evaluateAll(diffRows),
              await mainDiff.locator(':scope > div').evaluateAll(diffRows));
            await paired(main, native, name, 'diff', report, {
              record:[record(cardAt(main, diffIndex)), record(cardAt(native, diffIndex))],
              diff:[mainDiff, nativeDiff],
              whatIcon:[record(cardAt(main, diffIndex)).locator('p').filter({has:main.page.locator('svg[width="13"]')}).locator('svg').first(),
                record(cardAt(native, diffIndex)).locator('p').filter({has:native.page.locator('svg[width="13"]')}).locator('svg').first()],
            });
            for (const item of [main, native]) {
              await item.targets.toggles.nth(fullIndex).click();
              assert.equal(await expandedCount(item), 1, 'Opening another row closes the previous row.');
              const details = record(cardAt(item, fullIndex));
              await details.getByText('OLD_FULL_MARKER', {exact:true}).waitFor();
              await details.getByText('NEW_FULL_MARKER', {exact:true}).waitFor();
              assert.equal(await details.locator('div[class~="max-h-[320px]"]').count(), 0);
            }
            await paired(main, native, name, 'full-values', report, {
              record:[record(cardAt(main, fullIndex)), record(cardAt(native, fullIndex))],
            });

            for (let index = 0; index < actors.length; index++) {
              const filtered = loaded.filter(entry => entry.actor_user_id === actors[index].actor_user_id);
              assert.ok(filtered.length > 0, 'Interleave every actor bucket into the first page.');
              for (const item of [main, native]) {
                await item.targets.actors.nth(index).click();
                await expectRows(item, filtered.length);
                assert.equal((await item.targets.count.textContent()).trim(), String(filtered.length));
                assert.equal(await expandedCount(item), 0, 'Selecting an actor clears expansion.');
                assert.equal(await item.page.getByRole('button', {name:'Load more', exact:true}).count(), 0);
                assert.ok((await item.targets.topbar.textContent()).includes(`${actorName(actors[index])} only`));
                if (prefix === '' && theme === 'light') {
                  await resetScroll(item);
                  await capture(item, `${name}-filter-${index}-${item.native ? 'native' : 'main'}.png`);
                }
                await item.targets.toggles.first().click();
                await record(cardAt(item, 0)).getByText('When', {exact:true}).waitFor();
                await item.page.getByTitle('Clear actor filter', {exact:true}).click();
                await expectRows(item, 50);
                assert.equal(await expandedCount(item), 1, 'Clearing the chip preserves the expanded record.');
                assert.equal((await item.targets.count.textContent()).trim(), '50+');
                const opened = loaded.findIndex(entry => entry.id === filtered[0].id);
                await item.targets.toggles.nth(opened).click();
                await item.page.waitForFunction(native => document.querySelectorAll(native
                  ? '.native-project-activity__feed [data-activity-expanded="true"]'
                  : 'div[class~="pb-3.5"]').length === 0, item.native);
                assert.equal(await expandedCount(item), 0);
                await item.targets.actors.nth(index).click();
                await expectRows(item, filtered.length);
                await item.targets.toggles.first().click();
                await record(cardAt(item, 0)).getByText('When', {exact:true}).waitFor();
                await item.targets.actors.nth(index).click();
                await expectRows(item, 50);
                assert.equal(await expandedCount(item), 0, 'Clicking the active actor clears both filter and expansion.');
                assert.equal(await item.page.getByTitle('Clear actor filter', {exact:true}).count(), 0);
              }
            }
            for (const item of [main, native]) {
              await item.page.getByRole('button', {name:'Load more', exact:true}).click();
              await expectRows(item, initial.items.length);
              assert.equal((await item.targets.count.textContent()).trim(), String(initial.items.length));
              assert.equal(await item.page.getByRole('button', {name:'Load more', exact:true}).count(), 0);
            }
            const retained = await native.targets.feed.locator('[data-activity-id]').evaluateAll(nodes => nodes.map(node => node.dataset.activityId));
            assert.deepEqual(retained, initial.items.map(item => String(item.id)));
            const refresh = await fetch(origin + '/__native_project_activity_test/refresh', {
              method:'POST', headers:{cookie:`lific_token=${token}`, origin, 'content-type':'application/json'}, body:'[]',
            });
            assert.equal(refresh.status, 200, 'The fixture inserts and publishes one real project event.');
            // Wait before focus: a real project publication must prepend the row.
            try {
              await expectRows(native, initial.items.length + 1);
            } catch (error) {
              const state = await native.page.evaluate(() => {
                const owner = document.querySelector('[data-native-project-activity]');
                return {rows:document.querySelectorAll('.native-project-activity__row-toggle').length,
                  publications:document.querySelectorAll('[data-native-project-activity-events]').length,
                  run:typeof owner?.nativeActivityRun, schedule:typeof owner?.nativeActivitySchedule,
                  hidden:document.hidden, renderErrors:window.activityRenderErrors};
              });
              fs.writeFileSync(path.join(output, `${name}-realtime-failure.json`), JSON.stringify({state,
                errors, websocketEvents, pendingRequests:[...pendingRequests].map(request => ({
                  method:request.method(), url:request.url(), resourceType:request.resourceType()}))}, null, 2));
              await capture(native, `${name}-realtime-failure-native.png`);
              throw error;
            }
            for (const item of [main, native]) {
              try {
                await expectRows(item, initial.items.length + 1);
              } catch (error) {
                fs.writeFileSync(path.join(output, `${name}-main-realtime-failure.json`),
                  JSON.stringify({errors:mainErrors, websocketEvents:mainWebsocketEvents,
                    state:await main.page.evaluate(() => ({hidden:document.hidden,
                      rows:document.querySelectorAll('[role="button"][tabindex="0"]').length}))}, null, 2));
                await capture(main, `${name}-realtime-failure-main.png`);
                throw error;
              }
              assert.equal((await item.targets.count.textContent()).trim(), String(initial.items.length + 1));
              assert.ok((await item.targets.toggles.first().textContent()).includes('NEW_ACTIVITY_MARKER'));
              await item.page.evaluate(() => window.dispatchEvent(new Event('focus')));
            }
            await settleScroll(native.page);
            const refreshed = await native.targets.feed.locator('[data-activity-id]').evaluateAll(nodes => nodes.map(node => node.dataset.activityId));
            assert.equal(new Set(refreshed).size, refreshed.length, 'Refresh never duplicates retained rows.');
            assert.deepEqual(refreshed.slice(1), retained, 'Refresh retains older loaded rows in order.');
            await paired(main, native, name, 'refreshed', report);
            if (prefix === '' && theme === 'light') {
              const frames = [['HIDE', "Couldn't load activity"], ['MISSING', "Couldn't load activity"]];
              if (projects.some(project => project.identifier === 'EMP')) frames.unshift(['EMP', 'No activity yet']);
              for (const [identifier, title] of frames) {
                await main.page.goto(`${mainOrigin}/#/${identifier}/activity`);
                await native.page.goto(`${proxy.origin}${prefix}/${identifier}/activity`);
                const expected = main.page.getByText(title, {exact:true});
                const actual = native.page.getByText(title, {exact:true});
                await expected.waitFor(); await actual.waitFor();
                const entry = {name, state:identifier, main:{title:await expected.evaluate(measure)},
                  native:{title:await actual.evaluate(measure)}};
                if (identifier === 'EMP') {
                  entry.main.historyIcon = await expected.locator('..').locator('svg[width="32"]').evaluate(measure);
                  entry.native.historyIcon = await actual.locator('..').locator('svg[width="32"]').evaluate(measure);
                }
                report.push(entry);
                await capture(main, `${name}-${identifier}-main.png`);
                await capture(native, `${name}-${identifier}-native.png`);
                for (const key of Object.keys(entry.main)) {
                  compare(entry.native[key], entry.main[key], `${name}.${identifier}.${key}`);
                }
              }
            }
            assert.deepEqual(errors, []);
            assert.deepEqual(requests.filter(request => /(?:^|\/)api\//.test(request.path)), [],
              'Native Activity reads through Rust framework transport.');
            assert.deepEqual(requests.filter(request => request.method === 'POST'
              && !request.path.startsWith(`${prefix}/__native_`) && !request.path.startsWith(`${prefix}/__topcoat`)), [],
              'Native Activity sends only framework POSTs.');
          } finally {
            await main.context.close(); await native.context.close(); await proxy.close();
          }
        }
      }
    }
  } finally {
    fs.writeFileSync(path.join(output, 'geometry.json'), JSON.stringify(report, null, 2));
    await browser.close();
    for (const socket of sockets) socket.destroy();
    await closeOriginalVite(vite);
    fs.rmSync(cache, {recursive:true, force:true});
  }
});
