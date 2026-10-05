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
  assert.equal(result.receivers, receivers, 'The physical sockets own exactly the expected session authority receivers.');
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
  assert.ok(['live', 'reconnect', 'membership', 'late_auth', 'burst', 'continuous', 'hidden_projection', 'render_failure'].includes(scenario));
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
            window.__homeLiveAppliedFrames = [];
            WebSocket.prototype.send = function (value) {
              if (new URL(this.url).pathname.endsWith('/__native_home/content') && contentSocket !== this) {
                contentSocket = this;
                // Registered after the framework receive handler. Its hydration
                // queues mount effects before this observation microtask.
                this.addEventListener('message', event => {
                  const frame = JSON.parse(event.data);
                  if (frame.t === 'snapshot' || frame.t === 'swap') {
                    queueMicrotask(() => {window.__homeLiveAppliedFrames.push(frame);});
                  }
                });
              }
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
          await resources(2, 2);
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
          const hiddenProjectionBaseline = (await control('count')).homeProjectionReads;
          const beforeHidden = contentFrames();
          const hiddenTitle = `Private hidden live edit ${scenario}-${index}`;
          await control('edit', {title: hiddenTitle, hidden: true});
          await new Promise(resolve => setTimeout(resolve, 500));
          assert.equal(contentFrames(), beforeHidden, 'A hidden-project event cannot refresh this account\'s content.');

          assert.equal(await work.getByText(hiddenTitle, {exact: true}).count(), 0);
          assert.equal(await work.locator('a[href*="/HIDE/"]').count(), 0);

          if (scenario === 'hidden_projection') {
            // The real next visible publication is ordered after the hidden
            // one in the same receiver. Its final rendered title is a positive
            // consumption barrier, independent of the earlier500ms observation.
            const barrierTitle = `Visible hidden-audience barrier ${index}`;
            await control('edit', {title: barrierTitle, hidden: false});
            await work.getByText(barrierTitle, {exact: true}).waitFor();
            assert.equal((await control('count')).homeProjectionReads, hiddenProjectionBaseline + 1,
              'Exactly the visible barrier projects Home; the preceding unauthorized publication performs no projection.');
            assert.deepEqual(errors, []);
            assert.equal(await page.evaluate(() => window.__homeLiveDocument), document);
            assert.equal(requests.filter(request => request.isNavigationRequest()).length, documentRequests);
            assert.ok(requests.every(request => !new URL(request.url()).pathname.split('/').includes('api')));
            await resources(2, 2);
            return;
          }

          if (scenario === 'burst' || scenario === 'continuous') {
            const settledTitle = `Scheduling baseline ${scenario}-${index}`;
            await control('edit', {title: settledTitle, hidden: false});
            await work.getByText(settledTitle, {exact: true}).waitFor();
            await page.clock.install();
            await page.clock.pauseAt(await page.evaluate(() => Date.now() + 1000));
            const projectionBaseline = (await control('count')).homeProjectionReads;
            await page.evaluate(() => {
              const owner = document.querySelector('[data-native-home]');
              let previous = owner.querySelector('.tc-native-home__page').textContent;
              window.__homeRefreshes = [];
              const observer = new MutationObserver(() => {
                const current = owner.querySelector('.tc-native-home__page').textContent;
                if (current !== previous) {
                  previous = current;
                  window.__homeRefreshes.push({time: performance.now(), text: current});
                }
              });
              observer.observe(owner, {subtree: true, childList: true, characterData: true});
              window.__stopHomeRefreshObservation = () => observer.disconnect();
            });
            const refreshCount = () => page.evaluate(() => window.__homeRefreshes.length);
            const before = await refreshCount();
            let elapsed = 0;
            const length = scenario === 'burst' ? 3 : 21;
            let finalTitle;
            for (let step = 0; step < length; step++) {
              finalTitle = `Scheduled Home ${scenario}-${index}-${step}`;
              const applied = await page.evaluate(() => window.__homeLiveAppliedFrames.length);
              const received = contentFrames();
              await control('edit', {title: finalTitle, hidden: false});
              await eventually(async () => frames.slice().filter(({url}) =>
                  new URL(url).pathname.endsWith('/__native_home/content')).slice(received)
                  .some(({frame}) => (frame.t === 'swap' && frame.html.includes('data-native-home-invalidation')) ||
                    ((frame.t === 'snapshot' || frame.t === 'swap') && frame.html.includes(finalTitle))) &&
                await page.evaluate(({before, title}) => window.__homeLiveAppliedFrames.slice(before)
                  .some(frame => (frame.t === 'swap' && frame.html.includes('data-native-home-invalidation')) ||
                    ((frame.t === 'snapshot' || frame.t === 'swap') && frame.html.includes(title))), {before: applied, title: finalTitle}),
                'A genuine server publication frame is received and applied before browser time advances.');
              if (scenario === 'burst') {
                assert.equal(await refreshCount(), before,
                  'An edit burst does not render before its quiet deadline.');
              } else if (elapsed >= 5500) {
                assert.ok((await control('count')).homeProjectionReads > projectionBaseline,
                  'The independent maximum deadline invokes the actual shared Home projection during continuous events.');
                assert.ok(await refreshCount() > before,
                  'Continuous real publications cannot postpone every render beyond five seconds.');
              }
              if (step + 1 < length) {
                await page.clock.runFor(300);
                elapsed += 300;
              }
            }
            if (scenario === 'burst') {
              await page.clock.runFor(400);
              assert.equal(await refreshCount(), before,
                'The last edit still has less than 750ms quiet time.');
              assert.equal((await control('count')).homeProjectionReads, projectionBaseline,
                'Real publications perform no shared Home projection before the quiet flush.');
            }
            const recentTitle = `Fresh local recent at flush ${scenario}-${index}`;
            await page.evaluate(({identifier, title}) => localStorage.setItem('lific_recents', JSON.stringify([
              {type: 'issue', routeId: identifier, identifier, title,
                project: identifier.replace(/-[0-9]+$/, ''), ts: Date.now()},
            ])), {identifier: fixture.identifier, title: recentTitle});
            assert.equal(await work.getByText(recentTitle, {exact: true}).count(), 0,
              'Writing current-tab recents does not itself fabricate a refresh.');
            await page.clock.runFor(scenario === 'burst' ? 350 : 750);
            await work.getByText(finalTitle, {exact: true}).waitFor();
            await work.locator('[data-home-section="recents"]').getByText(recentTitle, {exact: true}).waitFor();
            if (scenario === 'burst') {
              assert.equal(await refreshCount(), before + 1,
                'One final authorized snapshot replaces the entire quiet burst.');
              assert.equal((await control('count')).homeProjectionReads, projectionBaseline + 1,
                'The burst invokes the actual shared Home projection exactly once.');
            }
            assert.equal(await page.evaluate(() => window.__homeLiveDocument), document);
            assert.equal(requests.filter(request => request.isNavigationRequest()).length, documentRequests);
            assert.ok(requests.every(request => !new URL(request.url()).pathname.split('/').includes('api')));
            assert.deepEqual(errors, []);
            await resources(2, 2);
            await page.evaluate(() => window.__stopHomeRefreshObservation());
            return;
          }

          if (scenario === 'render_failure') {
            // Genuine server read failure, not a synthetic socket frame. Sessions
            // and caller rows are restored intact before the recovery attempt.
            await control('reader_fault', {enabled: true});
            try {
              // Exercise Home's supported visibility listener on this visible
              // tab without also invoking the shell's independent session check.
              assert.equal(await page.evaluate(() => document.visibilityState), 'visible');
              await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
              await assertEventually(() => frames.some(({url, frame}) =>
                new URL(url).pathname.endsWith('/__native_home/content') && frame.t === 'error' && frame.status === 500));
              assert.equal(await work.getByText(initialTitle, {exact: true}).count(), 1,
                'An unsuccessful background read retains the previously rendered Home.');
            } finally {await control('reader_fault', {enabled: false});}
            const recovered = `Recovered after failed Home read ${index}`;
            await control('edit', {title: recovered, hidden: false});
            await page.evaluate(() => window.dispatchEvent(new Event('focus')));
            await work.getByText(recovered, {exact: true}).waitFor();
            const next = `Live after failed Home read ${index}`;
            await control('edit', {title: next, hidden: false});
            await work.getByText(next, {exact: true}).waitFor();
            assert.equal(await page.evaluate(() => window.__homeLiveDocument), document);
            assert.equal(requests.filter(request => request.isNavigationRequest()).length, documentRequests);
            assert.ok(requests.every(request => !new URL(request.url()).pathname.split('/').includes('api')));
            assert.ok(requests.every(request => request.method() !== 'POST' || !new URL(request.url()).pathname.endsWith('/__native_home/content')));
            assert.equal(errors.length, 1,
              'Exactly the deliberately induced server failure is reported; every unrelated error fails.');
            assert.match(errors[0], /^\[topcoat\] Error: Connected render failed: 500(?:\n {4}at [^\n]+)+$/,
              'The sole error is the actual framework failure with its browser stack.');
            await resources(2, 2);
            return;
          }

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
            const connected = await resources(2, 2);
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
            const connected = await resources(2, 2);
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
            await resources(2, 2);
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
          const connected = await resources(2, 2);
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

async function eventually(predicate, message) {
  const deadline = Date.now() + 7000;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  assert.fail(message);
}

async function assertEventually(predicate) {
  const deadline = Date.now() + 7000;
  while (!predicate() && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 20));
  assert.ok(predicate(), 'The actual current content connection reports the expected server error frame.');
}
