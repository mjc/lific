// Production CSS requests; page.route explicitly simulates stale CDN bare-path content.
// Versioned stylesheet requests continue to the actual fixture server.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const {createHash} = require('node:crypto');
const {mountedProxy} = require('./browser_fixture.cjs');

const upstream = new URL(process.argv[2]), token = process.argv[3];
const stale = '/* simulated CDN cached bundle from before native Home */ body { margin: 8px; }';

test('document fingerprint escapes stale bare CSS and loads production native styles', async t => {
  assert.ok(process.env.PLAYWRIGHT_EXECUTABLE_PATH, 'Use repository Chromium.');
  const {chromium} = await import(path.resolve(__dirname, '../../../e2e/node_modules/playwright/index.mjs'));
  const browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
  try {
    for (const prefix of ['', '/app', '/ACC']) await t.test(prefix || 'root', async () => {
      const proxy = await mountedProxy(upstream, prefix);
      let context;
      try {
        context = await browser.newContext({viewport: {width: 1440, height: 900}, colorScheme: 'light'});
        await context.addCookies([{name: 'lific_token', value: token, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
        await context.addInitScript(() => {
          window.styleViolations = [];
          document.addEventListener('securitypolicyviolation', event => {
            window.styleViolations.push({directive: event.effectiveDirective, blocked: event.blockedURI});
          });
        });
        const page = await context.newPage(), failed = [], errors = [];
        page.setDefaultTimeout(15000);
        page.on('requestfailed', request => failed.push({url: request.url(), failure: request.failure()}));
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => {if (message.type() === 'error') errors.push(message.text());});
        let bareHits = 0;
        await page.route('**/*', route => {
          const url = new URL(route.request().url());
          if (url.origin === proxy.origin && url.pathname === `${prefix}/__topcoat-app.css` && url.search === '') {
            bareHits++;
            return route.fulfill({status: 200, contentType: 'text/css; charset=utf-8',
              headers: {'cache-control': 'max-age=14400'}, body: stale});
          }
          return route.continue();
        });
        // Prove that the explicit CDN simulation serves obsolete bare-path bytes.
        const bare = await page.goto(`${proxy.origin}${prefix}/__topcoat-app.css`);
        assert.equal(await bare.text(), stale);
        assert.equal(bareHits, 1);
        const receivedCss = page.waitForResponse(response =>
          new URL(response.url()).pathname === `${prefix}/__topcoat-app.css` &&
          response.request().resourceType() === 'stylesheet');
        const receivedRuntime = page.waitForResponse(response =>
          new URL(response.url()).pathname === `${prefix}/__topcoat-runtime.js` &&
          response.request().resourceType() === 'script');
        const document = await page.goto(`${proxy.origin}${prefix}/`);
        assert.equal(document.status(), 200);
        const cssResponse = await receivedCss;
        const links = await page.locator('link[rel="stylesheet"]').evaluateAll(elements => elements.map(element => element.href));
        assert.deepEqual(links, [cssResponse.url()], 'The document-discovered link owns the actual stylesheet request.');
        const cssUrl = new URL(links[0]);
        assert.equal(cssUrl.origin, proxy.origin);
        assert.equal(cssUrl.pathname, `${prefix}/__topcoat-app.css`);
        assert.match(cssUrl.searchParams.get('v') || '', /^[a-f0-9]{64}$/,
          'The document must avoid the stale bare CDN key.');
        assert.equal(cssResponse.status(), 200);
        assert.equal(cssResponse.headers()['content-type'], 'text/css; charset=utf-8');
        assert.equal(cssResponse.headers()['cache-control'], 'no-cache');
        assert.equal(cssResponse.headers()['x-content-type-options'], 'nosniff');
        const css = await cssResponse.body();
        assert.equal(cssUrl.searchParams.get('v'), createHash('sha256').update(css).digest('hex'),
          'The version derives from the exact stylesheet bytes, not a release label.');
        for (const selector of [':root', '.tc-button', '.tc-shell__skip', '.native-home-shell',
          '.tc-home-active', '.tc-home-sections', '.tc-native-home__page']) {
          assert.ok(css.toString('utf8').includes(selector), `Missing actual bundled rule ${selector}`);
        }
        const runtimeResponse = await receivedRuntime;
        const scripts = await page.locator('script[src]').evaluateAll(elements => elements.map(element => element.src));
        assert.deepEqual(scripts, [runtimeResponse.url()], 'Home loads only the actual fingerprinted framework runtime.');
        const runtimeUrl = new URL(scripts[0]);
        assert.equal(runtimeUrl.origin, proxy.origin);
        assert.equal(runtimeUrl.pathname, `${prefix}/__topcoat-runtime.js`);
        assert.match(runtimeUrl.searchParams.get('v') || '', /^[a-f0-9]{64}$/);
        assert.equal(runtimeResponse.status(), 200);
        assert.equal(runtimeResponse.headers()['content-type'], 'text/javascript; charset=utf-8');
        assert.equal(runtimeResponse.headers()['cache-control'], 'no-cache');
        assert.equal(runtimeResponse.headers()['x-content-type-options'], 'nosniff');
        assert.equal(runtimeUrl.searchParams.get('v'), createHash('sha256').update(await runtimeResponse.body()).digest('hex'));
        assert.equal(bareHits, 1, 'Home never requests the simulated stale bare stylesheet.');
        assert.ok(proxy.requests.some(request => request.path === `${prefix}/__topcoat-app.css${cssUrl.search}`),
          'Fingerprint request reaches the real production fixture.');
        await page.locator('[data-native-home-connected="true"]').first().waitFor();
        const applied = await page.evaluate(() => {
          const sheet = document.querySelector('link[rel="stylesheet"]').sheet;
          const style = selector => getComputedStyle(document.querySelector(selector));
          return {rules: sheet?.cssRules.length || 0, shell: style('.native-home-shell').display,
            sidebar: style('.native-home-sidebar').width, border: style('.native-home-panel').borderTopWidth,
            radius: style('.native-home-panel').borderTopLeftRadius,
            pagePadding: style('.tc-native-home__page').paddingTop,
            columns: style('.tc-native-home__columns').flexDirection,
            heading: style('.tc-home-active__heading h2').textTransform,
            bodyMargin: getComputedStyle(document.body).margin, violations: window.styleViolations};
        });
        assert.ok(applied.rules > 0, 'The browser accepts the actual same-origin CSSOM.');
        assert.deepEqual({...applied, rules: 0}, {rules: 0, shell: 'flex', sidebar: '230px', border: '0px', radius: '12px',
          pagePadding: '40px', columns: 'row', heading: 'uppercase', bodyMargin: '0px', violations: []});
        assert.deepEqual(failed, [], 'Stylesheet loading has no network failures.');
        assert.deepEqual(errors, [], 'No CSS MIME/CSP or native browser errors.');
      } finally {
        try {if (context) await context.close();}
        finally {await proxy.close();}
      }
    });
  } finally {await browser.close();}
});
