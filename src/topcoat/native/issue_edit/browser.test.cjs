// Reusable native component through the real production server and cookie session.
// Feature assertions come from pinned master InlineTitle/EditableMarkdown/IssueDetail.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {mountedProxy} = require('../browser_fixture.cjs');
const upstream = new URL(process.argv[2]);
const token = process.argv[3];
const scenario = process.argv[4];
const output = '/tmp/lific-native-issue-editor';
const endpoint = '/__native_issue_edit/save/';
const selector = {
  title: '#native-issue-title-ACC-1', titleInput: '#native-issue-title-input-ACC-1',
  edit: '#native-issue-body-edit-ACC-1', body: '#native-issue-body-input-ACC-1',
  save: '[data-native-issue-body-save]', cancel: '[data-native-issue-body-cancel]',
  seq: '[data-native-issue-seq]', error: '[data-native-issue-save-error]',
};
const seq = async page => Number(await page.locator(selector.seq).textContent());
async function ready(page, url) {
  const response = await page.goto(url);
  assert.equal(response.status(), 200);
  await page.locator(selector.title).waitFor();
  await page.waitForFunction(() => document.querySelector('[data-native-issue-editor]') &&
    [...document.scripts].some(script => script.type === 'module' && script.src.includes('__topcoat-runtime.js')));
}
async function commit(page, field, action, tag = 'saved') {
  const before = await seq(page);
  const responsePromise = page.waitForResponse(response => response.request().method() === 'POST' &&
    new URL(response.url()).pathname.endsWith(`${endpoint}${field}`));
  await action();
  const response = await responsePromise;
  assert.equal(response.status(), 200);
  const wire = await response.json();
  assert.equal(wire[0][tag === 'conflict' ? 'err' : 'ok'], tag);
  await page.waitForFunction(before => Number(document.querySelector('[data-native-issue-seq]').textContent) > before, before);
  await page.waitForFunction(() => ![...document.querySelectorAll('[role="status"]')]
    .some(element => !element.hidden && element.textContent === 'Saving…'));
  return seq(page);
}
async function startTitle(page) {
  await page.locator(selector.title).click();
  await page.locator(selector.titleInput).waitFor({state: 'visible'});
  await page.waitForFunction(id => document.activeElement === document.getElementById(id), selector.titleInput.slice(1));
}
async function startBody(page) {
  await page.locator(selector.edit).click();
  await page.locator(selector.body).waitFor({state: 'visible'});
  await page.waitForFunction(id => document.activeElement === document.getElementById(id), selector.body.slice(1));
}

