// Original Layout.svelte at master 9683d38 is the shell contract.
// node home_shell.browser.test.cjs <fixture-origin> <token> <scenario> [pinned-master-web-directory] [hostile-project-name]
const {test} = require('node:test');
const {installOriginalFonts, captureOriginalFonts} = require('./original_fonts_fixture.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {tmpdir} = require('node:os');
const {mountedProxy, launchBrowser} = require('./browser_fixture.cjs');

const upstream = new URL(process.argv[2]), token = process.argv[3], scenario = process.argv[4];
const snapshot = process.argv[5];
const hostileProjectName = process.argv[6];
const output = '/tmp/lific-native-home-shell';
const destinations = ['Overview', 'Issues', 'Board', 'Graph', 'Modules', 'Pages', 'Files', 'Plans', 'Activity', 'Insights'];

async function evidence(page, proxy, name, requests, errors, networkFailures) {
  try {await page.screenshot({path: path.join(output, `${name}.png`), fullPage: true});}
  catch (error) {errors.push(`screenshot: ${error.message}`);}
  const mounts = await page.locator('[data-topcoat-on\\:mount]').evaluateAll(elements => elements.map(element => ({
    tag: element.tagName, source: element.getAttribute('data-topcoat-on:mount'),
  })));
  fs.writeFileSync(path.join(output, `${name}.json`), JSON.stringify({requests, errors, networkFailures, mounts,
    proxyRequests: proxy.requests, sockets: proxy.sockets}, null, 2));
}

function foreground(element) {
  const canvas = document.createElement('canvas'), context = canvas.getContext('2d');
  const style = getComputedStyle(element);
  context.fillStyle = element instanceof SVGElement ? style.stroke : style.color;
  context.fillRect(0, 0, 1, 1);
  return Array.from(context.getImageData(0, 0, 1, 1).data);
}

async function reference(browser, referenceOrigin, name, theme, viewport) {
  if (!referenceOrigin) return;
  const context = await browser.newContext({viewport, isMobile: viewport.width < 768, hasTouch: viewport.width < 768, colorScheme: theme, locale: 'en-US',
    timezoneId: 'America/Denver', reducedMotion: 'reduce'});
  try {
    await installOriginalFonts(context);
    await context.addInitScript(({token, theme}) => {
      localStorage.setItem('lific_token', token);
      localStorage.setItem('lific_theme', theme);
      localStorage.setItem('lific_motion', 'reduced');
    }, {token, theme});
    const page = await context.newPage();
    await page.clock.setFixedTime('2026-10-03T16:00:00Z');
    await page.goto(`${referenceOrigin}/#/`);
    await page.getByText('Visible active initial work', {exact: true}).waitFor();
    await page.evaluate(() => document.fonts.ready);
    await captureOriginalFonts(page,path.join(output,`${name}-original-fonts.json`));
    await page.screenshot({path: path.join(output, `${name}-original.png`), fullPage: true});
    const actionForeground = await page.getByRole('button', {name: 'New issue', exact: true}).evaluate(foreground);
    const activityAlign = await page.getByText('system', {exact: true}).first().evaluate(element => getComputedStyle(element).textAlign);
    const measured = await page.evaluate(() => {
      const main = document.querySelector('main'), panel = main.parentElement, rect = panel.getBoundingClientRect();
      return {x: rect.x, y: rect.y, right: rect.right, bottom: rect.bottom,
        radius: getComputedStyle(panel).borderTopLeftRadius, border: getComputedStyle(main).borderTopWidth,
        horizontal: document.documentElement.scrollWidth > innerWidth};
    });
    const original = {...measured, actionForeground, activityAlign};
    fs.writeFileSync(path.join(output, `${name}-original.json`), JSON.stringify(original, null, 2));
    return original;
  } finally {await context.close();}
}

test(`native Home original shell: ${scenario}`, async t => {
  assert.ok(['disclosure', 'geometry', 'mobile', 'mobile_lifetime', 'mobile_unavailable', 'mobile_search_history', 'hostile_project', 'preferences'].includes(scenario));
  if (scenario === 'hostile_project') assert.ok(hostileProjectName, 'The real database fixture supplies the exact hostile name.');
  fs.mkdirSync(output, {recursive: true});
  const browser = await launchBrowser();
  let vite, referenceOrigin, referenceCache;
  const proxySockets = new Set();
  try {
    if (snapshot && scenario === 'geometry') {
      const {createServer} = await import(pathToFileURL(path.join(snapshot, 'node_modules/vite/dist/node/index.js')).href);
      const configure = proxy => proxy.on('open', socket => {
        proxySockets.add(socket);
        socket.once('close', () => proxySockets.delete(socket));
      });
      referenceCache = fs.mkdtempSync(path.join(tmpdir(), 'lific-pinned-vite-'));
      vite = await createServer({cacheDir:referenceCache,root: snapshot, logLevel: 'silent', configFile: path.join(snapshot, 'vite.config.ts'), server: {
        host: '127.0.0.1', port: 0, strictPort: false, proxy: {
          '/api': {target: upstream.origin, ws: true, configure},
          '/public/api': {target: upstream.origin, ws: true, configure},
        },
      }});
      await vite.listen();
      referenceOrigin = `http://127.0.0.1:${vite.httpServer.address().port}`;
    }
    for (const prefix of ['', '/app', '/ACC']) await t.test(prefix || 'root', async t => {
      const modes = ['geometry', 'hostile_project'].includes(scenario)
        ? [['desktop', {width: 1440, height: 900}], ['phone', {width: 390, height: 844}]]
        : [[scenario.startsWith('mobile') ? 'phone' : 'desktop', scenario.startsWith('mobile') ? {width: 390, height: 844} : {width: 1440, height: 900}]];
      for (const [mode, viewport] of modes) for (const theme of scenario === 'geometry' ? ['light', 'dark'] : ['light']) {
        await t.test(`${mode}-${theme}`, async () => {
          const proxy = await mountedProxy(upstream, prefix);
          let context, page;
          const requests = [], errors = [], networkFailures = [];
          const name = `${scenario}-${prefix.slice(1) || 'root'}-${mode}-${theme}`;
          try {
            context = await browser.newContext({viewport, isMobile: mode === 'phone', hasTouch: mode === 'phone', colorScheme: theme, locale: 'en-US',
              timezoneId: 'America/Denver', reducedMotion: 'reduce'});
            await context.addCookies([{name: 'lific_token', value: token, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
            await context.addInitScript(theme => {
              if (localStorage.getItem('lific_theme') === null) localStorage.setItem('lific_theme', theme);
              globalThis.__nativeProjectInjected = false;
            }, theme);
            context.on('request', request => requests.push(request.url()));
            const maskDownloads = ['ChevronRight', 'Ellipsis'].map(name => new Promise(resolve => {
              context.on('requestfinished', request => {
                if (new URL(request.url()).pathname.endsWith(`/${name}.mask.svg`)) resolve();
              });
            }));
            context.on('requestfailed', request => networkFailures.push({url: request.url(), error: request.failure()?.errorText}));
            page = await context.newPage();
            page.setDefaultTimeout(7000);
            page.on('pageerror', error => errors.push(error.message));
            page.on('console', message => {if (message.type() === 'error') errors.push(message.text());});
            await page.clock.setFixedTime('2026-10-03T16:00:00Z');
            // A real prior document avoids depending on Chromium retaining its initial about:blank.
            const predecessor = `${proxy.origin}${prefix}/__native_home_shell_predecessor?nav-predecessor=${scenario}`;
            if (['mobile_unavailable', 'mobile_search_history'].includes(scenario)) {
              const response = await page.goto(predecessor);
              assert.equal(response.status(), 200);
              assert.equal(response.headers()['content-type'], 'text/html; charset=utf-8');
              assert.equal(await page.locator('link[rel="icon"]').getAttribute('href'), `${prefix}/favicon.png`);
            }
            assert.equal((await page.goto(`${proxy.origin}${prefix}/`)).status(), 200);
            await page.locator('[data-native-home-connected="true"]').first().waitFor();
            await page.getByText('Visible active initial work', {exact: true}).waitFor();
            await page.waitForFunction(selector=>{
              const icon=document.querySelector(selector);
              return icon && icon.getBBox().width>0 && icon.getBBox().height>0;
            },mode==='desktop'?'#native-home-collapse svg use':'#native-home-mobile-open svg use');
            if (mode === 'desktop') {
              await Promise.all(maskDownloads);
              await page.waitForFunction(() => ['.ns-project-toggle', '.ns-overflow'].every(selector => {
                const control = document.querySelector(`.native-home-sidebar ${selector}`);
                if (!control) return false;
                const style = getComputedStyle(control, '::before');
                const name = selector === '.ns-project-toggle' ? 'ChevronRight' : 'Ellipsis';
                return parseFloat(style.width) > 0 && parseFloat(style.height) > 0 && style.maskSize === 'contain'
                  && style.backgroundColor === style.color
                  && style.maskImage.includes(`${name}.mask.svg`);
              }));
              for (const [selector, name, size] of [['.ns-project-toggle', 'ChevronRight', 13], ['.ns-overflow', 'Ellipsis', 15]]) {
                const control = page.locator(`.native-home-sidebar ${selector}`).first();
                const style = await control.evaluate(element => {
                  const style = getComputedStyle(element, '::before');
                  return {mask: style.maskImage, width: parseFloat(style.width), height: parseFloat(style.height)};
                });
                assert.ok(style.mask.includes(`${proxy.origin}${prefix}/__native_icons/`) && style.mask.endsWith(`/${name}.mask.svg")`));
                assert.equal(style.width, size);
                assert.equal(style.height, size);
                assert.equal(await control.locator(':scope > :is(svg,.native-icon-mask)').count(), 0);
              }
            }
            const iconRequests=requests.filter(url=>new URL(url).pathname.includes('/__native_icons/'));
            assert.equal(new Set(iconRequests).size,iconRequests.length,
              'Repeated icon instances share one browser asset request.');
            assert.equal(await page.locator('[data-native-sidebar-layout="phone"]').count(),0,
              'Hydrated unopened Home does not eagerly request or render the phone project tree.');

            // Resolve the real catalog ID from the mounted overview link. Both
            // shared sidebar layouts expose that same ID on their native rows.
            let projectId;
            if (['disclosure', 'mobile', 'mobile_lifetime', 'mobile_unavailable', 'hostile_project'].includes(scenario)) {
              const overview = page.locator(`.native-home-sidebar a[data-ns-link][href="${prefix}/ACC/overview"]`);
              assert.equal(await overview.count(), 1);
              projectId = await overview.getAttribute('data-ns-link');
              assert.match(projectId, /^\d+$/);
            }
            const phoneProject = async (nav, name = 'Visible project') => {
              const row = nav.locator(`#native-sidebar-phone-project-${projectId}[data-native-project-trigger="ACC"]`);
              await row.waitFor();
              assert.equal(await row.count(), 1);
              assert.equal(await row.getAttribute('aria-label'), `Open ${name} navigation`);
              assert.equal(await row.textContent(), `AC${name}ACC`);
              assert.equal(await row.locator(':scope > span').nth(1).locator(':scope > span').textContent(), name);
              return row;
            };

            if (scenario === 'disclosure') {
              const project = page.locator(`[data-ns-project="${projectId}"]`);
              const toggle = project.getByRole('button', {name: 'Expand Visible project', exact: true});
              assert.equal(await toggle.count(), 1, 'Home has a project disclosure distinct from overview navigation.');
              assert.equal(await toggle.getAttribute('aria-expanded'), 'false');
              const controlled = await toggle.getAttribute('aria-controls');
              assert.ok(controlled);
              assert.equal(await page.locator(`#${controlled}`).isVisible(), false);
              await toggle.focus();
              await page.keyboard.press('Enter');
              assert.equal(await project.getByRole('button', {name: 'Collapse Visible project', exact: true}).getAttribute('aria-expanded'), 'true');
              await page.waitForFunction(() => {
                const icon = document.querySelector('.ns-project-toggle[aria-expanded=true]');
                if (!icon) return false;
                const rotation = new DOMMatrix(getComputedStyle(icon, '::before').transform);
                return Math.abs(rotation.a) < .01 && rotation.b > .99;
              }, undefined, {timeout: 5000});
              const links = page.locator(`#${controlled}`).getByRole('link');
              assert.deepEqual(await links.allTextContents(), destinations);
              for (const [index, slug] of destinations.map(label => label.toLowerCase()).entries()) {
                assert.equal(await links.nth(index).getAttribute('href'), `${prefix}/ACC/${slug}`);
              }
              const overview = project.locator(`a[data-ns-link="${projectId}"]`);
              assert.equal(await overview.textContent(), 'ACVisible project');
              assert.equal(await overview.getAttribute('href'), `${prefix}/ACC/overview`);
              assert.equal(await page.getByText('Private hidden project', {exact: true}).count(), 0);
              await page.getByRole('button', {name: 'Collapse sidebar', exact: true}).click();
              assert.equal(await page.locator('.native-home-sidebar').isVisible(), false);
              await page.getByRole('button', {name: 'Expand sidebar', exact: true}).click();
              assert.equal(await project.getByRole('button', {name: 'Collapse Visible project', exact: true}).getAttribute('aria-expanded'), 'true',
                'Folding the docked sidebar retains the live project tree.');
              await page.reload();
              await page.getByText('Visible active initial work', {exact: true}).waitFor();
              await page.getByRole('button', {name: 'Collapse sidebar', exact: true}).click();
              await page.reload();
              await page.getByRole('button', {name: 'Expand sidebar', exact: true}).waitFor();
            } else if (scenario === 'geometry') {
              const original = await reference(browser, referenceOrigin, name, theme, viewport);
              const brand = page.locator('.native-home-brand');
              if (mode === 'desktop') {
                assert.equal(await brand.getAttribute('href'), 'https://github.com/VoidNullable/lific');
                assert.equal(await brand.getAttribute('target'), '_blank');
                assert.equal(await brand.locator('img').getAttribute('src'), `${prefix}/logo.webp`);
                assert.equal(await brand.locator('img').getAttribute('width'), '26');
                assert.match(await brand.textContent(), /Lific.*v\d/s);
                assert.equal(await page.locator('.native-home-account').textContent(), 'viewer');
                assert.equal(await page.locator('.native-home-account-link').getAttribute('href'), `${prefix}/settings`);
                assert.equal(await page.locator('.native-home-avatar').textContent(), 'V');
              } else {
                assert.equal(await page.locator('.native-home-sidebar').isVisible(), false,
                  'Original phone Home hides the docked sidebar instead of squeezing it into a narrow column.');
                await page.getByRole('button', {name: 'Open navigation', exact: true}).waitFor();
              }
              const geometry = await page.evaluate(() => {
                const main = document.querySelector('.native-home-panel'), style = getComputedStyle(main), rect = main.getBoundingClientRect();
                return {x: rect.x, y: rect.y, right: rect.right, bottom: rect.bottom,
                  border: style.borderTopWidth, radius: style.borderTopLeftRadius,
                  horizontal: document.documentElement.scrollWidth > innerWidth};
              });
              assert.deepEqual(geometry, {x: mode === 'desktop' ? 230 : 0, y: mode === 'desktop' ? 36.796875 : 84.796875,
                right: viewport.width, bottom: viewport.height, border: '0px', radius: mode === 'desktop' ? '12px' : '0px', horizontal: false});
              assert.equal(await page.locator('.tc-native-home__greeting h1').evaluate(element => getComputedStyle(element).fontSize),
                '22px', 'Home title matches the original text-title typography.');
              assert.equal(await page.locator('#native-home-quick-jump kbd').textContent(), '⌘K',
                'Home keeps the original visible quick-jump keyboard hint.');
              const expectedForeground = original?.actionForeground || [20, 18, 16, 255];
              const appearance = {
                label: await page.locator('.tc-native-home__new').evaluate(foreground),
                icon: await page.locator('.tc-native-home__new svg').evaluate(foreground),
                activity: await Promise.all(['.tc-home-sections__activity', '.tc-home-sections__activity-text']
                  .map(selector => page.locator(selector).first().evaluate(element => getComputedStyle(element).textAlign))),
              };
              const activityAlign = original?.activityAlign || 'center';
              assert.deepEqual(appearance, {label: expectedForeground, icon: expectedForeground, activity: [activityAlign, activityAlign]},
                'Activity alignment and success label/icon colors match the original in both themes.');
              assert.equal(await page.locator('.native-home-shadow-top').count(), 1);
              assert.equal(await page.locator('.native-home-shadow-left').isVisible(), mode === 'desktop');
            } else if (scenario === 'mobile') {
              const open = page.getByRole('button', {name: 'Open navigation', exact: true});
              assert.equal(await open.count(), 1, 'Phone has the original navigation entry point.');
              await open.click();
              const nav = page.locator('[data-native-mobile-nav]');
              await nav.waitFor({state: 'visible'});
              assert.equal(await nav.isVisible(), true);
              const rect = await nav.boundingBox();
              assert.deepEqual([rect.x, rect.y, rect.width, rect.height], [0, 0, viewport.width, viewport.height]);
              const phoneRow=await phoneProject(nav);
              await phoneRow.evaluate(node=>{window.retainedNativePhoneRow=node;});
              await phoneRow.click();
              assert.equal(await nav.locator('[data-native-mobile-root]').isVisible(), false);
              await nav.locator('[data-native-mobile-project]:not([hidden])').waitFor({state: 'visible'});
              assert.deepEqual(await nav.locator('[data-native-mobile-project]').getByRole('link').allTextContents(), destinations);
              await page.keyboard.press('Escape');
              await nav.locator('[data-native-mobile-root]').waitFor({state: 'visible'});
              assert.equal(await nav.locator('[data-native-mobile-root]').isVisible(), true, 'Escape pops project detail before closing root.');
              await page.keyboard.press('Escape');
              await nav.waitFor({state: 'hidden'});
              await page.waitForFunction(() => document.getElementById('native-home-mobile-open') === document.activeElement);
              assert.equal(await nav.isVisible(), false);
              assert.equal(await open.evaluate(element => element === document.activeElement), true);
              await open.click();
              await nav.waitFor({state:'visible'});
              assert.equal(await (await phoneProject(nav)).evaluate(node=>node===window.retainedNativePhoneRow),true,
                'Reopening navigation keeps the initialized project row.');
              await page.keyboard.press('Escape');await nav.waitFor({state:'hidden'});
            } else if (scenario === 'mobile_lifetime') {
              const current = page.url();
              const phonePosts = [];
              page.on('request', request => {
                if (request.method() === 'POST' && new URL(request.url()).pathname === `${prefix}/__native_home/phone`) {
                  phonePosts.push(request.url());
                }
              });
              const open = page.getByRole('button', {name: 'Open navigation', exact: true});
              assert.equal(await page.locator('[data-native-mobile-nav]').count(), 0,
                'The unopened phone dialog is admitted on first use.');
              await open.click();
              const nav = page.getByRole('dialog', {name: 'Workspace navigation', exact: true});
              await nav.waitFor();
              assert.equal(await page.locator('.native-home-body').evaluate(element => element.inert), true,
                'Original phone navigation isolates the background while its owned modal is open.');
              const first = nav.getByRole('button', {name: 'Close navigation', exact: true});
              await page.waitForFunction(() => document.querySelector('[data-native-mobile-root] button') === document.activeElement);
              await nav.evaluate(element => { element.__retainedPhoneChrome = true; });
              const last = nav.getByRole('button', {name: 'Choose theme, current: light', exact: true});
              await first.focus();
              await page.keyboard.press('Shift+Tab');
              assert.equal(await last.evaluate(element => element === document.activeElement), true);
              await page.keyboard.press('Tab');
              assert.equal(await first.evaluate(element => element === document.activeElement), true);
              await page.evaluate(() => document.getElementById('main-content').focus());
              assert.equal(await nav.evaluate(element => element.contains(document.activeElement)), true);
              const project = await phoneProject(nav);
              await project.click();
              const pane = nav.locator('[data-native-mobile-project]:not([hidden])');
              const back = pane.getByRole('button', {name: 'Back to projects', exact: true});
              await pane.getByRole('link', {name: 'Insights', exact: true}).focus();
              await page.keyboard.press('Tab');
              assert.equal(await back.evaluate(element => element === document.activeElement), true);
              await page.keyboard.press('Escape');
              assert.equal(await project.evaluate(element => element === document.activeElement), true,
                'Popping the project pane returns focus to its original row.');
              await page.goBack();
              await nav.waitFor({state: 'hidden'});
              assert.equal(await page.locator('[data-native-mobile-nav]').count(), 1,
                'The initialized phone dialog survives close and history reopen.');
              assert.equal(page.url(), current, 'Owned Back closes the root instead of leaving Home.');
              assert.equal(await page.locator('.native-home-body').evaluate(element => element.inert), false);
              assert.equal(await open.evaluate(element => element === document.activeElement), true);
              await page.goForward();
              await nav.waitFor();
              assert.equal(await nav.evaluate(element => element.__retainedPhoneChrome), true,
                'History reopen retains the admitted dialog and its subordinate owners.');
              assert.equal(page.url(), current, 'Forward restores the owned root navigation entry.');
              await (await phoneProject(nav)).click();
              await page.goBack();
              await nav.locator('[data-native-mobile-root]').waitFor();
              assert.equal(page.url(), current);
              await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
              assert.equal(phonePosts.length, 1,
                'Project selection and owned Back/Forward retain the admitted phone scope without another parent render.');
              await page.setViewportSize({width: 1440, height: 900});
              await nav.waitFor({state: 'hidden'});
              assert.equal(await page.locator('.native-home-sidebar').isVisible(), true);
              assert.equal(await page.locator('.native-home-body').evaluate(element => element.inert), false);
              await page.goForward();
              await page.waitForFunction(() => !document.querySelector('.native-home-body').inert);
              assert.equal(await nav.isVisible(), false,
                'Desktop owner ignores obsolete phone Forward entries.');
              assert.equal(page.url(), current);
            } else if (scenario === 'mobile_unavailable') {
              const current = page.url();
              await page.getByRole('button', {name: 'Open navigation', exact: true}).click();
              const nav = page.getByRole('dialog', {name: 'Workspace navigation', exact: true});
              await (await phoneProject(nav)).click();
              const pane = await page.evaluate(() => {
                const entry = history.state.lificNativeHomeNav;
                history.replaceState({...history.state, lificNativeHomeNav: {...entry, project: 'HIDE'}}, '');
                return entry.pane;
              });
              assert.equal(pane, 'project', 'Restore a genuine owned project entry, changing only its unavailable identifier.');
              await page.reload();
              await page.getByText('Visible active initial work', {exact: true}).waitFor();
              // Pinned MobileNav.svelte resolves history identifiers against live visible projects.
              await nav.getByRole('heading', {name: 'Project unavailable', exact: true}).waitFor();
              assert.equal(await nav.getByText('This project is no longer in your project list.', {exact: true}).isVisible(), true);
              assert.equal(await nav.locator('[data-native-mobile-root]').isVisible(), false);
              assert.equal(await page.getByText('Private hidden project', {exact: true}).count(), 0);
              assert.equal(await page.getByText('Private hidden initial work', {exact: true}).count(), 0);
              assert.equal(await nav.locator(`a[href^="${prefix}/HIDE/"]`).count(), 0);
              assert.equal(await page.locator('.native-home-body').evaluate(element => element.inert), true);
              const back = nav.getByRole('button', {name: 'Back to projects', exact: true});
              const close = nav.getByRole('button', {name: 'Close navigation', exact: true});
              await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'Back to projects');
              await page.keyboard.press('Shift+Tab');
              assert.equal(await close.evaluate(element => element === document.activeElement), true);
              await page.keyboard.press('Tab');
              assert.equal(await back.evaluate(element => element === document.activeElement), true);
              await back.click();
              await phoneProject(nav);
              assert.equal(await nav.getByRole('heading', {name: 'Project unavailable', exact: true}).isVisible(), false);
              assert.equal(page.url(), current);
              await page.goForward();
              await nav.getByRole('heading', {name: 'Project unavailable', exact: true}).waitFor();
              await close.click();
              await nav.waitFor({state: 'hidden'});
              assert.equal(page.url(), current, 'Closing unavailable project detail unwinds both owned panes to Home.');
              await page.waitForFunction(() => !document.querySelector('.native-home-body').inert);
              await page.goBack();
              await page.waitForURL(predecessor);
            } else if (scenario === 'mobile_search_history') {
              const current = page.url();
              const open = page.getByRole('button', {name: 'Open navigation', exact: true});
              const nav = page.getByRole('dialog', {name: 'Workspace navigation', exact: true});
              for (let round = 0; round < 3; round++) {
                if (round) {
                  await page.goForward();
                  await page.waitForURL(current);
                  await page.getByText('Visible active initial work', {exact: true}).waitFor();
                  assert.equal(await nav.isVisible(), false, 'Forward restores closed Home after Search unwinds to its base.');
                }
                await open.click();
                await nav.getByRole('button', {name: 'Search issues, pages, projects…', exact: true}).click();
                const palette = page.getByRole('dialog', {name: 'Jump to project', exact: true});
                await palette.waitFor();
                assert.equal(await nav.isVisible(), false);
                assert.equal(await page.locator('.native-home-body').evaluate(element => element.inert), false);
                if (round === 2) {
                  // Original CommandPalette.onWindowKeydown closes root search on Escape.
                  await page.keyboard.press('Escape');
                  await palette.waitFor({state: 'hidden', timeout: 7000});
                  assert.equal(await open.evaluate(element => element === document.activeElement), true,
                    'Escape returns focus to the visible phone navigation opener after Search.');
                } else {
                  await palette.getByRole('button', {name: 'Close project search', exact: true}).click();
                  await palette.waitFor({state: 'hidden'});
                }
                await page.goBack();
                // controller.close(onOpenPalette) releases Search only after reaching the owned base.
                await page.waitForURL(predecessor);
              }
            } else if (scenario === 'hostile_project') {
              const assertNoAttacker = async () => {
                assert.equal(await page.locator('img[src="/native-hostile-project"], script:not([src])').count(), 0,
                  'Hostile project markup never creates attacker elements.');
                assert.equal(await page.evaluate(() => globalThis.__nativeProjectInjected), false);
                assert.equal(requests.some(url => new URL(url).pathname.endsWith('/native-hostile-project')), false);
                assert.deepEqual(errors, [], 'Hostile labels compile without ignored hydration/console errors.');
              };
              const assertLinks = async container => {
                const links = container.getByRole('link');
                assert.deepEqual(await links.allTextContents(), destinations);
                for (const [index, destination] of destinations.entries()) {
                  assert.equal(await links.nth(index).getAttribute('href'), `${prefix}/ACC/${destination.toLowerCase()}`);
                }
              };
              for (let hydration = 0; hydration < 2; hydration++) {
                if (hydration) {
                  await page.reload();
                  await page.getByText('Visible active initial work', {exact: true}).waitFor();
                }
                if (mode === 'desktop') {
                  const project = page.locator(`[data-ns-project="${projectId}"]`);
                  const title = project.locator(`a[data-ns-link="${projectId}"]`);
                  assert.equal(await title.textContent(), `AC${hostileProjectName}`);
                  assert.equal(await title.locator('span').last().textContent(), hostileProjectName);
                  assert.equal(await title.getAttribute('title'), hostileProjectName);
                  assert.equal(await title.getAttribute('href'), `${prefix}/ACC/overview`);
                  const toggle = project.getByRole('button', {name: `Expand ${hostileProjectName}`, exact: true});
                  assert.equal(await toggle.getAttribute('aria-label'), `Expand ${hostileProjectName}`);
                  await toggle.click();
                  const collapse = project.getByRole('button', {name: `Collapse ${hostileProjectName}`, exact: true});
                  assert.equal(await collapse.getAttribute('aria-expanded'), 'true');
                  await assertLinks(page.locator(`#${await collapse.getAttribute('aria-controls')}`));
                } else {
                  const nav = page.getByRole('dialog', {name: 'Workspace navigation', exact: true});
                  if (!hydration) {
                    await page.getByRole('button', {name: 'Open navigation', exact: true}).click();
                    const row = await phoneProject(nav, hostileProjectName);
                    await row.click();
                  }
                  const pane = nav.locator('[data-native-mobile-project]:not([hidden])');
                  await pane.waitFor();
                  assert.equal(await pane.locator('strong').textContent(), hostileProjectName);
                  await assertLinks(pane);
                }
                await assertNoAttacker();
              }
              assert.equal(await page.getByText('Private hidden project', {exact: true}).count(), 0);
            } else {
              const chooser = page.getByRole('button', {name: 'Choose theme, current: light', exact: true});
              assert.equal(await chooser.count(), 1, 'The original account footer owns the theme preference chooser.');
              await chooser.click();
              const menu = page.getByRole('menu', {name: 'Theme'});
              assert.deepEqual(await menu.getByRole('menuitemradio').allTextContents(), ['Light', 'Dark', 'System']);
              await menu.getByRole('menuitemradio', {name: 'Dark', exact: true}).click();
              assert.equal(await page.evaluate(() => localStorage.getItem('lific_theme')), 'dark');
              assert.equal(await page.evaluate(() => getComputedStyle(document.documentElement).colorScheme), 'dark');
              await page.reload();
              await page.getByRole('button', {name: 'Choose theme, current: dark', exact: true}).click();
              await page.getByRole('menuitemradio', {name: 'System', exact: true}).click();
              assert.equal(await page.evaluate(() => localStorage.getItem('lific_theme')), null);
              await page.emulateMedia({colorScheme: 'dark'});
              await page.waitForFunction(() => getComputedStyle(document.documentElement).colorScheme === 'dark');
              await page.emulateMedia({colorScheme: 'light'});
              await page.waitForFunction(() => getComputedStyle(document.documentElement).colorScheme === 'light');
              const other = await context.newPage();
              await other.goto(`${proxy.origin}${prefix}/`);
              await other.evaluate(() => localStorage.setItem('lific_theme', 'dark'));
              await page.getByRole('button', {name: 'Choose theme, current: dark', exact: true}).waitFor();
            }
            assert.equal(requests.some(url => new URL(url).pathname.split('/').includes('api')), false, 'Native shell state and preferences never call REST.');
            assert.deepEqual(errors, []);
          } finally {
            try {if (page) await evidence(page, proxy, name, requests, errors, networkFailures);}
            finally {try {if (context) await context.close();} finally {await proxy.close();}}
          }
        });
      }
    });
  } finally {
    for (const socket of proxySockets) socket.destroy();
    try {await browser.close();}
    finally {try {if (vite) await vite.close();}
      finally {if (referenceCache) fs.rmSync(referenceCache,{recursive:true,force:true});}}
  }
});
