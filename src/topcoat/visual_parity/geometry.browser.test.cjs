const {test} = require('node:test');
const assert = require('node:assert/strict');
const {startFixture} = require('../acceptance/server.js');

// Measured from the real Svelte Home at immutable baseline 9683d38af8e1e6f9b076439fe90d9519109b2218.
// The matching 1440×900 and 390×844 captures/geometry.json accompany LIF-229.
const REFERENCE = {
  desktop: {sidebar: 230, contentTop: 36.8, headingX: 318, headingY: 76.8, headingSize: 22},
  phone: {sidebar: 0, contentTop: 84.8, headingX: 80, headingY: 116.8, headingSize: 22},
};
const near = (actual, expected, label, tolerance = 2) => assert.ok(Math.abs(actual - expected) <= tolerance,
  `${label}: expected ${expected} ± ${tolerance}, received ${actual}`);

test('the executable Home retains measured Svelte shell and type geometry, touch layout, and larger text',
  async t => {
    assert.ok(process.env.PLAYWRIGHT_EXECUTABLE_PATH, 'Run geometry checks through the repository e2e devenv workflow with its Chromium executable.');
    const fixture = await startFixture();
    try {
      const updated = await fixture.api(`/issues/${fixture.issue.id}`, {method: 'PUT', body: {status: 'active'}});
      assert.equal(updated.ok, true);
      for (let index = 0; index < 7; index++) {
        const created = await fixture.api('/issues', {method: 'POST', body: {project_id: fixture.project.id,
          title: `Geometry regression issue ${index}`, status: 'active', priority: 'high'}});
        assert.equal(created.ok, true);
      }
      for (const device of ['desktop', 'phone']) for (const theme of ['light', 'dark']) {
        await t.test(`${device} ${theme}`, async () => {
          const context = await fixture.browser.newContext({
            viewport: device === 'phone' ? {width: 390, height: 844} : {width: 1440, height: 900},
            isMobile: device === 'phone', hasTouch: device === 'phone', colorScheme: theme,
          });
          await context.addCookies([{name: 'lific_token', value: fixture.token, url: fixture.origin, httpOnly: true}]);
          await context.addInitScript(({origin, theme, token}) => {
            if (location.origin !== origin) return;
            localStorage.setItem('lific_token', token);
            localStorage.setItem('lific_theme', theme);
          }, {origin: fixture.origin, theme, token: fixture.token});
          const page = await context.newPage();
          await page.goto(fixture.url('/'));
          await page.waitForFunction(() => document.querySelector('[data-topcoat-dashboard]')?.getAttribute('aria-busy') === 'false');
          await page.evaluate(() => document.fonts.ready);
          const geometry = await page.evaluate(() => {
            const heading = document.querySelector('.tc-dashboard h1');
            const style = getComputedStyle(heading);
            const shell = document.querySelector('.tc-shell');
            return {header: shell.querySelector('.tc-shell__header').getBoundingClientRect().toJSON(),
              contentTop: shell.querySelector('.tc-page-chrome').getBoundingClientRect().bottom,
              heading: heading.getBoundingClientRect().toJSON(), size: parseFloat(style.fontSize), family: style.fontFamily,
              border: getComputedStyle(document.querySelector('.tc-dashboard__card')).borderTopColor,
              overflow: document.documentElement.scrollWidth > innerWidth,
              statusName: document.querySelector('.tc-dashboard__status').getAttribute('aria-label')};
          });
          const reference = REFERENCE[device];
          if (device === 'desktop') near(geometry.header.width, reference.sidebar, 'sidebar header width');
          near(geometry.contentTop, reference.contentTop, 'content starts beneath route chrome');
          near(geometry.heading.x, reference.headingX, 'greeting horizontal position');
          near(geometry.heading.y, reference.headingY, 'greeting vertical position');
          near(geometry.size, reference.headingSize, 'greeting text size', .1);
          assert.match(geometry.family, /Space Grotesk/);
          assert.equal(geometry.border, theme === 'light' ? 'rgb(208, 220, 214)' : 'rgb(61, 72, 66)');
          assert.equal(geometry.overflow, false);
          assert.equal(geometry.statusName, 'active');
          assert.ok(await page.getByRole('img', {name: 'active', exact: true}).count() > 0,
            'Status remains exposed to assistive technology after switching to SVG.');
          if (device === 'desktop') {
            await page.locator('.tc-projects__create-toggle').click();
            await page.getByRole('textbox', {name: 'New group name'}).fill(`Visual parity ${theme}`);
            await page.getByRole('button', {name: 'Create group', exact: true}).click();
            await page.getByRole('button', {name: `Collapse Visual parity ${theme}`}).waitFor();
          } else {
            await page.getByRole('button', {name: 'Open navigation', exact: true}).click();
            await page.getByRole('dialog', {name: 'Navigation', exact: true}).waitFor();
            await page.getByRole('button', {name: 'Close navigation', exact: true}).first().click();
            await page.locator('.tc-shell__main').evaluate(el => {el.scrollTop = el.scrollHeight;});
            assert.equal(await page.locator('.tc-shell__main').evaluate(el => el.scrollTop > 0), true);
          }
          await page.evaluate(() => localStorage.setItem('lific_font_scale', 'lg'));
          await page.reload();
          await page.waitForFunction(() => document.querySelector('[data-topcoat-dashboard]')?.getAttribute('aria-busy') === 'false');
          near(await page.locator('.tc-dashboard h1').evaluate(el => parseFloat(getComputedStyle(el).fontSize)), 24.75, 'larger greeting text', .1);
          await context.close();
        });
      }
    } finally { await fixture.close(); }
  });
