// Real native Home; Rust owns the disposable database and revocation controls.
// node session.browser.test.cjs <fixture-origin> <viewer-token> <fixture-json>
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {mountedProxy} = require('./browser_fixture.cjs');
const path = require('node:path');

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

test('native session reloads another-tab replacements under the current cookie and redirects revoked reads', async t => {
  assert.ok(process.env.PLAYWRIGHT_EXECUTABLE_PATH, 'Use the repository Chromium environment.');
  const {chromium} = await import(path.resolve(__dirname, '../../../e2e/node_modules/playwright/index.mjs'));
  const browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
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
