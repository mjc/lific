// Genuine routes, trigger-generated audits and browser timers; no shadow model.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {mountedProxy, launchBrowser, launchVisibilityBrowser} = require('../browser_fixture.cjs');
const upstream = new URL(process.argv[2]), token = process.argv[3];
const fixture = JSON.parse(process.argv[4]), scenario = process.argv[5];
const output = path.join(require('node:os').tmpdir(), 'lific-native-activity-browser');
const normalize = text => text.replace(/\s+/g, ' ').trim();
const visibleRows = timeline => timeline.locator('ol > li:visible');
const visibleTimes = timeline => visibleRows(timeline).locator('time');

// Observe the actual interval API, preserving callback, delay, receiver and ID.
// Installed after initial hydration; the first real hidden→visible transition
// then captures the genuine resumed interval rather than guessing its start.
async function observeIntervals(page) {
  await page.evaluate(() => {
    const create = window.setInterval, clear = window.clearInterval;
    const records = [], active = new Map();
    window.activityIntervals = {records, active, ticks: 0};
    window.setInterval = function (callback, delay, ...args) {
      let entry;
      const observed = typeof callback === 'function' ? function (...values) {
        if (entry) {entry.ticks++; window.activityIntervals.ticks++;}
        return Reflect.apply(callback, this, values);
      } : callback;
      const id = Reflect.apply(create, this, [observed, delay, ...args]);
      if (Number(delay) === 30000) {
        entry = {id: String(id), startedAt: Date.now(), ticks: 0};
        records.push(entry); active.set(id, entry);
      }
      return id;
    };
    window.clearInterval = function (id) {
      active.delete(id);
      return Reflect.apply(clear, this, [id]);
    };
  });
}
async function intervals(page) {
  return page.evaluate(() => ({
    active: window.activityIntervals.active.size,
    records: window.activityIntervals.records,
    ticks: window.activityIntervals.ticks,
  }));
}
async function allTimes(timeline, expected) {
  // Wait only for real rendered text; no mutation/visibility getter override.
  await timeline.page().waitForFunction(({selector, expected}) => {
    const nodes = Array.from(document.querySelectorAll(selector))
      .filter(node => node.getClientRects().length > 0);
    return nodes.length === 6 && nodes.every(node => node.textContent === expected);
  }, {selector: '[data-native-issue-activity] ol > li time', expected}, {timeout: 15000});
  assert.deepEqual(await visibleTimes(timeline).allTextContents(), Array(6).fill(expected));
}