test(`native issue component ${scenario}`, async t => {
  assert.ok(['fields', 'conflict', 'failure'].includes(scenario));
  assert.ok(process.env.PLAYWRIGHT_EXECUTABLE_PATH, 'Use repository Chromium.');
  fs.mkdirSync(output, {recursive: true});
  const {chromium} = await import(path.resolve(__dirname, '../../../../e2e/node_modules/playwright/index.mjs'));
  const browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
  try {
    for (const prefix of ['', '/app', '/ACC']) await t.test(prefix || 'root', async () => {
      const proxy = await mountedProxy(upstream, prefix);
      const context = await browser.newContext({viewport: {width: 1440, height: 900}, reducedMotion: 'reduce'});
      const requests = [], errors = [], consoleErrors = [], expectedFailures = [], posts = [], dialogs = [];
      context.on('request', request => {
        requests.push({url: request.url(), method: request.method(), authorization: !!request.headers().authorization});
        if (request.method() === 'POST' && new URL(request.url()).pathname.includes(endpoint)) posts.push(request.url());
      });
      await context.addCookies([{name: 'lific_token', value: token, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
      context.on('page', page => {
        page.on('pageerror', error => errors.push(error.message));
        page.on('dialog', dialog => {dialogs.push(dialog.type()); dialog.dismiss();});
        page.on('console', message => {if (message.type() === 'error') consoleErrors.push({text: message.text(), url: message.location().url});});
        page.setDefaultTimeout(15000);
      });
      const page = await context.newPage();
      const url = `${proxy.origin}${prefix}/ACC/__native_issue_editor`;
      try {
        await ready(page, url);
        assert.equal(await page.locator('script[src]').count(), 1, 'Only the native framework runtime loads.');
        assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')), null, 'Cookie-only session.');
        if (scenario === 'fields') {
          const title = `Native title ${prefix || 'root'} --> --!><img src=x onerror=window.nativeHostile=1><script>window.nativeHostile=1</script>`;
          const beforePosts = posts.length;
          await startTitle(page);
          await page.locator(selector.titleInput).fill(`  ${title}  `);
          await commit(page, 'title', () => page.locator(selector.titleInput).press('Enter'));
          assert.equal(await page.locator(selector.title).textContent(), title);
          await page.waitForTimeout(150);
          assert.equal(posts.length, beforePosts + 1, 'Enter-triggered blur makes exactly one save.');
          for (const value of [`  ${title}  `, '   ']) {
            const before = await seq(page), count = posts.length;
            await startTitle(page);
            await page.locator(selector.titleInput).fill(value);
            await page.locator(selector.titleInput).press('Enter');
            await page.locator(selector.titleInput).waitFor({state: 'hidden'});
            await page.waitForTimeout(150);
            assert.equal(await seq(page), before);
            assert.equal(posts.length, count, 'Original trimmed unchanged/empty title makes no save call.');
            assert.equal(await page.locator(selector.title).textContent(), title);
          }
          await startTitle(page);
          await page.locator(selector.titleInput).fill('Discarded title draft');
          const canceledTitleSeq = await seq(page), canceledTitlePosts = posts.length;
          await page.locator(selector.titleInput).press('Escape');
          await page.locator(selector.titleInput).waitFor({state: 'hidden'});
          await page.waitForFunction(id => document.activeElement === document.getElementById(id), selector.title.slice(1));
          await page.waitForTimeout(150);
          assert.equal(await seq(page), canceledTitleSeq);
          assert.equal(posts.length, canceledTitlePosts, 'Escape-generated blur never commits.');

          const body = `  # Exact ${prefix || 'root'}\n\n--> --!><img src=x onerror=window.nativeHostile=1><script>window.nativeHostile = 1</script>\nACC-2 @viewer  \n`;
          await startBody(page);
          await page.locator(selector.body).fill(body);
          await commit(page, 'description', () => page.locator(selector.body).press('Control+s'));
          await page.locator(selector.body).waitFor({state: 'hidden'});
          assert.equal(await page.locator('.native-issue-editor__preview').textContent(), body);
          assert.equal(await page.evaluate(() => window.nativeHostile), undefined);
          await startBody(page);
          await page.locator(selector.body).fill('Discarded description');
          const beforeCancel = await seq(page), cancelPosts = posts.length;
          await page.locator(selector.cancel).click();
          await page.locator(selector.body).waitFor({state: 'hidden'});
          await page.waitForFunction(id => document.activeElement === document.getElementById(id), selector.edit.slice(1));
          assert.equal(await seq(page), beforeCancel);
          assert.equal(posts.length, cancelPosts);
          await startBody(page);
          assert.equal(await page.locator(selector.body).inputValue(), body, 'Cancel resets draft to saved value.');
          await page.locator(selector.body).press('Escape');
          await page.locator(selector.body).waitFor({state: 'hidden'});
          for (const [field, values] of [['status', ['done', 'todo']], ['priority', ['urgent', 'low']]]) {
            for (const value of values) {
              await commit(page, field, () => page.locator(`[data-native-issue-${field}-option="${value}"]`).click());
              assert.equal(await page.locator(`[data-native-issue-${field}]`).textContent(), value);
            }
          }
          await page.reload();
          await page.locator(selector.title).waitFor();
          assert.equal(await page.locator(selector.title).textContent(), title, 'Real Rust write survives reload.');
          assert.equal(await page.locator('.native-issue-editor__preview').textContent(), body);
          assert.equal(await page.locator('[data-native-issue-status]').textContent(), 'todo');
          assert.equal(await page.locator('[data-native-issue-priority]').textContent(), 'low');
          await startTitle(page);
          assert.equal(await page.locator(selector.titleInput).inputValue(), title,
            'Framework signal hydration preserves the exact hostile title after reload.');
          await page.locator(selector.titleInput).press('Escape');
          await startBody(page);
          assert.equal(await page.locator(selector.body).inputValue(), body,
            'Framework signal hydration preserves exact hostile body and whitespace after reload.');
          await page.locator(selector.body).press('Escape');
          assert.equal(await page.locator('.native-issue-editor img, .native-issue-editor script').count(), 0,
            'Hostile text never creates editor elements.');
          assert.equal(await page.evaluate(() => window.nativeHostile), undefined);
          assert.equal(requests.some(request => new URL(request.url).pathname.split('/').at(-1) === 'x'), false,
            'Hostile image text never makes a network request.');
        } else if (scenario === 'conflict') {
          await startBody(page);
          const draft = `Losing dirty description ${prefix || 'root'}\n\n  preserve exact whitespace  `;
          await page.locator(selector.body).fill(draft);
          const stale = await seq(page);
          const winner = await context.newPage();
          await ready(winner, url);
          await startTitle(winner);
          const winningTitle = `Other native writer ${prefix || 'root'}`;
          await winner.locator(selector.titleInput).fill(winningTitle);
          const winningSeq = await commit(winner, 'title', () => winner.locator(selector.titleInput).press('Enter'));
          assert.ok(winningSeq > stale);
          await commit(page, 'description', () => page.locator(selector.save).click(), 'conflict');
          assert.equal(await seq(page), winningSeq, 'Conflict uses the captured winner sequence.');
          assert.equal(await page.locator(selector.title).textContent(), winningTitle);
          assert.equal(await page.locator(selector.body).inputValue(), draft, 'Losing draft survives conflict.');
          assert.equal(await page.locator(selector.body).isVisible(), true);
          await page.locator(selector.error).filter({hasText: 'This issue changed. Your draft is still here.'}).waitFor();
          await commit(page, 'description', () => page.locator(selector.save).click());
          await page.locator(selector.body).waitFor({state: 'hidden'});
          assert.equal(await page.locator('.native-issue-editor__preview').textContent(), draft);
          await winner.reload();
          assert.equal(await winner.locator('.native-issue-editor__preview').textContent(), draft);
          await winner.close();
        } else {
          for (const mode of ['503', 'abort']) {
            for (const field of ['description', 'title']) {
              const draft = field === 'description'
                ? `  Exact failed ${mode} ${prefix || 'root'}\n\n  keep trailing spaces  \n`
                : `  Failed title ${mode} ${prefix || 'root'}  `;
              if (field === 'description') await startBody(page); else await startTitle(page);
              const input = page.locator(field === 'description' ? selector.body : selector.titleInput);
              await input.fill(draft);
              const before = await seq(page);
              const failureUrl = `${proxy.origin}${prefix}${endpoint}${field}`;
              expectedFailures.push({url: failureUrl, mode});
              let faultCount = 0;
              const fault = async route => {
                faultCount += 1;
                if (faultCount !== 1) return route.continue();
                if (mode === '503') await route.fulfill({status: 503, contentType: 'application/json',
                  body: JSON.stringify({error: 'Injected component transport failure'})});
                else await route.abort('failed');
              };
              await page.route(failureUrl, fault);
              const attempted = page.waitForRequest(request => request.method() === 'POST' && request.url() === failureUrl);
              if (field === 'description') await page.locator(selector.save).click(); else await input.press('Enter');
              await attempted;
              await page.waitForFunction(() => ![...document.querySelectorAll('[role="status"]')]
                .some(element => !element.hidden && element.textContent === 'Saving…'), null, {timeout: 5000});
              assert.equal(faultCount, 1, 'One actual native POST receives the injected transport failure.');
              assert.equal(await seq(page), before, 'Failed request never advances observed sequence.');
              assert.equal(await input.isVisible(), true, 'Failed save retains the editor.');
              assert.equal(await input.inputValue(), draft, 'Exact dirty draft survives transport failure.');
              await page.locator(selector.error).waitFor({state: 'visible'});
              assert.ok((await page.locator(selector.error).textContent()).trim(), 'Failure is visible to the editor.');
              if (field === 'description') assert.equal(await page.locator(selector.save).isEnabled(), true);
              else await page.waitForFunction(id => document.activeElement === document.getElementById(id), selector.titleInput.slice(1));
              assert.deepEqual(errors, [], 'Failed native request is handled without an unhandled browser rejection.');
              await page.unroute(failureUrl, fault);
              await commit(page, field, () => field === 'description'
                ? page.locator(selector.save).click() : input.press('Enter'));
              await input.waitFor({state: 'hidden'});
              await page.reload();
              await page.locator(selector.title).waitFor();
              assert.equal(await page.locator(field === 'description' ? '.native-issue-editor__preview' : selector.title).textContent(),
                field === 'description' ? draft : draft.trim(), 'Actual retry persists through reload.');
            }
          }
        }
        assert.equal(requests.some(request => new URL(request.url).pathname.split('/').includes('api')), false,
          'Every origin and unmounted path stays outside REST.');
        assert.equal(requests.some(request => request.authorization), false, 'Native actions use actual cookie identity.');
        assert.ok(posts.length > 0);
        assert.ok(posts.every(url => new URL(url).pathname.startsWith(`${prefix}${endpoint}`)), 'Native procedure paths mount once.');
        assert.deepEqual(errors, []);
        assert.deepEqual(dialogs, [], 'Hostile text never opens a dialog.');
        const unexpectedConsoleErrors = consoleErrors.filter(error => !expectedFailures.some(failure =>
          error.url === failure.url && (failure.mode === '503'
            ? /^Failed to load resource: the server responded with a status of 503(?: |$)/.test(error.text)
            : /^Failed to load resource: net::ERR_FAILED$/.test(error.text))));
        assert.deepEqual(unexpectedConsoleErrors, [], 'Only the precise injected failed-resource diagnostics are expected.');
      } finally {
        const report = label => error => process.stderr.write(`${scenario} ${prefix || 'root'} ${label}: ${error.message}\n`);
        try {
          await page.screenshot({path: path.join(output, `${scenario}-${prefix.slice(1) || 'root'}.png`),
            fullPage: true, timeout: 5000}).catch(report('screenshot'));
          try {fs.writeFileSync(path.join(output, `${scenario}-${prefix.slice(1) || 'root'}.json`),
            JSON.stringify({requests, proxyRequests: proxy.requests, errors, consoleErrors, expectedFailures, dialogs}, null, 2));}
          catch (error) {report('diagnostics')(error);}
        } finally {
          try {await context.close().catch(report('context cleanup'));}
          finally {await proxy.close().catch(report('proxy cleanup'));}
        }
      }
    });
  } finally {await browser.close();}
});
