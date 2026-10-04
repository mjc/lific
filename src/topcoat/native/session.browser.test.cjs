// Real native Home; Rust owns the disposable database and revocation controls.
// node session.browser.test.cjs <fixture-origin> <viewer-token> <fixture-json>
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {mountedProxy, launchBrowser} = require('./browser_fixture.cjs');

const upstream = new URL(process.argv[2]);
const viewerToken = process.argv[3];
const fixture = JSON.parse(process.argv[4]);

const settle = page => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
const cookie = (origin, value) => ({name: 'lific_token', value, url: origin, httpOnly: true, sameSite: 'Lax'});

async function privatePage(browser, proxy, prefix, token) {
  const context = await browser.newContext();
  await context.addCookies([cookie(proxy.origin, token)]);
  const page = await context.newPage(), errors = [], privateRequests = [], frames = [];
  page.setDefaultTimeout(10000);
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => {
    if (message.type() === 'error' && message.text().startsWith('[topcoat]')) errors.push(message.text());
  });
  // Login's own implementation is a separate surface. Record requests while
  // private Home is active, including every session refresh and render.
  context.on('request', request => {
    if (new URL(page.url()).pathname === `${prefix}/`) privateRequests.push(request.url());
  });
  page.on('websocket', socket => {
    socket.on('framereceived', frame => frames.push(JSON.parse(frame.payload.toString())));
  });
  await page.goto(`${proxy.origin}${prefix}/`);
  await page.locator('[data-native-home-connected="true"]').first().waitFor();
  await page.getByText('Visible active initial work', {exact: true}).waitFor();
  return {context, page, errors, privateRequests, frames};
}

function assertNativeOnly(state) {
  assert.ok(state.privateRequests.every(url => !new URL(url).pathname.split('/').includes('api')),
    'Private session lifecycle and native reads never call REST.');
  assert.deepEqual(state.errors, [], 'Session outcomes remain visible instead of becoming framework console errors.');
}

