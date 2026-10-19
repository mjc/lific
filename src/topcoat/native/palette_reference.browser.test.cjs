// Real production Home and framework transport; no mocked search responses.
// Destination document requests are held or aborted after recording real
// navigation; this palette slice does not claim the separate issue/login ports.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const {launchBrowser, mountedProxy} = require('./browser_fixture.cjs');
const {paletteTransport} = require('./palette_transport_fixture.cjs');

const [origin, token, scenario, otherToken] = process.argv.slice(2);
const screenshotRoot = path.join(os.tmpdir(), 'lific-native-palette-reference');
const progress = async (prefix, stage) => fs.appendFile(
  path.join(screenshotRoot, `${scenario}-progress.jsonl`),
  `${JSON.stringify({time: new Date().toISOString(), prefix, stage})}\n`,
);

async function waitForPromise(promise, message) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {timer = setTimeout(() => reject(new Error(message)), 10000);}),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

(async () => {
  await fs.mkdir(screenshotRoot, {recursive: true});
  const browser = await launchBrowser();
  const failures = [];
  try {
    for (const prefix of ['', '/app', '/ACC']) {
      const proxy = await mountedProxy(new URL(origin), prefix);
      const context = await browser.newContext({viewport: {width: 1280, height: 900}});
      const errors = [], requestFailures = [], urls = [], headerChecks = [];
      let releaseNavigation;
      let page;
      try {
        await progress(prefix, 'context-start');
        await context.addCookies([{name: 'lific_token', value: token, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
        context.on('request', request => {
          urls.push(request.url());
          if (request.method() === 'POST') {
            headerChecks.push(request.allHeaders().then(headers => {
              assert.equal(headers.authorization, undefined, 'Native palette requests never use a bearer token.');
            }));
          }
        });
        page = await context.newPage();
        const transport = ['modified-pending', 'stale', 'disposed'].includes(scenario)
          ? await paletteTransport(page, prefix) : undefined;
        page.setDefaultTimeout(7000);
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => {if (message.type() === 'error') errors.push(message.text());});
        page.on('requestfailed', request => requestFailures.push({url: request.url(), error: request.failure()?.errorText}));
        const home = `${proxy.origin}${prefix}/`;
        await page.goto(home, {waitUntil: 'domcontentloaded'});
        await page.locator('#main-content').waitFor();
        const dialog = page.getByRole('dialog', {name: 'Jump to project', exact: true});
        const input = page.locator('#native-home-palette-query');
        const results = page.locator('.native-home-palette-results');
        const issueRows = () => results.locator('a[href*="/issues/"]');
        const openingFocus = [];
        const open = async () => {
          await page.locator('.tc-native-home__page[data-native-home-connected="true"]').waitFor();
          await page.waitForFunction(() => Boolean(document.getElementById('native-home-date')?.textContent.trim()));
          await page.locator('#native-home-palette-open').click();
          await dialog.waitFor();
          await page.locator('.native-home-palette-results[data-native-home-connected="true"]').waitFor();
          // Record before fill() focuses the input. Assert after reference
          // behavior so a missing row is the initial consumer RED boundary.
          openingFocus.push(await input.evaluate(element => element === document.activeElement));
        };
        const hit = (project, number) => results.locator(`a[href="${prefix}/${project}/issues/${project}-${number}"]`);
        const navigation = async (action, expected) => {
          await page.screenshot({path: path.join(screenshotRoot, `${scenario}-${prefix.slice(1) || 'root'}.png`), fullPage: true});
          let resolveNavigation;
          const requested = new Promise(resolve => {resolveNavigation = resolve;});
          const gate = new Promise(resolve => {releaseNavigation = resolve;});
          await page.route('**/*', async route => {
            const request = route.request();
            const url = new URL(request.url());
            if (request.isNavigationRequest() && (/\/issues\//.test(url.pathname) || url.pathname === `${prefix}/login`)) {
              resolveNavigation(url);
              await gate;
              await route.abort().catch(() => {}); // Cleanup closes the held destination document.
            } else {
              await route.continue();
            }
          });
          await action();
          const destination = await waitForPromise(requested, `No palette navigation to ${expected}`);
          assert.equal(destination.origin, proxy.origin);
          assert.equal(destination.pathname, `${prefix}${expected}`, 'The destination is mounted exactly once.');
        };
        const newTab = async (action, expected) => {
          let resolveRequest, resolvePage;
          const requested = new Promise(resolve => {resolveRequest = resolve;});
          const createdPages = [];
          const created = new Promise(resolve => {resolvePage = resolve;});
          const onPage = target => {createdPages.push(target); resolvePage(target);};
          context.on('page', onPage);
          const matches = url => url.pathname.includes('/issues/');
          const intercept = async route => {
            const request = route.request();
            if (request.isNavigationRequest()) {
              await progress(prefix, 'new-tab-navigation-intercepted');
              // The first popup request can precede creation of its frame.
              // Keep the actual request and associate it after the real page event.
              resolveRequest({url: new URL(request.url()), request});
              // Playwright publishes a popup only after its first navigation
              // completes or aborts. Record the real canonical request, then
              // abort its separate detail document before it can load hybrid code.
              await route.abort();
              await progress(prefix, 'new-tab-initial-navigation-aborted');
            } else {
              await route.continue();
            }
          };
          await context.route(matches, intercept);
          let destination;
          let actionResult;
          try {
            await progress(prefix, 'new-tab-action-start');
            actionResult = Promise.resolve().then(action);
            destination = await waitForPromise(Promise.race([requested, actionResult.then(() => requested)]),
              'Modified Enter must request a real new-tab destination.');
            const createdPage = await waitForPromise(created, 'Modified Enter must create a real browser page.');
            destination.target = destination.request.frame().page();
            assert.equal(destination.target === createdPage, true, 'The captured navigation belongs to the actual newly created page.');
            await progress(prefix, destination.target === page ? 'new-tab-request-wrong-original-page' : 'new-tab-request-target-received');
            assert.equal(destination.target === page, false, 'Modified Enter opens a new tab instead of navigating Home.');
            await progress(prefix, 'new-tab-page-identity-verified');
            assert.equal(createdPages.length, 1, 'One modified Enter creates one popup.');
            assert.equal(context.pages().length, 2, 'Only Home and its one new tab are open.');
            await progress(prefix, 'new-tab-popup-count-verified');
            assert.equal(destination.url.origin, proxy.origin);
            assert.equal(destination.url.pathname, `${prefix}${expected}`, 'The new-tab destination is mounted exactly once.');
            await progress(prefix, 'new-tab-canonical-url-verified');
            // Playwright records a popup's initiator separately from the web
            // capability. Verify the actual window property, without diffing
            // Playwright's entire Page graph on a failed assertion.
            assert.equal(await destination.target.evaluate(() => window.opener === null), true,
              'Original new-tab navigation uses noopener.');
            await progress(prefix, 'new-tab-noopener-verified');
            assert.equal(page.url(), home, 'The original Home document remains active.');
            await progress(prefix, 'new-tab-home-unchanged');
            await dialog.waitFor({state: 'hidden'});
            await progress(prefix, 'new-tab-palette-closed');
          } finally {
            context.off('page', onPage);
            for (const target of createdPages) await target.close();
            await progress(prefix, 'new-tab-target-close-complete');
            await context.unroute(matches, intercept);
            await progress(prefix, 'new-tab-unroute-complete');
          }
          await actionResult;
          await progress(prefix, 'new-tab-action-complete');
        };
        const settle = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        const assertNoLateAction = async () => {
          await settle();
          assert.equal(page.url(), home, 'A retired search cannot navigate its owning Home.');
          assert.equal(context.pages().length, 1, 'A retired modified Enter cannot open a tab.');
          assert.equal(urls.some(url => new URL(url).pathname.includes('/issues/')), false,
            'Retired pending selection never requests an issue document.');
        };
        await open();
        await progress(prefix, 'palette-open');
        if (scenario === 'references') {
          for (const query of ['ACC1', 'acc 1', 'ACC-1', 'acc-001']) {
            await input.fill('');
            await page.waitForFunction(() => document.querySelectorAll('.native-home-palette-results a[href*="/issues/"]').length === 0);
            await input.fill(query);
            await hit('ACC', 1).waitFor();
            assert.equal(await issueRows().count(), 1);
            assert.match(await hit('ACC', 1).textContent(), /Visible active initial work/);
            assert.match(await hit('ACC', 1).textContent(), /Visible project/);
            assert.match(await hit('ACC', 1).textContent(), /ACC-1/);
          }
          for (const query of ['1', ' #1 ']) {
            const previousRevision = await results.getAttribute('data-native-palette-revision');
            await input.fill(query);
            // These queries return identical rows. Await this query's projection
            // before treating those rows as the ready keyboard result list.
            await page.waitForFunction(previous => {
              const revision = document.querySelector('.native-home-palette-results')
                ?.getAttribute('data-native-palette-revision');
              return Boolean(revision) && revision !== previous;
            }, previousRevision);
            await hit('SEC', 1).waitFor();
            await hit('ACC', 1).waitFor();
            assert.deepEqual(await issueRows().evaluateAll(rows => rows.map(row => row.getAttribute('href'))),
              [`${prefix}/SEC/issues/SEC-1`, `${prefix}/ACC/issues/ACC-1`],
              'Home has no current project; even /ACC as a mount keeps personal catalog order.');
          }
          for (const [key, project] of [
            ['ArrowUp', 'SEC'], ['ArrowUp', 'SEC'],
            ['ArrowDown', 'ACC'], ['ArrowDown', 'ACC'],
          ]) {
            await input.press(key);
            await results.locator(
              `a[data-native-palette-selected="true"][href="${prefix}/${project}/issues/${project}-1"]`,
            ).waitFor();
            assert.equal(await results.locator('a[data-native-palette-selected="true"]').count(), 1,
              `${key} selects exactly one row and clamps at the result-list boundary.`);
          }
          await navigation(() => input.press('Enter', {noWaitAfter: true}), '/ACC/issues/ACC-1');
        } else if (scenario === 'modified-ready') {
          for (const modifier of ['Control', 'Meta']) {
            await input.fill('ACC1');
            await hit('ACC', 1).waitFor();
            await newTab(() => input.press(`${modifier}+Enter`, {noWaitAfter: true}), '/ACC/issues/ACC-1');
            await open();
          }
        } else if (scenario === 'modified-pending') {
          for (const modifier of ['Control', 'Meta']) {
            const fragment = `href="${prefix}/ACC/issues/ACC-1"`;
            const pending = transport.hold(fragment);
            await input.fill('ACC1');
            const held = await waitForPromise(pending, 'The real reference result must arrive at the transport hold.');
            assert.equal(await issueRows().count(), 0, 'The held result has not reached the palette.');
            await newTab(async () => {
              await input.press(`${modifier}+Enter`, {noWaitAfter: true});
              assert.equal(context.pages().length, 1, 'Pending modified Enter waits for the real result.');
              assert.equal(held.release(), true, 'Release the actual server run into its still-active scope.');
            }, '/ACC/issues/ACC-1');
            await open();
          }
        } else if (scenario === 'stale') {
          const fragment = `href="${prefix}/ACC/issues/ACC-1"`;
          let pending = transport.hold(fragment);
          await input.fill('ACC1');
          let held = await waitForPromise(pending, 'Hold the actual older reference run.');
          await input.press('Control+Enter', {noWaitAfter: true});
          await input.fill('SEC1');
          await hit('SEC', 1).waitFor();
          assert.equal(held.release(), true);
          await waitForPromise(held.received, 'The older server snapshot must really reach the browser.');
          await assertNoLateAction();
          assert.equal(await hit('ACC', 1).count(), 0, 'The newer query is not overwritten by the late older run.');
          assert.equal(await hit('SEC', 1).count(), 1);
          assert.equal(await input.inputValue(), 'SEC1');
          pending = transport.hold(fragment);
          await input.fill('ACC1');
          held = await waitForPromise(pending, 'Hold the actual reference run before Escape.');
          await input.press('Meta+Enter', {noWaitAfter: true});
          await page.keyboard.press('Escape');
          await dialog.waitFor({state: 'hidden'});
          await open();
          await page.waitForFunction(() => document.querySelectorAll('.native-home-palette-results a[href*="/issues/"]').length === 0);
          assert.equal(held.release(), true);
          await waitForPromise(held.received, 'The pre-Escape server snapshot must really reach the reopened browser.');
          await assertNoLateAction();
          assert.equal(await input.inputValue(), '', 'Reopening keeps its reset query after the old result arrives.');
          assert.equal(await issueRows().count(), 0);
          assert.equal(await input.evaluate(element => element === document.activeElement), true);
        } else if (scenario === 'disposed') {
          const fragment = `href="${prefix}/ACC/issues/ACC-1"`;
          const pending = transport.hold(fragment);
          await input.fill('ACC1');
          const held = await waitForPromise(pending, 'Hold the actual old-account reference result.');
          await input.press('Control+Enter', {noWaitAfter: true});
          const other = await context.newPage();
          await other.goto(home, {waitUntil: 'domcontentloaded'});
          const committed = page.waitForEvent('framenavigated', frame => frame === page.mainFrame() && frame.url() === home);
          await context.addCookies([{name: 'lific_token', value: otherToken, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
          await other.evaluate(() => localStorage.setItem('lific_token', 'palette-account-replacement'));
          await waitForPromise(committed, 'The replacement account must retire the complete old Home scope.');
          await page.waitForLoadState('domcontentloaded');
          await page.locator('.native-home-account').filter({hasText: /^non_member$/}).waitFor();
          await waitForPromise(held.closed, 'The old palette transport closes with its owning Home.');
          assert.equal(held.release(), false, 'A disposed transport cannot deliver its captured old-account result.');
          await other.close();
          await open();
          await assertNoLateAction();
          assert.equal(await issueRows().count(), 0);
          assert.equal(await input.inputValue(), '');
          await input.fill('ACC1');
          await results.getByText('Nothing matches “ACC1”', {exact: true}).waitFor();
          assert.equal(await hit('ACC', 1).count(), 0, 'The replacement account cannot inherit the old account’s authorized row.');
        } else if (scenario === 'denied') {
          for (const query of ['HIDE-1', 'UNKNOWN-1', 'ACC-99', 'ACC-DOC-1', 'doc 1', 'accdoc1']) {
            await input.fill(query);
            await results.getByText(`Nothing matches “${query}”`, {exact: true}).waitFor();
            assert.equal(await issueRows().count(), 0);
            assert.equal(await dialog.getByText('Private hidden initial work', {exact: true}).count(), 0);
          }
        } else if (scenario === 'escape') {
          await input.fill('ACC1');
          await hit('ACC', 1).waitFor();
          await page.keyboard.press('Escape');
          await dialog.waitFor({state: 'hidden'});
          assert.equal(await page.locator('#native-home-palette-open').evaluate(element => element === document.activeElement), true);
          await open();
          assert.equal(await input.inputValue(), '', 'Original show() resets a reopened query.');
          await page.waitForFunction(() => document.querySelectorAll('.native-home-palette-results a[href*="/issues/"]').length === 0);
          assert.equal(page.url(), home, 'Escape does not navigate to an old result.');
        } else if (scenario === 'cookie') {
          await input.fill('ACC1');
          await hit('ACC', 1).waitFor();
          await context.addCookies([{name: 'lific_token', value: otherToken, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
          const reloaded = page.waitForRequest(request => request.isNavigationRequest() && request.url() === home);
          const committed = page.waitForEvent('framenavigated', frame => frame === page.mainFrame() && frame.url() === home);
          await input.fill('HIDE1');
          await waitForPromise(reloaded, 'A changed session owner must reload the complete private Home before consuming query results.');
          await waitForPromise(committed, 'The changed-session Home document must commit before reopening its palette.');
          await page.waitForLoadState('domcontentloaded');
          await page.locator('#main-content').waitFor();
          await page.locator('.native-home-account').filter({hasText: /^non_member$/}).waitFor();
          assert.equal(await page.getByRole('link', {name: 'Visible active initial work', exact: true}).count(), 0,
            'The replaced Home omits the previous account’s issue.');
          await open();
          await input.fill('HIDE1');
          await hit('HIDE', 1).waitFor();
          assert.match(await hit('HIDE', 1).textContent(), /Private hidden initial work/);
          await input.fill('ACC-1');
          await results.getByText('Nothing matches “ACC-1”', {exact: true}).waitFor();
          assert.equal(await hit('ACC', 1).count(), 0);
          await context.clearCookies();
          await navigation(() => input.fill('HIDE-1'), '/login');
        } else {
          throw new Error(`Unknown palette scenario: ${scenario}`);
        }
        // These assertions stop at the recorded destination request. Its feature
        // implementation is covered by its own route port, not this palette test.
        assert.ok(openingFocus.every(Boolean), 'Opening the initialized native palette focuses its query before browser input.');
        assert.equal(urls.some(url => /(^|\/)api(?:\/|$)/.test(new URL(url).pathname)), false,
          'Palette initialization, query, authorization and selection use no REST at any origin.');
        assert.ok(proxy.requests.every(request => !prefix || request.path.startsWith(`${prefix}/`)));
        assert.deepEqual(errors, [], 'No ignored hydration, callback or console errors.');
        assert.deepEqual(requestFailures, [], 'Palette assets and native requests all succeed.');
        await Promise.all(headerChecks);
        if (!releaseNavigation) {
          await page.screenshot({path: path.join(screenshotRoot, `${scenario}-${prefix.slice(1) || 'root'}.png`), fullPage: true});
        }
      } catch (error) {
        await progress(prefix, `failure: ${error.message}`);
        failures.push(`${scenario} ${prefix || '/'}: ${error.stack}\nBrowser errors: ${JSON.stringify(errors)}\nRequest failures: ${JSON.stringify(requestFailures)}`);
        if (page) await page.screenshot({path: path.join(screenshotRoot, `${scenario}-${prefix.slice(1) || 'root'}-failed.png`), fullPage: true, timeout: 3000}).catch(() => {});
        await progress(prefix, 'failure-screenshot-complete');
      } finally {
        releaseNavigation?.();
        await progress(prefix, 'context-close-start');
        try {await waitForPromise(context.close(), 'Palette browser context cleanup timed out.');}
        finally {await proxy.close();}
        await progress(prefix, 'context-close-complete');
      }
    }
  } finally {
    await browser.close();
  }
  assert.deepEqual(failures, []);
})().catch(error => {console.error(error); process.exitCode = 1;});
