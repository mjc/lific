// Port of controls_preferences_keep_motion_and_synchronize_other_tabs.
// Real native Home documents, mounted production CSS, genuine storage events.
// node motion.browser.test.cjs <production-origin> <session-token>
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {mountedProxy, launchBrowser} = require('./browser_fixture.cjs');
const upstream = new URL(process.argv[2]), token = process.argv[3];
const output = '/tmp/lific-native-motion-browser';

async function motionState(page) {
  return page.locator('.tc-home-sections__activity').first().evaluate(element => {
    const style = getComputedStyle(element), pseudo = getComputedStyle(element, '::before');
    return {motion: document.documentElement.getAttribute('data-motion'),
      preference: localStorage.getItem('lific_motion'), visible: element.getClientRects().length > 0,
      transition: style.transitionDuration, animation: style.animationDuration,
      iterations: style.animationIterationCount, scroll: style.scrollBehavior,
      pseudoTransition: pseudo.transitionDuration, pseudoAnimation: pseudo.animationDuration};
  });
}
async function assertMotion(page, expected, label, states) {
  await page.waitForFunction(value => document.documentElement.getAttribute('data-motion') === value, expected);
  const actual = await motionState(page);
  states.push({label, ...actual});
  assert.equal(actual.motion, expected, label);
  assert.equal(actual.visible, true, 'The witness is a real visible native Home activity row.');
  const seconds = value => value.split(',').map(item => Number.parseFloat(item.trim()));
  if (expected === 'reduced') {
    for (const key of ['transition', 'animation', 'pseudoTransition', 'pseudoAnimation']) {
      assert.ok(seconds(actual[key]).every(value => value > 0 && value <= 0.0000011),
        `${label}: production ${key} uses the original 0.001ms reduced duration, got ${actual[key]}`);
    }
    assert.equal(actual.iterations, '1');
    assert.equal(actual.scroll, 'auto');
  } else {
    assert.ok(seconds(actual.transition).some(value => value >= 0.1),
      `${label}: full motion retains the actual native transition, got ${actual.transition}`);
  }
}

