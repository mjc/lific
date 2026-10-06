// Hold genuine generated-module responses before admitting an interactive document.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {mountedProxy, launchBrowser} = require('./browser_fixture.cjs');
const upstream = new URL(process.argv[2]), token = process.argv[3];

test('native connection waits for every generated handler at every mount', async t => {
  const browser = await launchBrowser();
  try {
    for (const prefix of ['', '/app', '/ACC']) {
      for (const asset of ['__native-home-shell.js', '__native-workspace.js', '__native-sidebar.js']) {
        await t.test(`${prefix || 'root'} ${asset}`, async () => {
          const proxy = await mountedProxy(upstream, prefix);
          const context = await browser.newContext({viewport: {width: 1200, height: 850}, reducedMotion: 'reduce'});
          let release;
          const held = new Promise(resolve => {release = resolve;});
          const pending = [];
          try {
            await context.addCookies([{name: 'lific_token', value: token, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
            await context.addInitScript(() => localStorage.setItem('lific_motion', 'reduced'));
            const page = await context.newPage();
            page.setDefaultTimeout(7000);
            const errors = [], requests = [];
            page.on('pageerror', error => errors.push(error.message));
            page.on('console', message => {if (message.type() === 'error') errors.push(message.text());});
            page.on('request', request => requests.push({url: request.url(), type: request.resourceType()}));
            let observed;
            const requested = new Promise(resolve => {observed = resolve;});
            await page.route(`**/${asset}?*`, route => {
              const task = (async () => {
                const response = await route.fetch();
                assert.equal(response.status(), 200, 'The gate holds a real successful immutable asset response.');
                assert.equal(response.headers()['cache-control'], 'public, max-age=31536000, immutable');
                observed();
                await held;
                await route.fulfill({response});
              })();
              pending.push(task);
              return task;
            });
            assert.equal((await page.goto(`${proxy.origin}${prefix}/`, {waitUntil: 'commit'})).status(), 200);
            let deadline;
            try {
              await Promise.race([
                requested,
                new Promise((_, reject) => {deadline = setTimeout(() => reject(new Error(`Handler was not requested: ${asset}`)), 7000);}),
              ]);
            } finally {clearTimeout(deadline);}
            const connected = page.locator('[data-native-home-connected="true"]').first();
            await assert.rejects(connected.waitFor({timeout: 1000}), /Timeout/,
              'The document must remain disconnected while an owning handler has not arrived.');
            release();
            await Promise.all(pending);
            await connected.waitFor();
            const aside = page.getByRole('complementary', {name: 'Workspace sidebar', exact: true});
            await page.evaluate(() => {window.nativeHandlerAdmissionDocument = 'retained';});
            const documents = requests.filter(request => request.type === 'document').length;
            await aside.getByRole('button', {name: 'Expand Visible project', exact: true}).click();
            await aside.getByRole('button', {name: 'Collapse Visible project', exact: true}).waitFor();
            await aside.locator(`a[data-sidebar-project][href="${prefix}/ACC/overview"]`).click();
            await page.waitForURL(`${proxy.origin}${prefix}/ACC/overview`);
            await page.locator('.native-overview').waitFor();
            assert.equal(await page.evaluate(() => window.nativeHandlerAdmissionDocument), 'retained',
              'Overview navigation uses the admitted same document handlers.');
            assert.equal(requests.filter(request => request.type === 'document').length, documents);
            assert.deepEqual(requests.filter(request => /\/api(?:\/|\?|$)/.test(new URL(request.url).pathname)), [],
              'Native disclosure and navigation do not use legacy REST.');
            assert.deepEqual(errors, []);
          } finally {
            release();
            await Promise.allSettled(pending);
            await context.close();
            await proxy.close();
          }
        });
      }
    }
  } finally {await browser.close();}
});