test(`native Activity ${scenario} uses real production data and scope`, async t => {
  assert.ok(['history', 'clock'].includes(scenario));
  const browser = scenario === 'history' ? await launchBrowser() : null;
  try {
    for (const prefix of ['', '/app', '/ACC']) await t.test(prefix || 'root', async () => {
      const proxy = await mountedProxy(upstream, prefix);
      const errors = [], consoleErrors = [], requests = [];
      let owner, context, page;
      try {
        owner = scenario === 'clock' ? await launchVisibilityBrowser() : null;
        context = owner ? owner.context : await browser.newContext({viewport: {width: 1440, height: 900}});
        context.on('request', request => requests.push(request));
        await context.addCookies([{name: 'lific_token', value: token, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
        page = await context.newPage();
        if (owner) await page.setViewportSize({width: 1440, height: 900});
        page.setDefaultTimeout(15000);
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => {if (message.type() === 'error') consoleErrors.push(message.text());});
        if (scenario === 'clock') await page.clock.install({time: new Date(fixture.clock_time)});
        assert.equal((await page.goto(`${proxy.origin}${prefix}/ACC/issues/${fixture.identifier}`)).status(), 200);
        const timeline = page.locator('[data-native-issue-activity]');
        await timeline.getByRole('heading', {name: 'Activity', exact: true}).waitFor();
        assert.equal(await visibleRows(timeline).count(), 6);
        assert.equal(await timeline.locator('.native-issue-activity__count').textContent(), '8');
        assert.equal(await timeline.getByRole('button', {name: 'Show all 8 entries', exact: true}).count(), 1);
        const navigations = requests.filter(request => request.isNavigationRequest()).length;
        await page.evaluate(() => {
          window.activityDocument = document;
          window.activityParent = document.querySelector('.native-home-shell');
        });

        if (scenario === 'history') {
          const firstIds = await visibleRows(timeline).evaluateAll(nodes => nodes.map(node => Number(node.dataset.activityId)));
          assert.deepEqual(firstIds, fixture.history_ids.slice(0, 6));
          await timeline.getByRole('button', {name: 'Show all 8 entries', exact: true}).click();
          assert.equal(await visibleRows(timeline).count(), 8);
          assert.deepEqual(await visibleRows(timeline).evaluateAll(nodes => nodes.map(node => Number(node.dataset.activityId))), fixture.history_ids);
          const description = visibleRows(timeline).filter({hasText: 'changed description'});
          assert.equal(await description.count(), 1);
          await description.getByRole('button', {name: 'show change', exact: true}).click();
          const blocks = description.locator('.native-issue-activity__values > div:visible');
          assert.equal(await blocks.count(), 2);
          assert.deepEqual(await blocks.allTextContents(), ['(empty)', fixture.description]);
          assert.equal(await description.getByRole('button', {name: 'hide change', exact: true}).count(), 1);
          await timeline.getByRole('button', {name: 'Show recent only', exact: true}).click();
          assert.equal(await visibleRows(timeline).count(), 6);
          await timeline.getByRole('button', {name: 'Show all 8 entries', exact: true}).click();
          assert.equal(await visibleRows(timeline).count(), 8);
          assert.deepEqual(await blocks.allTextContents(), ['(empty)', fixture.description]);
          await description.getByRole('button', {name: 'hide change', exact: true}).click();
          assert.equal(await blocks.count(), 0);
          await timeline.getByRole('button', {name: 'Show recent only', exact: true}).click();
          assert.equal(await visibleRows(timeline).count(), 6);
          const lines = (await visibleRows(timeline).locator('.native-issue-activity__line').allInnerTexts()).map(normalize);
          assert.ok(lines[0].includes('changed title Activity history title 3 → Activity history title 4'));
        } else {
          await allTimes(timeline, 'just now');
          await page.clock.pauseAt(await page.evaluate(() => Date.now() + 1000));
          await page.clock.setSystemTime(new Date(fixture.clock_time));
          await observeIntervals(page);
          const other = await context.newPage();
          await other.goto('about:blank'); await other.bringToFront();
          await page.waitForFunction(() => document.visibilityState === 'hidden');
          await page.bringToFront();
          await page.waitForFunction(() => document.visibilityState === 'visible');
          const resumed = await intervals(page);
          assert.equal(resumed.active, 1, 'All six visible history rows share one actual resumed timer.');
          assert.equal(resumed.records.length, 1);
          assert.equal(resumed.records[0].startedAt, Date.parse(fixture.clock_time));
          assert.equal(resumed.ticks, 0);
          await allTimes(timeline, 'just now');
          await page.clock.runFor(29999);
          assert.equal((await intervals(page)).ticks, 0);
          await allTimes(timeline, 'just now');
          await page.clock.runFor(1);
          assert.equal((await intervals(page)).ticks, 1);
          await allTimes(timeline, '1m ago');
          await other.bringToFront();
          await page.waitForFunction(() => document.visibilityState === 'hidden');
          assert.equal((await intervals(page)).active, 0, 'Actual hidden event clears the owning clock.');
          await page.clock.runFor(120000);
          assert.equal((await intervals(page)).ticks, 1);
          assert.deepEqual(await visibleTimes(timeline).allTextContents(), Array(6).fill('1m ago'));
          await page.bringToFront();
          await page.waitForFunction(() => document.visibilityState === 'visible');
          await allTimes(timeline, '3m ago');
          assert.equal((await intervals(page)).ticks, 1, 'Visibility refreshes immediately without a timer callback.');
          assert.equal((await intervals(page)).active, 1);
          await other.close();
          await page.evaluate(() => {window.retiredActivity = document.querySelector('[data-native-issue-activity]');});
          await page.getByRole('navigation', {name: 'Breadcrumb', exact: true}).getByRole('link', {name: 'Issues', exact: true}).click();
          await page.locator('[data-native-issue-list]').waitFor();
          assert.ok(await page.evaluate(() => !window.retiredActivity.isConnected && window.activityParent === document.querySelector('.native-home-shell')));
          assert.equal((await intervals(page)).active, 0, 'Actual issue-scope retirement clears its timer.');
          const ticks = (await intervals(page)).ticks;
          const retiredText = await page.evaluate(() => window.retiredActivity.textContent);
          await page.clock.runFor(90000);
          assert.equal((await intervals(page)).ticks, ticks);
          assert.equal(await page.evaluate(() => window.retiredActivity.textContent), retiredText);
        }
        assert.ok(await page.evaluate(() => window.activityDocument === document));
        assert.equal(requests.filter(request => request.isNavigationRequest()).length, navigations);
        assert.ok(requests.every(request => !new URL(request.url()).pathname.split('/').includes('api')));
        assert.equal(requests.filter(request => request.method() === 'POST' && new URL(request.url()).pathname.endsWith('/__native_issue_edit/save')).length, 0);
        assert.deepEqual(errors, []);
        assert.deepEqual(consoleErrors, [], 'Caught mount/clock failures must fail acceptance.');
      } finally {
        try {
          fs.mkdirSync(output, {recursive: true});
          fs.writeFileSync(path.join(output, `${scenario}-${prefix.slice(1) || 'root'}-diagnostics.json`), JSON.stringify({errors, consoleErrors, requests: requests.map(request => ({url: request.url(), method: request.method()}))}, null, 2));
        } finally {
          try {
            if (owner) {
              try {if (context) {for (const existing of context.pages()) await existing.close(); await context.clearCookies();}}
              finally {await owner.close();}
            } else if (context) await context.close();
          } finally {await proxy.close();}
        }
      }
    });
  } finally {if (browser) await browser.close();}
});
