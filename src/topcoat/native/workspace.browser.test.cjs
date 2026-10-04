const {test} = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const readline = require('node:readline');
const {mountedProxy, launchBrowser} = require(path.join(process.cwd(), 'src/topcoat/native/browser_fixture.cjs'));
const upstream = new URL(process.argv[2]), token = process.argv[3];
const input = readline.createInterface({input: process.stdin});
let awaiting;
input.on('line', line => { assert.ok(awaiting); const resolve = awaiting; awaiting = undefined; resolve(JSON.parse(line)); });
function control(action, values = {}) {
  return new Promise(resolve => {
    assert.equal(awaiting, undefined);
    awaiting = resolve;
    process.stdout.write(`WORKSPACE_CONTROL ${JSON.stringify({action, ...values})}\n`);
  });
}
const rename = title => control('rename', {title});
const membership = role => control('membership', {role});
async function retainedParent(page) {
  assert.ok(await page.evaluate(() => document.querySelector('.native-home-shell') === window.workspaceParent && window.workspaceParent.testParentToken === window.workspaceToken));
}
async function list(page, origin, prefix) {
  await page.waitForURL(`${origin}${prefix}/ACC/issues`);
  await page.locator('[data-native-issue-list]').waitFor();
  assert.equal(await page.locator('[data-native-issue-editor]').count(), 0);
  await retainedParent(page);
}
async function editableIssue(page, origin, prefix, snapshot) {
  await page.waitForURL(`${origin}${prefix}/ACC/issues/ACC-1`);
  await page.waitForFunction(title => document.querySelector('#native-issue-title-ACC-1')?.textContent === title, snapshot.title);
  assert.equal(Number(await page.locator('[data-native-issue-seq]').textContent()), snapshot.seq);
  assert.equal(await page.locator('#native-issue-body-input-ACC-1').isVisible(), false, 'Retired edit draft does not survive route replacement.');
  await retainedParent(page);
}
function nativeDocument(html, list) {
  assert.ok(html.includes('native-home-shell'), 'Actual native workspace shell renders in initial HTTP.');
  assert.ok(html.includes(list ? 'data-native-issue-list' : 'data-native-issue-editor'), 'Authorized initial page content renders before browser hydration.');
  const scripts = [...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"/g)].map(match => match[1]);
  assert.equal(scripts.length, 1, `Only framework runtime is loaded: ${scripts}`);
  assert.ok(scripts[0].includes('__topcoat-runtime'));
  assert.ok(!html.includes('data-lific-session-state'), 'Legacy session bootstrap is absent.');
  if (list) {
    assert.ok(html.includes('ACC-1') && html.includes('Visible active initial work'), 'Initial list contains the authorized real issue.');
    assert.ok(!html.includes('Private hidden initial work') && !html.includes('HIDE-1'));
  }
}
test('normal native issue → Issues → actual row keeps workspace owner', async t => {
  const browser = await launchBrowser();
  try {
    for (const prefix of ['', '/app', '/ACC']) await t.test(prefix || 'root', async () => {
      const proxy = await mountedProxy(upstream, prefix);
      const context = await browser.newContext({viewport:{width:1440,height:900}, reducedMotion:'reduce'});
      const requests = [], errors = [];
      context.on('request', request => requests.push({url:request.url(), authorization:request.headers().authorization, type:request.resourceType()}));
      await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
      const page = await context.newPage();
      page.setDefaultTimeout(15000);
      page.on('pageerror', error => errors.push(error.message));
      try {
        assert.equal((await membership('maintainer')).role, 'maintainer');
        await rename('Visible active initial work');
        // Independent direct GET proves normal list SSR, before any body wait.
        const initial = await context.request.get(`${proxy.origin}${prefix}/ACC/issues`);
        assert.equal(initial.status(), 200);
        nativeDocument(await initial.text(), true);
        const response = await page.goto(`${proxy.origin}${prefix}/ACC/issues/ACC-1`);
        assert.equal(response.status(), 200);
        nativeDocument(await response.text(), false);
        await page.locator('[data-native-issue-editor="ACC-1"]').waitFor();
        await page.locator('#native-issue-body-edit-ACC-1').click();
        await page.locator('#native-issue-body-input-ACC-1').waitFor({state:'visible'});
        await page.locator('#native-issue-body-input-ACC-1').fill('Uncommitted retired draft');
        await page.evaluate(() => {
          window.workspaceParent = document.querySelector('.native-home-shell');
          window.workspaceToken = Symbol('test parent identity');
          window.workspaceParent.testParentToken = window.workspaceToken;
          window.retiredEditor = document.querySelector('[data-native-issue-editor="ACC-1"]');
        });
        const documents = requests.filter(request => request.type === 'document').length;
        const breadcrumb = page.locator('.native-issue-detail__breadcrumbs a[title="Issues"]');
        assert.equal(await breadcrumb.getAttribute('href'), `${prefix}/ACC/issues`);
        await breadcrumb.click();
        await page.waitForURL(`${proxy.origin}${prefix}/ACC/issues`);
        await page.locator('[data-native-issue-list]').waitFor();
        assert.equal(await page.locator('[data-native-issue-editor]').count(), 0);
        assert.ok(await page.evaluate(() => !window.retiredEditor.isConnected && document.querySelector('.native-home-shell') === window.workspaceParent && window.workspaceParent.testParentToken === window.workspaceToken));
        const freshTitle = `Fresh independent workspace ${prefix || 'root'}`;
        const fresh = await rename(freshTitle);
        const row = page.locator('[data-native-issue-list] a').filter({hasText:'ACC-1'});
        assert.equal(await row.getAttribute('href'), `${prefix}/ACC/issues/ACC-1`);
        await row.click();
        await page.waitForURL(`${proxy.origin}${prefix}/ACC/issues/ACC-1`);
        await page.waitForFunction(title => document.querySelector('#native-issue-title-ACC-1')?.textContent === title, fresh.title);
        assert.equal(Number(await page.locator('[data-native-issue-seq]').textContent()), fresh.seq);
        assert.equal(await page.locator('#native-issue-body-input-ACC-1').isVisible(), false, 'Retired edit draft does not survive route replacement.');
        assert.ok(await page.evaluate(() => document.querySelector('.native-home-shell') === window.workspaceParent && window.workspaceParent.testParentToken === window.workspaceToken));
        await page.evaluate(() => { window.forwardEditor = document.querySelector('[data-native-issue-editor="ACC-1"]'); });
        await page.goBack();
        await list(page, proxy.origin, prefix);
        assert.ok(await page.evaluate(() => !window.forwardEditor.isConnected), 'Back disposes the previous issue owner.');
        const historyTitle = `Fresh history workspace ${prefix || 'root'}`;
        const history = await rename(historyTitle);
        await page.goForward();
        await editableIssue(page, proxy.origin, prefix, history);
        assert.ok(await page.evaluate(() => document.querySelector('[data-native-issue-editor="ACC-1"]') !== window.forwardEditor), 'Forward creates a fresh issue owner.');
        await page.goBack();
        await list(page, proxy.origin, prefix);
        const demoted = await membership('viewer');
        assert.equal(demoted.role, 'viewer');
        const viewerTitle = `Fresh Viewer workspace ${prefix || 'root'}`;
        const viewer = await rename(viewerTitle);
        await page.goForward();
        await page.waitForURL(`${proxy.origin}${prefix}/ACC/issues/ACC-1`);
        await page.getByRole('heading', {name:viewer.title, exact:true}).waitFor();
        assert.equal(Number(await page.locator('[data-native-issue-seq]').textContent()), viewer.seq);
        for (const selector of ['input[aria-label="Issue title"]', 'textarea[aria-label="Issue description"]',
          '[data-native-issue-body-save]', '[data-native-issue-status-option]', '[data-native-issue-priority-option]',
          'button[aria-label="Change issue status"]', 'button[aria-label="Change issue priority"]'])
          assert.equal(await page.locator(selector).count(), 0, 'Forward reads current Viewer membership rather than old Maintainer state.');
        assert.equal(await page.getByText('Read-only', {exact:true}).count(), 1);
        assert.equal(await page.getByRole('button', {name:'Export', exact:true}).count(), 1, 'Fresh Viewer retains genuine export access.');
        await retainedParent(page);
        await page.evaluate(() => { window.viewerEditor = document.querySelector('[data-native-issue-editor="ACC-1"]'); });
        assert.equal((await membership('maintainer')).role, 'maintainer');
        await page.goBack();
        await list(page, proxy.origin, prefix);
        assert.ok(await page.evaluate(() => !window.viewerEditor.isConnected));
        await page.goForward();
        const restored = await control('snapshot');
        await editableIssue(page, proxy.origin, prefix, restored);
        assert.equal(await page.getByText('Read-only', {exact:true}).count(), 0);
        assert.equal(await page.getByRole('button', {name:'Change issue status', exact:true}).count(), 1);
        assert.equal(restored.description, 'Persisted workspace description', 'History never commits an abandoned draft.');
        await page.locator('#native-issue-body-edit-ACC-1').click();
        await page.locator('#native-issue-body-input-ACC-1').waitFor({state:'visible'});
        assert.equal(await page.locator('#native-issue-body-input-ACC-1').inputValue(), restored.description, 'Fresh owner starts from persisted source.');
        assert.equal(requests.filter(request => request.type === 'document').length, documents, 'Supported navigation keeps the original document.');
        // Home is outside the issue/list region. A failed classification must
        // preserve the ordinary link's canonical document navigation.
        const home = page.locator('.native-home-home-link');
        assert.equal(await home.getAttribute('href'), `${prefix}/`);
        const homeUrl = `${proxy.origin}${prefix}/`;
        const destinationUrl = `${proxy.origin}${prefix}/__native_workspace/destination`;
        let abortedClassifications = 0;
        await page.route(destinationUrl, async route => {
          assert.equal(route.request().method(), 'POST');
          abortedClassifications++;
          await route.abort('failed');
        });
        const fallback = page.waitForResponse(response => response.request().resourceType() === 'document' &&
          response.request().method() === 'GET' && response.url() === homeUrl);
        await home.click();
        const fallbackResponse = await fallback;
        assert.equal(fallbackResponse.status(), 200, 'Failed classifier still reaches the real mounted Home route.');
        assert.ok((await fallbackResponse.text()).includes('data-native-home'), 'Fallback renders actual production Home.');
        await page.waitForURL(homeUrl, {waitUntil:'domcontentloaded'});
        await page.locator('[data-native-home]').waitFor();
        assert.equal(abortedClassifications, 1, 'Only the genuine workspace destination request was aborted.');
        assert.equal(requests.filter(request => request.type === 'document').length, documents + 1,
          'Unsupported fallback performs exactly one ordinary document navigation.');
        assert.equal(await page.evaluate(() => Object.prototype.hasOwnProperty.call(window, 'workspaceParent')), false,
          'Ordinary Home fallback replaces the issue workspace document.');
        assert.ok(!requests.some(request => /\/api\//.test(new URL(request.url).pathname)), 'No existing JSON API calls.');
        assert.ok(!requests.some(request => request.authorization), 'Native transport uses the cookie session.');
        assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')), null);
        assert.deepEqual(errors, []);
      } finally { await context.close(); await proxy.close(); }
    });
  } finally { await browser.close(); input.close(); }
});
