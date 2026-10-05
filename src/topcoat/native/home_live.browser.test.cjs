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
  assert.ok(['live', 'activity_rate', 'reconnect', 'membership', 'late_auth', 'burst', 'continuous', 'hidden_projection', 'render_failure', 'owner_retirement', 'owner_snapshot', 'busy_success', 'busy_failure'].includes(scenario));
  const browser = await launchBrowser();
  try {
    for (const [index, prefix] of ['', '/app', '/ACC'].entries()) {
      await t.test(prefix || 'root', async () => {
        const proxy = await mountedProxy(upstream, prefix,
          scenario.startsWith('busy_') ? {incomingPath: '/__native_home/content'} : {});
        const context = await browser.newContext();
        const errors = [], requests = [], sockets = [], frames = [], sentFrames = [], inputs = [];
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
            socket.on('framesent', ({payload}) => {
              sentFrames.push({url: socket.url(), frame: JSON.parse(payload.toString())});
            });
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
          // Rate-only swaps are independent of content invalidation and projection.
          const contentFrames = () => frames.filter(({url, frame}) => new URL(url).pathname.endsWith('/__native_home/content') && /data-native-home-(?:invalidation|snapshot)/.test(JSON.stringify(frame))).length;

          // A hidden-project update must not invalidate this account's Home.
          const hiddenProjectionBaseline = (await control('count')).homeProjectionReads;
          const beforeHidden = contentFrames();
          const hiddenTitle = `Private hidden live edit ${scenario}-${index}`;
          await control('edit', {title: hiddenTitle, hidden: true});
          await new Promise(resolve => setTimeout(resolve, 500));
          assert.equal(contentFrames(), beforeHidden, 'A hidden-project event cannot refresh this account\'s content.');

          assert.equal(await work.getByText(hiddenTitle, {exact: true}).count(), 0);
          assert.equal(await work.locator('a[href*="/HIDE/"]').count(), 0);

          if (scenario === 'activity_rate') {
            const projections = (await control('count')).homeProjectionReads;
            let title;
            for (let edit = 0; edit < 3; edit++) {
              title = `Authorized rate edit ${index}-${edit}`;
              await control('edit', {title, hidden: false});
            }
            await work.getByText(title, {exact: true}).waitFor();
            const rate = work.locator('[data-native-home-activity-rate]');
            await rate.waitFor({state: 'visible'});
            assert.equal(await rate.getAttribute('title'),
              'Websocket activity rate; the day fallback includes the last 24 hours');
            await page.waitForFunction(() => document.querySelector('[data-native-home-activity-rate]')?.textContent.trim() === '3 updates/min');
            assert.equal((await control('count')).homeProjectionReads, projections + 1,
              'Rate updates preserve the three-event counter through the one quiet-burst projection.');
            const settled = contentFrames();
            await new Promise(resolve => setTimeout(resolve, 1200));
            assert.equal(await rate.textContent(), '3 updates/min');
            assert.equal((await control('count')).homeProjectionReads, projections + 1,
              'Independent rate ticks do not read the Home projection.');
            assert.equal(contentFrames(), settled, 'Rate ticks do not invalidate or render the Home projection.');
            await control('edit', {title: `Hidden rate edit ${index}`, hidden: true});
            await new Promise(resolve => setTimeout(resolve, 500));
            assert.equal(await rate.textContent(), '3 updates/min', 'Hidden audit events cannot enter this account rate.');
            assert.equal(contentFrames(), settled);
            assert.equal(await page.evaluate(() => window.__homeLiveDocument), document);
            assert.equal(requests.filter(request => request.isNavigationRequest()).length, documentRequests);
            assert.ok(requests.every(request => !new URL(request.url()).pathname.split('/').includes('api')));
            assert.deepEqual(errors, []);
            const connected = await resources(2, 2);
            assert.equal(connected.eventReceivers, 2, 'Activity rate shares the existing Home publication receiver.');
            return;
          }

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
              let previous = owner.querySelector('.tc-home-active').textContent;
              window.__homeRefreshes = [];
              const observer = new MutationObserver(() => {
                const current = owner.querySelector('.tc-home-active').textContent;
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

          if (scenario === 'owner_retirement' || scenario === 'owner_snapshot') {
            await page.clock.install();
            await page.clock.pauseAt(await page.evaluate(() => Date.now() + 1000));
            const queuedTitle = `Queued before parent retirement ${index}`;
            const applied = await page.evaluate(() => window.__homeLiveAppliedFrames.length);
            await control('edit', {title: queuedTitle, hidden: false});
            await eventually(() => page.evaluate(before => window.__homeLiveAppliedFrames.slice(before)
              .some(frame => frame.t === 'swap' && frame.html.includes('nativeHomeRealtime')), applied),
              'The genuine publication arms the old owner quiet/max timers.');
            await page.evaluate(() => {window.__retiredHomeOwner = document.querySelector('[data-native-home]');});
            const parentRecent = `Fresh recents on new owner ${index}`;
            await storeRecent(page, parentRecent);
            const beforeOwnerSnapshot = await page.evaluate(() => window.__homeLiveAppliedFrames.length);
            const parentResponse = await page.evaluate(async () => {
              const detail = {};
              window.dispatchEvent(new CustomEvent('topcoat:dev-runtime:v1', {detail}));
              if (!detail.runtime) throw new Error('The actual public parent refresh seam is required.');
              const response = await detail.runtime.request(new AbortController().signal);
              if (!response.ok || response.redirected) throw new Error(`Actual parent render failed: ${response.status}`);
              const parsed = new DOMParser().parseFromString(await response.text(), 'text/html');
              detail.runtime.replace(() => document.body.replaceChildren(
                ...Array.from(parsed.body.childNodes, node => document.importNode(node, true))));
              return {status: response.status, type: response.headers.get('content-type')};
            });
            assert.equal(parentResponse.status, 200);
            assert.match(parentResponse.type, /^text\/html/);
            await work.getByText(queuedTitle, {exact: true}).waitFor();
            await work.locator('[data-home-section="recents"]').getByText(parentRecent, {exact: true}).waitFor();
            await eventually(() => page.evaluate(() =>
              document.querySelector('.tc-native-home__page')?.getAttribute('data-native-home-connected') === 'true' &&
              document.querySelector('.native-home-palette-results')?.getAttribute('data-native-home-connected') === 'true'),
              'The actual replacement content and palette connect while the browser clock is paused.');
            await resources(2, 2);
            assert.deepEqual(await page.evaluate(() => ({
              distinct: window.__retiredHomeOwner !== document.querySelector('[data-native-home]'),
              oldConnected: window.__retiredHomeOwner.isConnected,
              oldCallback: typeof window.__retiredHomeOwner.nativeHomeRun,
              newCallback: typeof document.querySelector('[data-native-home]').nativeHomeRun,
            })), {distinct: true, oldConnected: false, oldCallback: 'undefined', newCallback: 'function'});
            const baseline = (await control('count')).homeProjectionReads;
            await page.clock.runFor(5000);
            assert.equal((await control('count')).homeProjectionReads, baseline,
              'Retired quiet/max deadlines cannot act on the replacement owner in the same Runtime.');
            if (scenario === 'owner_snapshot') {
              // The service publication was genuinely committed before parent
              // replacement. The new connection subscribes afterwards and
              // includes it in its initial snapshot, without an invalidation swap.
              const initialSnapshot = await page.evaluate(({before, title}) =>
                window.__homeLiveAppliedFrames.slice(before).some(frame =>
                  frame.t === 'snapshot' && frame.html.includes(title)),
                {before: beforeOwnerSnapshot, title: queuedTitle});
              assert.ok(initialSnapshot, 'The new owner receives the committed publication in its genuine connected snapshot.');
              assert.equal(await work.getByText(queuedTitle, {exact: true}).count(), 1);
              await eventually(() => ownerPublicationConsumed(page, beforeOwnerSnapshot, queuedTitle),
                'The shared publication barrier admits the actual new-owner snapshot.', async () => ({
                  prefix, title: queuedTitle, fixture: await control('count'), errors,
                  owner: await page.evaluate(({before, title}) => ({
                    newOwner: document.querySelector('[data-native-home]') !== window.__retiredHomeOwner,
                    oldOwnerDetached: !window.__retiredHomeOwner.isConnected,
                    exactTitleRendered: [...document.querySelector('[data-native-home]').querySelectorAll('.tc-dashboard__issue-title')]
                      .some(element => element.textContent === title),
                    frames: window.__homeLiveAppliedFrames.slice(before).map(frame => ({
                      t: frame.t, containsCommittedTitle: frame.html?.includes(title),
                      hasRealtimeCallback: frame.html?.includes('nativeHomeRealtime'),
                    })),
                  }), {before: beforeOwnerSnapshot, title: queuedTitle}),
                }));
            }
            await storeRecent(page, `Fresh during replacement burst ${index}`);
            let finalTitle;
            for (let step = 0; step < 21; step++) {
              const before = await page.evaluate(() => window.__homeLiveAppliedFrames.length);
              finalTitle = `Replacement owner continuous ${index}-${step}`;
              await control('edit', {title: finalTitle, hidden: false});
              await eventually(() => ownerPublicationConsumed(page, before, finalTitle),
                'The new owner processes the genuine publication.', async () => ({
                  prefix, step, title: finalTitle, appliedBefore: before, errors,
                  fixture: await control('count'),
                  contentFrames: frames.filter(({url}) => new URL(url).pathname.endsWith('/__native_home/content'))
                    .slice(-12).map(({frame}) => summarizePublication(frame, finalTitle)),
                  sentRuns: sentFrames.filter(({url}) => new URL(url).pathname.endsWith('/__native_home/content'))
                    .slice(-8).map(({frame}) => ({run: frame.run, shard: frame.shard})),
                  owner: await page.evaluate(({before, title}) => {
                    const owner = document.querySelector('[data-native-home]');
                    return {
                      currentOwnerConnected: owner?.isConnected,
                      oldOwnerDetached: !window.__retiredHomeOwner.isConnected,
                      currentOwnerHasRun: typeof owner?.nativeHomeRun,
                      exactTitleRendered: [...owner.querySelectorAll('.tc-dashboard__issue-title')]
                        .some(element => element.textContent === title),
                      connected: owner.querySelector('.tc-native-home__page')?.getAttribute('data-native-home-connected'),
                      observedFrames: window.__homeLiveAppliedFrames.slice(before).map(frame => ({
                        t: frame.t, region: frame.region, id: frame.id,
                        hasRealtimeCallback: frame.html?.includes('nativeHomeRealtime'),
                        containsCommittedTitle: frame.html?.includes(title),
                        htmlLength: frame.html?.length,
                      })),
                    };
                  }, {before, title: finalTitle}),
                }));
              if (step >= 19) assert.ok((await control('count')).homeProjectionReads > baseline,
                'The new owner has its own independent five-second maximum deadline.');
              if (step < 20) await page.clock.runFor(300);
            }
            const finalRecent = `Fresh replacement trailing recents ${index}`;
            await storeRecent(page, finalRecent);
            await page.clock.runFor(750);
            await work.getByText(finalTitle, {exact: true}).waitFor();
            await work.locator('[data-home-section="recents"]').getByText(finalRecent, {exact: true}).waitFor();
            assert.equal((await control('count')).homeProjectionReads, baseline + 2,
              'Replacement owner performs one maximum-wait projection and one final quiet projection.');
            assert.equal(await page.evaluate(() => window.__homeLiveDocument), document);
            assert.equal(requests.filter(request => request.isNavigationRequest()).length, documentRequests);
            assert.ok(requests.every(request => !new URL(request.url()).pathname.split('/').includes('api')));
            assert.ok(requests.every(request => request.method() !== 'POST' || !new URL(request.url()).pathname.endsWith('/__native_home/content')));
            assert.deepEqual(errors, []);
            await resources(2, 2);
            return;
          }

          if (scenario === 'busy_success' || scenario === 'busy_failure') {
            await page.clock.install();
            await page.clock.pauseAt(await page.evaluate(() => Date.now() + 1000));
            const baseline = (await control('count')).homeProjectionReads;
            const sentRuns = () => sentFrames.filter(({url, frame}) =>
              new URL(url).pathname.endsWith('/__native_home/content') && Number.isInteger(frame.run)).length;
            const beforeRuns = sentRuns(), failed = scenario === 'busy_failure';
            proxy.incomingGate.hold();
            let readerFault = false;
            try {
              if (failed) {await control('reader_fault', {enabled: true}); readerFault = true;}
              assert.equal(await page.evaluate(() => document.visibilityState), 'visible');
              await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
              await page.clock.runFor(50);
              await eventually(async () => {
                const bytes = proxy.incomingGate.text();
                return failed ? bytes.includes('"t":"error"') && bytes.includes('"status":500') :
                  bytes.includes('"t":"snapshot"') && bytes.includes(initialTitle);
              }, 'The actual server completion bytes are held, without delivering a replacement frame.');
              assert.equal(sentRuns(), beforeRuns + 1);
              assert.equal((await control('count')).homeProjectionReads, baseline + (failed ? 0 : 1));
              if (readerFault) {await control('reader_fault', {enabled: false}); readerFault = false;}
              const heldBeforePublication = proxy.incomingGate.text().length;
              const title = `Genuine pending work ${scenario}-${index}`;
              await control('edit', {title, hidden: false});
              if (!failed) await eventually(async () => proxy.incomingGate.text().slice(heldBeforePublication).includes('nativeHomeRealtime'),
                'The new actual publication is processed by the server while completion remains withheld.');
              // The publication stays behind the snapshot in genuine wire order.
              // Supported visibility flushes create pending in the busy browser.
              for (let tick = 0; tick < 2; tick++) {
                await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
                await page.clock.runFor(50);
              }
              assert.equal(sentRuns(), beforeRuns + 1, 'Busy flushes coalesce into one pending flag.');
              const recentTitle = `Fresh pending recents ${scenario}-${index}`;
              await storeRecent(page, recentTitle);
              proxy.incomingGate.release();
              await work.getByText(title, {exact: true}).waitFor();
              await work.locator('[data-home-section="recents"]').getByText(recentTitle, {exact: true}).waitFor();
              assert.equal(sentRuns(), beforeRuns + 2, 'Completion starts exactly one trailing render.');
              const expected = baseline + (failed ? 1 : 2);
              assert.equal((await control('count')).homeProjectionReads, expected);
              await page.clock.runFor(750);
              assert.equal(sentRuns(), beforeRuns + 2, 'Superseded held invalidations cannot add another refresh.');
              assert.equal((await control('count')).homeProjectionReads, expected);
              const before = await page.evaluate(() => window.__homeLiveAppliedFrames.length);
              const next = `Live after busy ${scenario}-${index}`;
              await control('edit', {title: next, hidden: false});
              await eventually(() => page.evaluate(before => window.__homeLiveAppliedFrames.slice(before)
                .some(frame => frame.t === 'swap' && frame.html.includes('nativeHomeRealtime')), before),
                'The successful trailing run retains genuine realtime subscription.');
              await page.clock.runFor(750);
              await work.getByText(next, {exact: true}).waitFor();
              assert.equal((await control('count')).homeProjectionReads, expected + 1);
              if (failed) {
                assert.equal(errors.length, 1);
                assert.match(errors[0], /^\[topcoat\] Error: Connected render failed: 500(?:\n {4}at [^\n]+)+$/);
              } else assert.deepEqual(errors, []);
              assert.equal(await page.evaluate(() => window.__homeLiveDocument), document);
              assert.equal(requests.filter(request => request.isNavigationRequest()).length, documentRequests);
              assert.ok(requests.every(request => !new URL(request.url()).pathname.split('/').includes('api')));
              assert.ok(requests.every(request => request.method() !== 'POST' || !new URL(request.url()).pathname.endsWith('/__native_home/content')));
              await resources(2, 2);
              return;
            } finally {
              if (readerFault) await control('reader_fault', {enabled: false});
            }
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
            const baseline = (await control('count')).activityDayCount;
            await work.locator('[data-native-home-activity-rate]').filter({hasText: `${baseline} updates/day`}).waitFor({state: 'visible'});
            assert.equal(await work.locator('[data-native-home-activity-rate]').textContent(), `${baseline} updates/day`,
              'A new physical connection reads a fresh authorized baseline including the missed audit event.');
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

async function ownerPublicationConsumed(page, before, title) {
  return page.evaluate(({before, title}) => {
    const owner = document.querySelector('[data-native-home]');
    return window.__homeLiveAppliedFrames.slice(before).some(frame =>
      (frame.t === 'swap' && frame.html.includes('nativeHomeRealtime')) ||
      ((frame.t === 'snapshot' || frame.t === 'swap') && frame.html.includes(title) &&
        owner !== window.__retiredHomeOwner && owner.isConnected &&
        [...owner.querySelectorAll('.tc-dashboard__issue-title')]
          .some(element => element.textContent === title)));
  }, {before, title});
}

function summarizePublication(frame, title) {
  return {t: frame.t, id: frame.id, region: frame.region,
    hasRealtimeCallback: frame.html?.includes('nativeHomeRealtime'),
    containsCommittedTitle: frame.html?.includes(title), htmlLength: frame.html?.length};
}

async function eventually(predicate, message, diagnostics) {
  const deadline = Date.now() + 7000;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  assert.fail(diagnostics ? `${message} ${JSON.stringify(await diagnostics())}` : message);
}

async function assertEventually(predicate) {
  const deadline = Date.now() + 7000;
  while (!predicate() && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 20));
  assert.ok(predicate(), 'The actual current content connection reports the expected server error frame.');
}

async function storeRecent(page, title) {
  await page.evaluate(({identifier, title}) => localStorage.setItem('lific_recents', JSON.stringify([
    {type: 'issue', routeId: identifier, identifier, title,
      project: identifier.replace(/-[0-9]+$/, ''), ts: Date.now()},
  ])), {identifier: fixture.identifier, title});
}
