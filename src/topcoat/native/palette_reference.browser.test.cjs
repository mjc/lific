// Real production Home and framework transport; no mocked search responses.
// Destination document requests are held after proving browser navigation,
// so this palette slice does not claim the still-separate issue/login ports.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const {launchBrowser, mountedProxy} = require('./browser_fixture.cjs');

const [origin, token, scenario, otherToken] = process.argv.slice(2);
const screenshotRoot = path.join(os.tmpdir(), 'lific-native-palette-reference');

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
        await open();
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
            await input.fill(query);
            await hit('SEC', 1).waitFor();
            await hit('ACC', 1).waitFor();
            assert.deepEqual(await issueRows().evaluateAll(rows => rows.map(row => row.getAttribute('href'))),
              [`${prefix}/SEC/issues/SEC-1`, `${prefix}/ACC/issues/ACC-1`],
              'Home has no current project; even /ACC as a mount keeps personal catalog order.');
          }
          await input.press('ArrowUp');
          await input.press('ArrowUp');
          await input.press('ArrowDown');
          await input.press('ArrowDown');
          await navigation(() => input.press('Enter', {noWaitAfter: true}), '/ACC/issues/ACC-1');
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
        // These assertions stop at the held destination document. Its feature
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
        failures.push(`${scenario} ${prefix || '/'}: ${error.stack}\nBrowser errors: ${JSON.stringify(errors)}\nRequest failures: ${JSON.stringify(requestFailures)}`);
        if (page) await page.screenshot({path: path.join(screenshotRoot, `${scenario}-${prefix.slice(1) || 'root'}-failed.png`), fullPage: true}).catch(() => {});
      } finally {
        releaseNavigation?.();
        try {await context.close();} finally {await proxy.close();}
      }
    }
  } finally {
    await browser.close();
  }
  assert.deepEqual(failures, []);
})().catch(error => {console.error(error); process.exitCode = 1;});