test('native session discovers account replacements under the current cookie and redirects revoked reads', async t => {
  const browser = await launchBrowser();
  try {
    for (const [index, prefix] of ['', '/app', '/ACC'].entries()) {
      let replacementVerified = false;
      await t.test(prefix || 'root', async t => {
        const proxy = await mountedProxy(upstream, prefix);
        try {
          await t.test('another tab replaces the session and storage clear reloads exactly once', async () => {
            const state = await privatePage(browser, proxy, prefix, viewerToken);
            try {
              const {page, context} = state;
              const other = await context.newPage();
              await other.goto(`${proxy.origin}${prefix}/`);
              await page.evaluate(() => {
                window.sessionStorageEvents = 0;
                window.addEventListener('storage', () => {window.sessionStorageEvents += 1;});
              });
              const documentRequests = () => proxy.requests.filter(request => request.method === 'GET' && request.path === `${prefix}/`).length;
              const initial = documentRequests();
              await other.evaluate(() => localStorage.setItem('lific_theme', 'dark'));
              await page.waitForFunction(() => window.sessionStorageEvents === 1);
              await page.evaluate(() => {
                window.dispatchEvent(new StorageEvent('storage', {key: 'lific_token', oldValue: 'before', newValue: 'after', storageArea: sessionStorage}));
                window.dispatchEvent(new StorageEvent('storage', {key: 'lific_token', oldValue: 'same', newValue: 'same', storageArea: localStorage}));
              });
              await settle(page);
              assert.equal(documentRequests(), initial, 'Unrelated, sessionStorage and unchanged events keep the current document.');
              assert.equal(await page.locator('.native-home-account').textContent(), 'viewer');
              // Exercise repeated native replacements before changing accounts.
              await page.locator('#native-home-palette-open').click();
              await page.locator('#native-home-palette-query').fill('Visible');
              await page.locator('.native-home-palette-results').getByText('Visible project', {exact: true}).waitFor();
              await page.locator('#native-home-palette-query').fill('ACC');
              await page.locator('.native-home-palette-results').getByText('Visible project', {exact: true}).waitFor();
              await page.locator('#native-home-palette-close').click();
              await context.addCookies([cookie(proxy.origin, fixture.replacementToken)]);
              const reloaded = page.waitForEvent('domcontentloaded', {timeout: 5000}).then(() => true, () => false);
              await other.evaluate(token => localStorage.setItem('lific_token', token), 'untrusted-storage-value');
              assert.equal(await reloaded, true, 'Changing lific_token in another tab reloads the current cookie-owned Home.');
              await page.getByText('Private hidden initial work', {exact: true}).waitFor();
              assert.equal(await page.locator('.native-home-account').textContent(), 'admin',
                'The cookie account owns the reloaded Home; stored text supplies no authority.');
              assert.equal(documentRequests(), initial + 1, 'One account change causes one reload after native shard replacements.');
              const cleared = page.waitForEvent('domcontentloaded');
              await other.evaluate(() => localStorage.clear());
              await cleared;
              await page.getByText('Private hidden initial work', {exact: true}).waitFor();
              assert.equal(documentRequests(), initial + 2, 'Clearing localStorage also invalidates the document once.');
              assertNativeOnly(state);
              replacementVerified = true;
            } finally {await state.context.close();}
          });
          await t.test('protocol-emulated trusted focus discovers a cookie-only account replacement', async () => {
            const state = await privatePage(browser, proxy, prefix, viewerToken);
            try {
              const {page, context} = state;
              const other = await context.newPage();
              await other.goto(`${proxy.origin}${prefix}/`);
              // Playwright forces each tab focused. Disable that default, then
              // use Chromium's focus emulation to deliver a trusted focus event.
              // Headless visibility stays visible; this does not prove restoration.
              const pageDriver = await context.newCDPSession(page);
              const otherDriver = await context.newCDPSession(other);
              await Promise.all([pageDriver, otherDriver].map(driver =>
                driver.send('Emulation.setFocusEmulationEnabled', {enabled: false})));
              await page.bringToFront();
              await settle(page);
              const events = [];
              await page.exposeFunction('recordCookieOnlyEvent', event => events.push(event));
              const observe = () => page.evaluate(() => {
                window.addEventListener('focus', event => window.recordCookieOnlyEvent({
                  type: 'focus', trusted: event.isTrusted, visibility: document.visibilityState,
                }));
                window.addEventListener('storage', event => window.recordCookieOnlyEvent({
                  type: 'storage', trusted: event.isTrusted,
                }));
              });
              const stored = await page.evaluate(() => localStorage.getItem('lific_token'));
              for (const [token, account, hiddenVisible] of [
                [fixture.replacementToken, 'admin', true],
                [viewerToken, 'viewer', false],
              ]) {
                await observe();
                const before = events.length;
                await other.bringToFront();
                await pageDriver.send('Emulation.setFocusEmulationEnabled', {enabled: false});
                await page.waitForFunction(() => !document.hasFocus(),
                  undefined, {polling: 50, timeout: 2000});
                await context.addCookies([cookie(proxy.origin, token)]);
                const restored = page.waitForEvent('domcontentloaded', {timeout: 5000}).then(() => true, () => false);
                await pageDriver.send('Emulation.setFocusEmulationEnabled', {enabled: true});
                const didRestore = await restored;
                const restoredEvents = events.slice(before);
                assert.ok(restoredEvents.some(event => event.type === 'focus' && event.trusted && event.visibility === 'visible'),
                  'Chromium delivered a trusted focus event on the visible page.');
                assert.equal(restoredEvents.some(event => event.type === 'storage'), false,
                  'Cookie replacement produces no storage event.');
                assert.equal(didRestore, true,
                  'Trusted focus discovers the current HttpOnly cookie without a storage event or input action.');
                await page.locator('[data-native-home-connected="true"]').first().waitFor();
                await page.getByText('Visible active initial work', {exact: true}).waitFor();
                assert.equal(await page.locator('.native-home-account').textContent(), account,
                  'The current cookie replaces the render-authoritative account baseline.');
                assert.equal(await page.getByText('Private hidden initial work', {exact: true}).count(), hiddenVisible ? 1 : 0,
                  'Private content from the previous account is retired when authority changes.');
                assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')), stored,
                  'Stored token text remains unchanged throughout cookie-only replacement.');
              }
              assertNativeOnly(state);
            } finally {await state.context.close();}
          });
          for (const failedCheck of [false, true]) {
            await t.test(`a focus queued behind a ${failedCheck ? 'failed request' : 'real delayed response'} rechecks the replacement cookie`, async () => {
              const state = await privatePage(browser, proxy, prefix, viewerToken);
              let releaseFirst;
              try {
                const {page, context} = state;
                const other = await context.newPage();
                await other.goto(`${proxy.origin}${prefix}/`);
                const pageDriver = await context.newCDPSession(page);
                const otherDriver = await context.newCDPSession(other);
                await Promise.all([pageDriver, otherDriver].map(driver =>
                  driver.send('Emulation.setFocusEmulationEnabled', {enabled: false})));
                await other.bringToFront();
                await page.waitForFunction(() => !document.hasFocus(), undefined, {polling: 50, timeout: 2000});
                const events = [];
                await page.exposeFunction('recordPendingSessionEvent', event => events.push(event));
                await page.evaluate(() => {
                  window.addEventListener('focus', event => window.recordPendingSessionEvent({type: 'focus', trusted: event.isTrusted}));
                  window.addEventListener('storage', event => window.recordPendingSessionEvent({type: 'storage', trusted: event.isTrusted}));
                });
                const stored = await page.evaluate(() => localStorage.getItem('lific_token'));
                const checks = [];
                let firstReady, firstFailed;
                const firstResponse = new Promise((resolve, reject) => {firstReady = resolve; firstFailed = reject;});
                const released = new Promise(resolve => {releaseFirst = resolve;});
                await page.route(url => url.pathname === `${prefix}/__native_home/session`, async route => {
                  checks.push(await route.request().headerValue('cookie'));
                  if (checks.length === 1) {
                    try {
                      // Fetch the real server outcome under A before changing the cookie.
                      const response = await route.fetch({timeout: 5000});
                      assert.equal(response.status(), 200);
                      firstReady();
                      await released;
                      if (failedCheck) await route.abort('failed');
                      else await route.fulfill({response});
                    } catch (error) {
                      firstFailed(error);
                      await route.abort();
                    }
                  } else {
                    await route.continue();
                  }
                });
                await pageDriver.send('Emulation.setFocusEmulationEnabled', {enabled: true});
                let firstTimer;
                try {
                  await Promise.race([firstResponse, new Promise((_, reject) => {
                    firstTimer = setTimeout(() => reject(new Error('The first real session response did not arrive within 5 seconds.')), 5000);
                  })]);
                } finally {clearTimeout(firstTimer);}
                assert.ok(checks[0].includes(`lific_token=${viewerToken}`), 'The held response was requested under account A.');
                assert.equal(await page.locator('.native-home-account').textContent(), 'viewer');
                await pageDriver.send('Emulation.setFocusEmulationEnabled', {enabled: false});
                await page.waitForFunction(() => !document.hasFocus(), undefined, {polling: 50, timeout: 2000});
                await context.addCookies([cookie(proxy.origin, fixture.replacementToken)]);
                const restored = page.waitForEvent('domcontentloaded', {timeout: 5000}).then(() => true, () => false);
                await pageDriver.send('Emulation.setFocusEmulationEnabled', {enabled: true});
                await page.waitForFunction(() => document.hasFocus());
                assert.equal(checks.length, 1, 'A second focus is coalesced while the first response is held.');
                releaseFirst();
                const didRestore = await restored;
                assert.equal(events.filter(event => event.type === 'focus' && event.trusted).length, 2,
                  'Chromium delivered both trusted focus events.');
                assert.equal(events.some(event => event.type === 'storage'), false);
                assert.equal(didRestore, true,
                  failedCheck
                    ? 'A failed A request cannot discard the queued focus after the cookie changes to B.'
                    : 'An equal-A response cannot discard the queued focus after the cookie changes to B.');
                await page.locator('[data-native-home-connected="true"]').first().waitFor();
                await page.getByText('Private hidden initial work', {exact: true}).waitFor();
                assert.equal(await page.locator('.native-home-account').textContent(), 'admin');
                assert.equal(checks.length, 2, 'One queued focus causes one fresh followup.');
                assert.ok(checks[1].includes(`lific_token=${fixture.replacementToken}`), 'The followup uses current account B.');
                assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')), stored);
                assertNativeOnly(state);
              } finally {
                if (releaseFirst) releaseFirst();
                await state.context.close();
              }
            });
          }
          if (!replacementVerified) return;
          await t.test('a revoked connected palette read navigates to mounted login and clears private Home', async () => {
            const state = await privatePage(browser, proxy, prefix, fixture.revocationTokens[index]);
            try {
              const {page} = state;
              await page.locator('#native-home-palette-open').click();
              await page.locator('.native-home-palette-results').getByText('Visible project', {exact: true}).waitFor();
              const acknowledged = new Promise(resolve => {
                process.stdin.once('data', data => {process.stdin.pause(); assert.equal(data.toString().trim(), 'revoked'); resolve();});
                process.stdin.resume();
              });
              process.stdout.write(`@lific-fixture:revoke:${index}\n`);
              await acknowledged;
              await page.locator('#native-home-palette-query').fill('after revocation');
              await page.waitForURL(`${proxy.origin}${prefix}/login`);
              await page.waitForLoadState('domcontentloaded');
              assert.ok(state.frames.some(frame => frame.t === 'redirect' && frame.location === `${prefix}/login`),
                'The real framework socket carries a mounted login redirect.');
              assert.equal(await page.locator('[data-native-home]').count(), 0);
              assert.equal(await page.getByText('Visible active initial work', {exact: true}).count(), 0);
              assertNativeOnly(state);
            } finally {await state.context.close();}
          });
        } finally {await proxy.close();}
      });
      if (!replacementVerified) break;
    }
  } finally {
    try {await browser.close();}
    finally {process.stdin.destroy();}
  }
});
