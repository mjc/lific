const {test} = require('node:test');
const assert = require('node:assert/strict');
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
    await page.goto(fixture.url('/ACC/overview'), {waitUntil: 'networkidle'});
    await page.locator('.native-overview__name').waitFor();
    await page.evaluate(() => document.fonts.ready);
    const attention = page.locator('[data-native-overview-attention]');
    const rect = await attention.boundingBox();
    assert.ok(Math.abs(rect.x - 439) < 2, `Attention starts at reference439px, received${rect.x}.`);
    assert.ok(Math.abs(rect.width - 792) < 2, `Attention width is reference792px, received${rect.width}.`);
    assert.equal(await page.locator('.native-overview__name').evaluate(el => parseFloat(getComputedStyle(el).fontSize)), 28);
    assert.equal(await page.getByRole('progressbar').getAttribute('aria-valuenow'), '0');
    assert.equal((await page.locator('.native-overview__completion > span').textContent()).trim(), '0/1 done');
    assert.equal(await page.locator('.native-overview__hero .native-overview__completion').count(), 1);
    assert.equal(await page.getByRole('navigation', {name: 'Project sections'}).count(), 0);
    const sections = page.locator('.native-overview__column > section');
    const group = sections.filter({has: page.getByText('Sidebar group', {exact: true})});
    const labels = sections.filter({has: page.getByRole('heading', {name: /^Labels/})});
    assert.equal(await group.count(), 1);
    assert.equal(await labels.count(), 1);
    assert.equal(await group.evaluate((element, selector) => element.nextElementSibling.matches(selector), '.native-overview__labels'), true);
    assert.ok(await page.getByRole('button', {name: fixture.project.name, exact: true}).count());
    assert.equal(await page.getByRole('heading', {name: /^Labels/}).count(), 1);
    assert.equal(await page.getByRole('heading', {name: 'Public view', exact: true}).count(), 1);
    assert.equal(await page.locator('.native-overview__icon').count(), 1);
    assert.equal(await page.locator('.tc-native-home__greeting-icon').count(), 0, 'Overview has a project icon, not the Home greeting.');
    await context.close();
  } finally {await fixture.close();}
});
