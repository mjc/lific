// Actual production Home: shared service publications and real runtime sockets.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const readline = require('node:readline');
const {mountedProxy, launchBrowser} = require('./browser_fixture.cjs');

const upstream = new URL(process.argv[2]);
const fixture = JSON.parse(process.argv[4]), scenario = process.argv[5];
const initialTitle = 'Visible active initial work';
const pending = new Map();
let sequence = 0;
const replies = readline.createInterface({input: process.stdin, terminal: false});
replies.on('line', line => {
  const result = JSON.parse(line), request = pending.get(result.id);
  assert.ok(request, 'Fixture replies acknowledge known non-secret control IDs.');
  pending.delete(result.id);
  clearTimeout(request.timer);
  request.resolve(result);
});
function control(action, fields = {}) {
  const id = ++sequence;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Home live fixture did not acknowledge ${action}.`));
    }, 7000);
    pending.set(id, {resolve, reject, timer});
    process.stdout.write(`@lific-fixture:home-live:${JSON.stringify({id, action, ...fields})}\n`);
  });
}
async function resources(sockets, receivers) {
  const result = await control('wait_count', {sockets, receivers});
  assert.equal(result.sockets, sockets, 'The real server owns exactly the expected native socket permits.');
  assert.equal(result.receivers, receivers, 'The current content lifetime owns exactly the expected session receiver.');
  return result;
}
function contentSockets(sockets) {
  return sockets.filter(socket => new URL(socket.url).pathname.endsWith('/__native_home/content'));
}
async function waitClosed(socket) {
  let timer;
  try {
    await Promise.race([socket.closing, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('The disconnected content socket did not close.')), 7000);
    })]);
  } finally {clearTimeout(timer);}
}

test(`native Home live production ${scenario}`, async t => {
  assert.ok(['live', 'reconnect', 'membership', 'late_auth'].includes(scenario));
  const browser = await launchBrowser();
  try {
    for (const [index, prefix] of ['', '/app', '/ACC'].entries()) {
      await t.test(prefix || 'root', async () => {
        const proxy = await mountedProxy(upstream, prefix);
        const context = await browser.newContext();
        const errors = [], requests = [], sockets = [], frames = [], inputs = [];
        try {
          await control('restore_member');
          await control('edit', {title: initialTitle, hidden: false});
          const baseline = await resources(0, 0);
          assert.equal(baseline.eventReceivers, 1, 'Only the independent fixture publication observer starts subscribed.');
          await context.addCookies([{name: 'lific_token', value: fixture.tokens[index], url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
          const page = await context.newPage();
          page.setDefaultTimeout(7000);
          page.on('console', message => {if (message.type() === 'error') errors.push(message.text());});
          page.on('pageerror', error => errors.push(error.message));
          context.on('request', request => requests.push(request));
          page.on('websocket', socket => {
            let close;
            const record = {url: socket.url(), closed: false, closing: new Promise(resolve => {close = resolve;})};
            sockets.push(record);
            socket.on('close', () => {record.closed = true; close();});
            socket.on('framereceived', ({payload}) => {
              try {frames.push({url: socket.url(), frame: JSON.parse(payload.toString())});}
              catch {errors.push('A native Home socket delivered a non-JSON frame.');}
            });
          });
          await page.exposeFunction('recordHomeLiveInput', event => inputs.push(event));
          await page.addInitScript(() => {
            window.__homeLiveDocument = crypto.randomUUID();
            for (const type of ['focus', 'storage', 'input', 'keydown', 'pointerdown']) {
              window.addEventListener(type, () => window.recordHomeLiveInput({type, document: window.__homeLiveDocument}), {capture: true});
            }
            // Test-only observation of the already existing framework socket.
            // Closing it performs a real WebSocket disconnect; no extra client,
            // fabricated render request, or production application state exists.
            const send = WebSocket.prototype.send;
            let contentSocket;
            WebSocket.prototype.send = function (value) {
              if (new URL(this.url).pathname.endsWith('/__native_home/content')) contentSocket = this;
              return send.call(this, value);
            };
            window.__disconnectHomeContent = () => new Promise(resolve => {
              contentSocket.addEventListener('close', resolve, {once: true});
              contentSocket.close();
            });
          });
          await page.goto(`${proxy.origin}${prefix}/`);
          const work = page.locator('.tc-native-home__page');
          await work.getByText(initialTitle, {exact: true}).waitFor();
          await page.waitForFunction(() =>
            document.querySelector('.tc-native-home__page')?.getAttribute('data-native-home-connected') === 'true' &&
            document.querySelector('.native-home-palette-results')?.getAttribute('data-native-home-connected') === 'true');
          await resources(2, 1);
          assert.deepEqual(sockets.map(socket => new URL(socket.url).pathname).sort(),
            [`${prefix}/__native_home/content`, `${prefix}/__native_home/palette`].sort());
          assert.deepEqual(await page.locator('script[src]').evaluateAll(elements => elements.map(element => new URL(element.src).pathname)),
            [`${prefix}/__topcoat-runtime.js`]);
          const document = await page.evaluate(() => window.__homeLiveDocument);
          const documentRequests = requests.filter(request => request.isNavigationRequest()).length;
          const inputCount = inputs.length;
          const palette = sockets.find(socket => new URL(socket.url).pathname.endsWith('/__native_home/palette'));
          const contentFrames = () => frames.filter(frame => new URL(frame.url).pathname.endsWith('/__native_home/content')).length;

          // A hidden-project update must not invalidate this account's Home.
          const beforeHidden = contentFrames();
          const hiddenTitle = `Private hidden live edit ${scenario}-${index}`;
          await control('edit', {title: hiddenTitle, hidden: true});
          await new Promise(resolve => setTimeout(resolve, 500));
          assert.equal(contentFrames(), beforeHidden, 'A hidden-project event cannot refresh this account\'s content.');
          assert.equal(await work.getByText(hiddenTitle, {exact: true}).count(), 0);
          assert.equal(await work.locator('a[href*="/HIDE/"]').count(), 0);

          if (scenario === 'membership') {
            // This contract covers Home's live body. The independently owned
            // shell project catalog has no live subscription in this slice.
            await control('remove_member');
            await work.getByText(initialTitle, {exact: true}).waitFor({state: 'hidden'});
            assert.equal(await work.getByText('Visible todo initial work', {exact: true}).count(), 0,
              'Removing membership erases all formerly visible project work.');
            assert.equal(await work.locator('a[href*="/ACC/"]').count(), 0,
              'Fresh content contains no formerly authorized project destination.');
            assert.equal(await work.getByText('Visible project', {exact: true}).count(), 0);
            assert.equal(await work.locator('[data-home-section="activity"]').count(), 0,
              'Fresh content erases the removed project activity projection.');
            const afterRemoval = contentFrames();
            const newlyPrivate = `Newly private publication ${index}`;
            await control('edit', {title: newlyPrivate, hidden: false});
            await new Promise(resolve => setTimeout(resolve, 500));
            assert.equal(contentFrames(), afterRemoval, 'Later events for the removed project no longer invalidate Home.');
            assert.equal(await work.getByText(newlyPrivate, {exact: true}).count(), 0);
            assert.equal(await work.getByText(hiddenTitle, {exact: true}).count(), 0);
            assert.equal(await page.evaluate(() => window.__homeLiveDocument), document);
            assert.equal(requests.filter(request => request.isNavigationRequest()).length, documentRequests);
            assert.equal(inputs.length, inputCount);
            assert.ok(sockets.every(socket => !socket.closed), 'Membership projection refresh preserves both existing connections.');
            assert.ok(requests.every(request => !new URL(request.url()).pathname.split('/').includes('api')));
            assert.deepEqual(errors, []);
            const connected = await resources(2, 1);
            assert.equal(connected.eventReceivers, 2);
            return;
          }

          if (scenario === 'late_auth') {
            const oldSockets = [...sockets];
            await context.addCookies([{name: 'lific_token', value: fixture.replacementToken, url: proxy.origin, httpOnly: true, sameSite: 'Lax'}]);
            await control('expire', {index});
            const navigated = page.waitForEvent('domcontentloaded', {timeout: 7000});
            const title = `Late authority publication ${index}`;
            await control('edit', {title, hidden: false});
            await navigated;
            assert.ok(frames.some(({frame}) => frame.t === 'redirect' && frame.location === `${prefix}/`),
              'Ordinary publication with expired A authority retires to mounted Home for current cookie B.');
            assert.equal(frames.some(({frame}) => frame.t === 'redirect' && frame.location === `${prefix}/login`), false,
              'A valid replacement cookie must not be forced onto Login by stale socket authority.');
            assert.equal(page.url(), `${proxy.origin}${prefix}/`);
            await work.getByText(title, {exact: true}).waitFor();
            await work.getByText(hiddenTitle, {exact: true}).waitFor();
            assert.equal(await page.locator('.native-home-account').textContent(), 'admin');
            assert.notEqual(await page.evaluate(() => window.__homeLiveDocument), document);
            assert.equal(requests.filter(request => request.isNavigationRequest()).length, documentRequests + 1,
              'Late authentication retirement makes exactly one fresh document request.');
            assert.equal(inputs.filter(event => event.document === document).length, inputCount,
              'Old Home discovers expired authority through publication without any input or focus event.');
            await Promise.all(oldSockets.map(waitClosed));
            await page.waitForFunction(() =>
              document.querySelector('.tc-native-home__page')?.getAttribute('data-native-home-connected') === 'true' &&
              document.querySelector('.native-home-palette-results')?.getAttribute('data-native-home-connected') === 'true');
            const connected = await resources(2, 1);
            assert.equal(connected.viewerSockets, 0, 'The expired account releases both old socket permits.');
            assert.equal(connected.replacementSockets, 2, 'The fresh document owns only current account B sockets.');
            assert.equal(connected.eventReceivers, 2);
            assert.ok(requests.every(request => !new URL(request.url()).pathname.split('/').includes('api')));
            assert.deepEqual(errors, []);
            return;
          }

          if (scenario === 'reconnect') {
            const old = contentSockets(sockets)[0];
            await page.evaluate(() => window.__disconnectHomeContent());
            await waitClosed(old);
            const missed = `Visible edit while content disconnected ${index}`;
            const edit = await control('edit', {title: missed, hidden: false});
            assert.equal(contentSockets(sockets).length, 1,
              'The missed edit commits before the framework opens its replacement content connection.');
            await work.getByText(missed, {exact: true}).waitFor();
            assert.equal(contentSockets(sockets).length, 2, 'The framework establishes one replacement content socket.');
            assert.ok(!palette.closed, 'Content recovery keeps the sibling palette connection alive.');
            assert.equal(await work.getByText(initialTitle, {exact: true}).count(), 0);
            await assertActivity(work, edit.titleRows);
            await resources(2, 1);
          }

          const title = `Visible live canary ${scenario}-${index}`;
          const edit = await control('edit', {title, hidden: false});
          await work.getByText(title, {exact: true}).waitFor();
          await assertActivity(work, edit.titleRows);
          assert.equal(await work.getByText(initialTitle, {exact: true}).count(), 0);
          assert.equal(await work.getByText(hiddenTitle, {exact: true}).count(), 0);
          assert.equal(await page.evaluate(() => window.__homeLiveDocument), document,
            'A hub mutation updates the existing Home document.');
          assert.equal(requests.filter(request => request.isNavigationRequest()).length, documentRequests);
          assert.equal(inputs.length, inputCount, 'Live refresh and reconnect require no focus, storage or user input.');
          assert.ok(requests.every(request => !new URL(request.url()).pathname.split('/').includes('api')), 'Home live refresh uses no REST.');
          assert.ok(requests.every(request => request.method() !== 'POST' || !new URL(request.url()).pathname.endsWith('/__native_home/content')),
            'Connected content never falls back to an HTTP shard POST.');
          assert.deepEqual(errors, [], 'Every browser and framework error remains visible to this test.');
          const active = sockets.filter(socket => !socket.closed);
          assert.deepEqual(active.map(socket => new URL(socket.url).pathname).sort(),
            [`${prefix}/__native_home/content`, `${prefix}/__native_home/palette`].sort());
          const connected = await resources(2, 1);
          assert.equal(connected.eventReceivers, 2, 'One content-owned live subscription joins the fixture observer.');
        } finally {
          try {
            await context.close();
            const disposed = await resources(0, 0);
            assert.equal(disposed.eventReceivers, 1, 'Disconnect releases the native event listener without an extra poller.');
          } finally {await proxy.close();}
        }
      });
    }
  } finally {
    try {await browser.close();}
    finally {
      replies.close();
      process.stdin.destroy();
      for (const request of pending.values()) clearTimeout(request.timer);
      pending.clear();
    }
  }
});

async function assertActivity(work, expectedRows) {
  const rows = work.locator('[data-home-section="activity"] .tc-home-sections__activity')
    .filter({hasText: `changed title on ${fixture.identifier}`});
  await rows.first().waitFor();
  assert.equal(await rows.count(), expectedRows, 'Home activity matches the original authorized eight-row project feed.');
}