// A real same-origin tab produces trusted storage events. No dispatchEvent,
// intercepted native reply, substituted document, or injected motion policy.
test('controls_preferences_keep_motion_and_synchronize_other_tabs', async t => {
  fs.mkdirSync(output, {recursive: true});
  const browser = await launchBrowser();
  try {
    for (const prefix of ['', '/app', '/ACC']) {
      for (const [mode, viewport] of [['desktop', {width: 1440, height: 900}], ['phone', {width: 390, height: 844}]]) {
        await t.test(`${prefix || 'root'}-${mode}`, async () => {
          const proxy = await mountedProxy(upstream, prefix);
          const context = await browser.newContext({viewport, isMobile: mode === 'phone', hasTouch: mode === 'phone',
            locale: 'en-US', timezoneId: 'America/Denver', colorScheme: 'light', reducedMotion: 'no-preference'});
          const requests = [], errors = [], states = [], storageEvents = [];
          context.on('request', request => requests.push(request.url()));
          await context.addCookies([{name: 'lific_token', value: token, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
          const writer = await context.newPage(), page = await context.newPage();
          let observedClear;
          const clearObserved = new Promise(resolve => {observedClear = resolve;});
          await page.exposeBinding('__recordNativeMotionStorageEvent', (_, event) => {
            storageEvents.push(event);
            if (event.key === null) observedClear();
          });
          await page.addInitScript(() => {
            window.addEventListener('storage', event => window.__recordNativeMotionStorageEvent({
              key: event.key, newValue: event.newValue, trusted: event.isTrusted,
              local: event.storageArea === localStorage,
            }));
          });
          for (const tab of [writer, page]) {
            tab.setDefaultTimeout(7000);
            tab.on('pageerror', error => errors.push(error.message));
            tab.on('console', message => {if (message.type() === 'error') errors.push(message.text());});
          }
          try {
            const predecessor = await writer.goto(`${proxy.origin}${prefix}/__native_home_shell_predecessor`);
            assert.equal(predecessor.status(), 200);
            await writer.evaluate(() => localStorage.setItem('lific_motion', 'reduced'));
            assert.equal((await page.goto(`${proxy.origin}${prefix}/`)).status(), 200);
            await page.locator('[data-native-home-connected="true"]').first().waitFor();
            await page.getByText('Visible active initial work', {exact: true}).waitFor();
            await assertMotion(page, 'reduced', 'stored reduced overrides an OS without reduced motion', states);
            await page.reload();
            await page.locator('[data-native-home-connected="true"]').first().waitFor();
            await assertMotion(page, 'reduced', 'stored reduced survives an actual reload', states);
            await writer.evaluate(() => localStorage.setItem('lific_motion', 'full'));
            await assertMotion(page, 'full', 'genuine other-tab full preference is applied', states);
            await page.emulateMedia({reducedMotion: 'reduce'});
            await page.waitForFunction(() => matchMedia('(prefers-reduced-motion: reduce)').matches);
            await assertMotion(page, 'full', 'stored full overrides an OS requesting reduced motion', states);
            await writer.evaluate(() => localStorage.setItem('lific_motion', 'invalid'));
            await assertMotion(page, 'reduced', 'invalid preference uses the OS default', states);
            await writer.evaluate(() => localStorage.setItem('lific_motion', 'system'));
            await page.waitForFunction(() => localStorage.getItem('lific_motion') === 'system');
            await page.emulateMedia({reducedMotion: 'no-preference'});
            await assertMotion(page, 'full', 'system preference follows a genuine OS media change', states);
            await writer.evaluate(() => localStorage.setItem('lific_motion', 'reduced'));
            await assertMotion(page, 'reduced', 'explicit reduced overrides OS motion', states);
            await writer.evaluate(() => localStorage.removeItem('lific_motion'));
            await assertMotion(page, 'full', 'other-tab removal returns to the OS default', states);
            await page.emulateMedia({reducedMotion: 'reduce'});
            await assertMotion(page, 'reduced', 'missing preference follows OS reduced motion', states);
            await writer.evaluate(() => localStorage.setItem('lific_motion', 'full'));
            await assertMotion(page, 'full', 'explicit full precedes the genuine storage clear', states);
            await writer.evaluate(() => localStorage.clear());
            await clearObserved;
            await assertMotion(page, 'reduced', 'a genuine other-tab storage clear retains the OS choice', states);
            const events = storageEvents;
            assert.ok(events.some(event => event.key === 'lific_motion' && event.newValue === 'full'));
            assert.ok(events.some(event => event.key === 'lific_motion' && event.newValue === null));
            assert.ok(events.every(event => event.trusted && event.local), 'The browser itself emitted every observed localStorage event.');
            await page.reload();
            await page.locator('[data-native-home-connected="true"]').first().waitFor();
            await assertMotion(page, 'reduced', 'missing preference resolves the OS on reload', states);
            await page.emulateMedia({reducedMotion: 'no-preference'});
            await assertMotion(page, 'full', 'the replacement scope listens to genuine OS changes', states);
            await writer.evaluate(() => localStorage.setItem('lific_motion', 'reduced'));
            await assertMotion(page, 'reduced', 'the replacement scope listens to genuine storage changes', states);
            await writer.evaluate(() => localStorage.setItem('lific_motion', 'full'));
            await assertMotion(page, 'full', 'stored full is ready for initial hydration', states);
            await page.emulateMedia({reducedMotion: 'reduce'});
            await page.reload();
            await page.locator('[data-native-home-connected="true"]').first().waitFor();
            await assertMotion(page, 'full', 'initial full preference overrides OS reduced motion after reload', states);
            await writer.evaluate(() => localStorage.setItem('lific_motion', 'invalid'));
            await assertMotion(page, 'reduced', 'invalid stored value falls back before reload', states);
            await page.reload();
            await page.locator('[data-native-home-connected="true"]').first().waitFor();
            await assertMotion(page, 'reduced', 'initial invalid preference defaults to OS reduced motion', states);
            assert.equal(requests.some(url => new URL(url).pathname.split('/').includes('api')), false,
              'Motion preference admission requires no REST requests.');
            assert.deepEqual(errors, []);
          } finally {
            const name = `${prefix.slice(1) || 'root'}-${mode}`;
            try {
              await page.screenshot({path: path.join(output, `${name}.png`), fullPage: true}).catch(() => {});
              fs.writeFileSync(path.join(output, `${name}.json`), JSON.stringify({states, requests, errors,
                proxyRequests: proxy.requests, sockets: proxy.sockets}, null, 2));
            } finally {try {await context.close();} finally {await proxy.close();}}
          }
        });
      }
    }
  } finally {await browser.close();}
});
