const {test} = require('node:test');
const assert = require('node:assert/strict');
const {execFileSync} = require('node:child_process');
const {startFixture} = require('./server.js');

async function downloadedBytes(download) {
  assert.equal(await download.failure(), null);
  const stream = await download.createReadStream(), chunks = [];
  for await (const chunk of stream) chunks.push(chunk);
  return Buffer.concat(chunks);
}

test('production resources upload, export and publish through authenticated and anonymous browser routes',
  {skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH, timeout: 120000}, async t => {
    const fixture = await startFixture();
    const privatePage = await fixture.newPage();
    privatePage.setDefaultTimeout(10000);
    const errors = [];
    privatePage.on('pageerror', error => errors.push(error.message));
    const bytes = Buffer.from('Acceptance attachment bytes\nA second line with Unicode: café.\n');
    let attachment;
    try {
      await t.test('authenticated issue upload persists attachment metadata and exact download bytes', async () => {
        await privatePage.goto(fixture.url(`/${fixture.project.identifier}/issues/${fixture.issue.identifier}`));
        const input = privatePage.locator('[data-issue-attachments] [data-attachment-files]');
        await input.setInputFiles({name: 'acceptance-notes.txt', mimeType: 'text/plain', buffer: bytes});
        const uploaded = privatePage.waitForResponse(response =>
          new URL(response.url()).pathname === '/app/api/attachments' && response.request().method() === 'POST');
        await privatePage.locator('[data-issue-attachments]').getByRole('button', {name: 'Upload', exact: true}).click();
        const response = await uploaded;
        assert.equal(response.status(), 200);
        assert.equal((await response.request().allHeaders()).authorization, `Bearer ${fixture.token}`);
        attachment = await response.json();
        assert.equal(attachment.filename, 'acceptance-notes.txt');
        assert.equal(attachment.size, bytes.length);
        const card = privatePage.locator(`[data-issue-attachment-list] [data-attachment-id="${attachment.id}"]`);
        await card.getByRole('link', {name: 'acceptance-notes.txt', exact: true}).waitFor();
        await card.getByRole('button', {name: 'Preview acceptance-notes.txt', exact: true}).click();
        await card.locator('[data-attachment-content]').filter({hasText: 'A second line with Unicode: café.'}).waitFor();
        const persisted = await fixture.api(`/attachments?entity_type=issue&entity_id=${fixture.issue.id}`);
        assert.equal(persisted.status, 200);
        assert.ok((await persisted.json()).some(row => row.id === attachment.id));
        const content = await fixture.api(`/attachments/${attachment.id}`);
        assert.equal(content.status, 200);
        assert.deepEqual(Buffer.from(await content.arrayBuffer()), bytes);
        const download = privatePage.waitForEvent('download');
        await card.getByRole('link', {name: 'acceptance-notes.txt', exact: true}).click();
        const saved = await download;
        assert.equal(saved.suggestedFilename(), 'acceptance-notes.txt');
        assert.deepEqual(await downloadedBytes(saved), bytes);
      });

      await t.test('issue and page Markdown exports and project bundles contain persisted content', async () => {
        for (const [kind, record] of [['issues', fixture.issue], ['pages', fixture.page]]) {
          const response = await fixture.api(`/export/${kind}/${record.identifier}`);
          assert.equal(response.status, 200);
          assert.match(response.headers.get('content-type'), /^text\/markdown/);
          assert.match(response.headers.get('content-disposition'), /attachment/);
          assert.ok((await response.text()).includes(record.title));
        }
        const response = await fixture.api(`/export/projects/${fixture.project.identifier}?format=json`);
        assert.equal(response.status, 200);
        const bundle = await response.json();
        assert.equal(bundle.root, fixture.project.identifier);
        assert.ok(bundle.files.some(file => file.content.includes(fixture.issue.title)));
        assert.ok(bundle.files.some(file => file.content.includes(fixture.page.title)));
        await privatePage.goto(fixture.url(`/${fixture.project.identifier}/pages/${fixture.page.id}`));
        await privatePage.waitForFunction(title => document.querySelector('[data-page-title]')?.value === title,
          fixture.page.title);
        const download = privatePage.waitForEvent('download');
        await privatePage.getByRole('button', {name: 'Export Markdown', exact: true}).click();
        const saved = await download;
        assert.match(saved.suggestedFilename(), /\.md$/);
        assert.ok((await downloadedBytes(saved)).toString().includes(fixture.page.title));
      });

      await t.test('project ZIP export saves a filename matching its downloaded bundle', async () => {
        await privatePage.goto(fixture.url(`/${fixture.project.identifier}/settings`));
        const response = privatePage.waitForResponse(response => response.url().includes('/api/export/projects/'));
        const download = privatePage.waitForEvent('download');
        await privatePage.getByRole('button', {name: 'Export project data', exact: true}).click();
        const saved = await download;
        const exported = await response;
        assert.equal(new URL(exported.url()).searchParams.get('format'), null);
        assert.match(exported.headers()['content-type'], /^application\/zip/);
        assert.match(exported.headers()['content-disposition'], /attachment;.*filename="?[^";]+\.zip/);
        const content = await downloadedBytes(saved);
        assert.equal(content.subarray(0, 4).toString('hex'), '504b0304');
        assert.equal(saved.suggestedFilename(), `${fixture.project.identifier}.zip`);
      });

      await t.test('anonymous published project, issue, page and downloads omit stored credentials', async () => {
        const published = await fixture.api(`/projects/${fixture.project.id}`, {method: 'PUT', body: {is_public: true}});
        assert.equal(published.status, 200);
        const publicPage = await fixture.newPage({authenticated: false});
        publicPage.setDefaultTimeout(10000);
        publicPage.on('pageerror', error => errors.push(error.message));
        // A private token remaining in this browser must never accompany public reads.
        await publicPage.addInitScript(token => localStorage.setItem('lific_token', token), fixture.token);
        await publicPage.context().addCookies([{name: 'lific_token', value: fixture.token,
          url: fixture.origin, httpOnly: true, sameSite: 'Lax'}]);
        const requests = [];
        publicPage.on('request', request => {
          if (new URL(request.url()).pathname.startsWith('/app/public/api/')) requests.push(request);
        });
        for (const [route, expected] of [
          [`/public/${fixture.project.identifier}/issues`, fixture.issue.title],
          [`/public/${fixture.project.identifier}/issues/${fixture.issue.identifier}`, fixture.issue.title],
          [`/public/${fixture.project.identifier}/pages`, fixture.page.title],
          [`/public/${fixture.project.identifier}/pages/${fixture.page.id}`, fixture.page.title],
        ]) {
          await publicPage.goto(fixture.url(route));
          await publicPage.waitForFunction(expected => {
            const content = document.querySelector('[data-public-content]');
            return content && !content.hidden && content.textContent.includes(expected);
          }, expected);
          assert.equal(await publicPage.locator('[data-public-error]').isVisible(), false);
        }
        await publicPage.goto(fixture.url(`/public/${fixture.project.identifier}/issues/${fixture.issue.identifier}`));
        const download = publicPage.waitForEvent('download');
        await publicPage.locator(`[data-public-download="${attachment.id}"]`).click();
        assert.deepEqual(await downloadedBytes(await download), bytes);
        assert.ok(requests.length > 0);
        for (const request of requests) {
          const headers = await request.allHeaders();
          assert.equal(headers.authorization, undefined, request.url());
          assert.equal(headers.cookie, undefined, request.url());
        }
        await publicPage.close();
      });

      await t.test('real Ogg media serves exact byte ranges and seeks in the anonymous preview', async () => {
        const media = execFileSync('ffmpeg', ['-hide_banner', '-loglevel', 'error', '-f', 'lavfi',
          '-i', 'sine=frequency=440:sample_rate=48000', '-t', '3', '-c:a', 'libopus', '-f', 'ogg', 'pipe:1']);
        const body = new FormData();
        body.append('file', new Blob([media], {type: 'audio/ogg'}), 'acceptance-audio.ogg');
        body.append('entity_type', 'issue');body.append('entity_id', String(fixture.issue.id));
        const uploaded = await fetch(fixture.url('/api/attachments'), {method: 'POST', body,
          headers: {Authorization: `Bearer ${fixture.token}`}});
        assert.equal(uploaded.status, 200, await uploaded.clone().text());
        const audio = await uploaded.json();
        const path = `/public/api/projects/${fixture.project.identifier}/attachments/${audio.id}`;
        const range = await fetch(fixture.url(path), {headers: {Range: 'bytes=10-29'}});
        assert.equal(range.status, 206);
        assert.equal(range.headers.get('content-range'), `bytes 10-29/${media.length}`);
        assert.deepEqual(Buffer.from(await range.arrayBuffer()), media.subarray(10, 30));
        const page = await fixture.newPage({authenticated: false});
        page.setDefaultTimeout(10000);
        page.on('pageerror', error => errors.push(error.message));
        await page.goto(fixture.url(`/public/${fixture.project.identifier}/issues/${fixture.issue.identifier}`));
        await page.locator(`[data-public-attachment="${audio.id}"] [data-public-preview]`).click();
        const player = page.locator(`[data-public-attachment="${audio.id}"] audio`);
        await player.waitFor();
        await page.waitForFunction(id => {
          const player = document.querySelector(`[data-public-attachment="${id}"] audio`);
          return player?.readyState >= 1 && player.duration > 2;
        }, audio.id);
        const seek = await player.evaluate(async player => {
          const completed = new Promise(resolve => player.addEventListener('seeked', resolve, {once: true}));
          player.currentTime = 1.5;
          await completed;
          await player.play();player.pause();
          return {time: player.currentTime, duration: player.duration, error: player.error?.message};
        });
        assert.ok(seek.time >= 1.4 && seek.time < 2);
        assert.ok(seek.duration > 2);
        assert.equal(seek.error, undefined);
        await page.close();
      });
      assert.deepEqual(errors, []);
    } finally {await fixture.close();}
  });
