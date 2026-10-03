const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {startFixture} = require('../acceptance/server.js');

// Reference: original ProjectSettings.svelte at 9683d38, desktop1440×900.
// Its840px wrapper has24px side padding: the attention list begins at439px and is792px wide.
test('overview retains the centered Svelte identity, progress, attention, and administration composition', async () => {
  assert.ok(process.env.PLAYWRIGHT_EXECUTABLE_PATH, 'Use the repository e2e Chromium environment.');
  const fixture = await startFixture();
  try {
    const context = await fixture.browser.newContext({viewport: {width: 1440, height: 900}});
    await context.addCookies([{name: 'lific_token', value: fixture.token, url: fixture.origin, httpOnly: true}]);
    await context.addInitScript(({origin, token}) => {
      if (location.origin === origin) localStorage.setItem('lific_token', token);
    }, {origin: fixture.origin, token: fixture.token});
    const page = await context.newPage();
    for (const asset of ['dashboard', 'project-settings']) for (const extension of ['js', 'css']) {
      const directory = asset === 'dashboard' ? 'dashboard' : 'project_settings';
      await page.route(`**/__topcoat-${asset}.${extension}`, route => route.fulfill({
        contentType: extension === 'css' ? 'text/css' : 'text/javascript',
        body: fs.readFileSync(path.join(__dirname, `../${directory}/assets/${asset}.${extension}`), 'utf8'),
      }));
    }
    await page.goto(fixture.url('/ACC/overview'), {waitUntil: 'networkidle'});
    await page.locator('[data-settings-identity-owner]').waitFor();
    await page.evaluate(() => document.fonts.ready);
    const attention = page.locator('.tc-dashboard[data-topcoat-dashboard="overview"] .tc-dashboard__card').first();
    const rect = await attention.boundingBox();
    assert.ok(Math.abs(rect.x - 439) < 2, `Attention starts at reference439px, received${rect.x}.`);
    assert.ok(Math.abs(rect.width - 792) < 2, `Attention width is reference792px, received${rect.width}.`);
    assert.equal(await page.locator('[data-settings-identity-owner] h1').evaluate(el => parseFloat(getComputedStyle(el).fontSize)), 28);
    assert.equal(await page.getByRole('progressbar').getAttribute('aria-label'), '0 of 1 issues done');
    assert.equal(await page.locator('.tc-dashboard__hero .tc-dashboard__metrics').count(), 1);
    assert.equal(await page.getByRole('navigation', {name: 'Project sections'}).count(), 0);
    const sections = await page.locator('[data-project-settings-content] > section').evaluateAll(rows => rows.map(row => row.querySelector('h2')?.textContent));
    assert.deepEqual(sections.slice(0, 2), ['Sidebar group', 'Labels']);
    assert.ok(await page.getByRole('button', {name: fixture.project.name, exact: true}).count());
    assert.equal(await page.getByRole('region', {name: 'Project administration', exact: true}).count(), 1,
      'Flattening presentation boxes retains the administration region in accessibility output.');
    assert.equal(await page.locator('.tc-dashboard__greeting-icon').count(), 0, 'Overview has a project icon, not the Home greeting.');
    await context.close();
  } finally {await fixture.close();}
});
