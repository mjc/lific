// Real production Home: browser capabilities and empty-work presentation.
// node home_browser_edges.test.cjs <fixture-origin> <session-token> <case>
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {mountedProxy, launchBrowser} = require('./browser_fixture.cjs');
const {inspectSvg} = require('./svg_geometry_fixture.cjs');

const upstream = new URL(process.argv[2]);
const token = process.argv[3];
const scenario = process.argv[4];
const output = '/tmp/lific-native-home-edges';
const fixedTime = '2026-10-03T16:00:00Z';

test(`production native Home ${scenario}`, async t => {
  assert.ok(['accessibility', 'storage', 'quiet', 'preloads'].includes(scenario));

  fs.mkdirSync(output, {recursive: true});

  const browser = await launchBrowser();
  try {
    for (const prefix of ['', '/app', '/ACC']) await t.test(prefix || 'root', async () => {
      const proxy = await mountedProxy(upstream, prefix);
      const context = await browser.newContext({viewport: {width: 1440, height: 900},
        locale: 'en-US', timezoneId: 'America/Denver', colorScheme: 'light', reducedMotion: 'reduce'});
      const requests = [], errors = [];
      context.on('request', request => requests.push(request.url()));
      await context.addCookies([{name: 'lific_token', value: token, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
      await context.addInitScript(denied => {
        localStorage.setItem('lific_recents', JSON.stringify([{type: 'issue', routeId: 'ACC-1',
          identifier: 'ACC-1', title: 'Stored recent must be unavailable', project: 'ACC', ts: 1}]));
        window.storageReadAttempts = 0;
        if (denied) Object.defineProperty(Storage.prototype, 'getItem', {configurable: true, value() {
          window.storageReadAttempts += 1;
          throw new DOMException('Storage disabled by browser policy', 'SecurityError');
        }});
      }, scenario === 'storage');
      const page = await context.newPage();
      page.setDefaultTimeout(15000);
      // Playwright's clock replaces resource timing with empty arrays.
      if (scenario !== 'preloads') await page.clock.setFixedTime(fixedTime);
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => {if (message.type() === 'error') errors.push(message.text());});
      try {
        const response = await page.goto(`${proxy.origin}${prefix}/`);
        assert.equal(response.status(), 200);
        await page.locator('[data-native-home-connected="true"]').first().waitFor();
        if (scenario !== 'preloads') {
          await page.locator('#native-home-greeting').filter({hasText: 'Good morning, viewer'}).waitFor();
          assert.equal(await page.locator('#native-home-date').textContent(), 'Saturday, October 3');
        }

        if (scenario === 'preloads') {
          const hints = [...((await response.headers()).link || '').matchAll(/<([^>]+)>; rel=preload; as=image/g)]
            .map(match => match[1]);
          assert.deepEqual(hints, [`${prefix}/logo.webp`]);
          for (const url of hints) {
            const downloads = proxy.requests.filter(request => request.path === url);
            assert.equal(downloads.length, 1, `One actual download for preloaded ${url}`);
          }
          for (const name of ['Circle', 'CircleDot']) {
            const icons = page.locator(`svg.native-icon[data-icon="${name}"]`);
            assert.ok(await icons.count() > 0);
            await page.waitForFunction(name => {
              const icon = document.querySelector(`svg.native-icon[data-icon="${name}"]`);
              return icon && icon.children.length > 0 && getComputedStyle(icon).stroke !== 'none';
            }, name);
            const paint = await icons.first().evaluate(inspectSvg);
            assert.ok(paint.width > 0 && paint.height > 0);
            assert.equal(paint.viewBox, '0 0 24 24');
            assert.notEqual(paint.stroke, 'none');
            assert.equal(paint.strokeWidth, '2px');
            const circles = [{tag: 'circle', attributes: {cx: '12', cy: '12', r: '10'}}];
            if (name === 'CircleDot') circles.push({tag: 'circle', attributes: {cx: '12', cy: '12', r: '1'}});
            assert.deepEqual(paint.shape, circles);
          }
          const timings = await page.evaluate(() => performance.getEntriesByType('resource')
            .filter(entry => new URL(entry.name).pathname.endsWith('/logo.webp'))
            .map(entry => ({url: entry.name, initiator: entry.initiatorType, transfer: entry.transferSize})));
          assert.ok(timings.some(entry => entry.initiator === 'link' && entry.transfer > 0),
            `Browser starts the actual logo download from the response header: ${JSON.stringify(timings)}`);
          await page.reload();
          await page.locator('[data-native-home-connected="true"]').first().waitFor();
          await page.locator('svg.native-icon[data-icon="Circle"]').first().evaluate(inspectSvg);
          assert.equal(await page.locator('svg.native-icon > use').count(), 0);
          assert.deepEqual(requests.filter(url => new URL(url).pathname.includes('/__native_icons/')), [],
            'Native icons make no external SVG requests before or after reload.');
          assert.deepEqual(proxy.requests.filter(request => request.path.includes('/__native_icons/')), [],
            'The production origin serves no external icon assets before or after reload.');
        } else if (scenario === 'accessibility') {
          const skip = page.locator('a[href="#main-content"]');
          assert.equal(await skip.count(), 1, 'Native Home exposes one skip link.');
          const main = page.locator('main#main-content');
          assert.equal(await main.count(), 1);
          assert.equal(await main.getAttribute('tabindex'), '-1');
          await page.keyboard.press('Tab');
          assert.equal(await skip.evaluate(element => element === document.activeElement), true,
            'The skip link is the first keyboard destination.');
          const rect = await skip.boundingBox();
          assert.ok(rect && rect.width > 0 && rect.height > 0 && rect.x >= 0 && rect.y >= 0,
            'The focused skip link is visible inside the viewport.');
          await page.keyboard.press('Enter');
          await page.waitForFunction(() => document.activeElement === document.getElementById('main-content'));
          assert.equal(new URL(page.url()).hash, '#main-content');
        } else if (scenario === 'storage') {
          assert.ok(await page.evaluate(() => window.storageReadAttempts) > 0,
            'The real native initialization attempts the denied browser read.');
          assert.equal(await page.getByText('Recently viewed', {exact: true}).count(), 0);
          assert.equal(await page.getByText('Stored recent must be unavailable', {exact: true}).count(), 0);
          await page.getByText('Visible active initial work', {exact: true}).waitFor();
          await page.locator('#native-home-collapse').click();
          await page.waitForFunction(() => document.querySelector('.native-home-shell').dataset.collapsed === 'true');
          await page.locator('#native-home-quick-jump').click();
          await page.locator('.native-home-palette-results').getByText('Visible project', {exact: true}).waitFor();
          await page.locator('#native-home-palette-close').click();
        } else {
          await page.getByText('All quiet here', {exact: true}).waitFor();
          assert.equal(await page.locator('[data-home-active-count]').textContent(), '0');
          assert.equal(await page.locator('.tc-home-active .tc-dashboard__issue').count(), 0);
          assert.equal(await page.getByText('Nothing active or todo assigned to you across your projects right now.', {exact: true}).count(), 1);
          const mascot = await page.locator('.tc-home-active__mascot').evaluate(async element => {
            const style = getComputedStyle(element), rect = element.getBoundingClientRect();
            const match = style.maskImage.match(/^url\(["']?(.*?)["']?\)$/);
            if (!match) throw new Error(`Missing actual mascot mask: ${style.maskImage}`);
            const image = new Image();
            image.src = match[1];
            await image.decode();
            return {url: image.src, naturalWidth: image.naturalWidth, naturalHeight: image.naturalHeight,
              width: rect.width, height: rect.height, opacity: style.opacity,
              background: style.backgroundColor, faint: style.getPropertyValue('--tc-faint').trim(),
              size: style.maskSize, position: style.maskPosition, repeat: style.maskRepeat};
          });
          assert.equal(new URL(mascot.url).pathname, `${prefix}/__native_home/mascot.png`);
          assert.deepEqual([mascot.naturalWidth, mascot.naturalHeight], [1000, 420]);
          assert.deepEqual([mascot.width, mascot.height], [180, 76]);
          assert.equal(mascot.opacity, '0.5');
          assert.equal(mascot.size, 'contain');
          assert.equal(mascot.position, '50% 50%');
          assert.equal(mascot.repeat, 'no-repeat');
          assert.notEqual(mascot.background, 'rgba(0, 0, 0, 0)');
          assert.ok(proxy.requests.some(request => request.path === `${prefix}/__native_home/mascot.png`),
            'The browser loads the mounted production PNG, rather than a mocked mask.');
        }
        assert.equal(requests.some(url => new URL(url).pathname.split('/').includes('api')), false,
          'All context requests, including other origins and unmounted paths, remain outside REST.');
        assert.deepEqual(errors, []);
      } finally {
        const reportFailure = label => error => {
          process.stderr.write(`${scenario} ${prefix || 'root'} ${label}: ${error.message}\n`);
        };
        try {
          await page.screenshot({path: path.join(output, `${scenario}-${prefix.slice(1) || 'root'}.png`),
            fullPage: true, timeout: 5000}).catch(reportFailure('screenshot failed'));
          try {
            fs.writeFileSync(path.join(output, `${scenario}-${prefix.slice(1) || 'root'}.json`),
              JSON.stringify({requests, proxyRequests: proxy.requests, sockets: proxy.sockets, errors}, null, 2));
          } catch (error) {reportFailure('diagnostics failed')(error);}
        } finally {
          try {await context.close().catch(reportFailure('context cleanup failed'));}
          finally {await proxy.close().catch(reportFailure('proxy cleanup failed'));}
        }
      }
    });
  } finally {await browser.close();}
});
