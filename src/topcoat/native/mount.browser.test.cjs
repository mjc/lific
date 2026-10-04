const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

const runtime = fs.readFileSync(path.join(__dirname, '../assets/runtime.js'), 'utf8');
const escape = value => value.replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
const signal = (id, value) => `<!-- ::topcoat::signal(${JSON.stringify({t: 'signal', id, v: value})}) -->`;
const handler = (event, expression) => `data-topcoat-on:${event}="${escape(expression)}"`;
const shard = (content, expressions) => `<!-- ::topcoat::shard::start("/shard", "1", [${expressions.map(expression => `"${escape(expression)}"`).join(', ')}]) -->${content}<!-- ::topcoat::shard::end("1") -->`;

const settle = page => page.evaluate(() => new Promise(resolve => {
  requestAnimationFrame(() => requestAnimationFrame(resolve));
}));

// Exercise the shipped framework in Chromium over HTTP. No replacement runtime,
// private scope exports, synthetic AbortController, or application controller.
async function fixture(content, initialState, run, onPost) {
  assert.ok(process.env.PLAYWRIGHT_EXECUTABLE_PATH, 'Use the repository e2e Chromium environment.');
  const {chromium} = await import(path.resolve(__dirname, '../../../e2e/node_modules/playwright/index.mjs'));
  const browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
  const requests = [], errors = [], pageErrors = [], serverErrors = [];
  const server = http.createServer(async (request, response) => {
    try {
      if (request.url === '/runtime.js') {
        response.setHeader('Content-Type', 'text/javascript');
        response.end(runtime);
        return;
      }
      if (request.method === 'POST') {
        const chunks = [];
        for await (const chunk of request) chunks.push(chunk);
        const body = JSON.parse(Buffer.concat(chunks).toString());
        requests.push({path: request.url, body, headers: request.headers});
        if (!onPost) {
          response.writeHead(500);
          response.end('Unexpected render');
          return;
        }
        response.setHeader('Content-Type', 'application/x-ndjson');
        response.end(`${JSON.stringify({t: 'snapshot', html: onPost(body)})}\n`);
        return;
      }
      response.setHeader('Content-Type', 'text/html');
      response.end(`<!doctype html><html><head><script>Object.assign(window, ${JSON.stringify(initialState)});</script></head><body>
        ${content}
        <script type="module">
          try { await import('/runtime.js'); }
          catch (error) { window.runtimeStartupError = String(error); }
          window.runtimeReady = true;
        </script>
      </body></html>`);
    } catch (error) {
      serverErrors.push(error.message);
      response.writeHead(500);
      response.end('Fixture failed');
    }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    page.setDefaultTimeout(5000);
    page.on('pageerror', error => pageErrors.push(error.message));
    page.on('console', message => {
      if (message.type() === 'error' && message.text().startsWith('[topcoat]')) errors.push(message.text());
    });
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForFunction(() => window.runtimeReady === true);
    await settle(page);
    await run({page, requests, errors, pageErrors});
    assert.deepEqual(serverErrors, []);
  } finally {
    await context.close();
    await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
}

test('mount defers its factory until later signal declarations and does not render without a change', async () => {
  const mount = `(() => {
    window.factoryCalls++;
    const value = cx.signal('a').get().toString();
    return event => {
      window.mountCalls++;
      window.mountValue = value;
      window.mountTargets = [event.target.id.toString(), event.current_target.id.toString()];
    };
  })()`;
  const content = `<section id="mount" ${handler('mount', mount)}>Initial content</section>
    ${signal('a', 'declared later')}
    <button ${handler('click', '() => { window.clickCalls++; }')}>Normal event</button>`;
  await fixture(content, {factoryCalls: 0, mountCalls: 0, clickCalls: 0}, async ({page, requests, errors, pageErrors}) => {
    assert.equal(await page.evaluate(() => window.runtimeStartupError), undefined,
      'Factory evaluation must wait until the later declaration exists.');
    assert.deepEqual(await page.evaluate(() => ({
      factoryCalls: window.factoryCalls,
      mountCalls: window.mountCalls,
      value: window.mountValue,
      targets: window.mountTargets,
      clickCalls: window.clickCalls,
    })), {
      factoryCalls: 1, mountCalls: 1, value: 'declared later', targets: ['mount', 'mount'], clickCalls: 0,
    });
    await page.getByRole('button', {name: 'Normal event'}).click();
    assert.equal(await page.evaluate(() => window.clickCalls), 1);
    assert.equal(requests.length, 0, 'A mount that changes no signal makes no request.');
    assert.deepEqual(errors, []);
    assert.deepEqual(pageErrors, []);
  });
});

test('a persistent initialization sentinel causes one render and a reused shard element mounts once per scope', async () => {
  const mount = `() => {
    window.mountCalls++;
    window.initialMount ??= document.querySelector('#mount');
    if (cx.signal('a').get().toNodeText() === 'false') {
      window.collections++;
      cx.signal('b').set(cx.hydrate('browser input'));
      cx.signal('a').set(cx.hydrate(true));
    }
  }`;
  const fragment = text => `<section id="mount" ${handler('mount', mount)}><output id="state">${text}</output></section>`;
  // Page-owned inputs survive replacement of the shard's content.
  const content = `${signal('a', false)}${signal('b', '')}${shard(fragment('Initial'), [
    'cx.signal("a").get()', 'cx.signal("b").get()',
  ])}`;
  await fixture(content, {mountCalls: 0, collections: 0}, async ({page, requests, errors, pageErrors}) => {
    assert.equal(await page.evaluate(() => window.collections), 1, 'Mount initializes once without a click.');
    await page.locator('#state').filter({hasText: 'Rendered browser input'}).waitFor();
    await settle(page);
    assert.equal(await page.evaluate(() => window.mountCalls), 2, 'Initial and replacement scopes each mount once.');
    assert.equal(await page.evaluate(() => window.collections), 1, 'Remount retains initialized input.');
    assert.equal(await page.evaluate(() => window.initialMount === document.querySelector('#mount')), true,
      'The real morph retains the element while replacing its owning scope.');
    assert.equal(requests.length, 1, 'Two initialization signal writes batch into one shard render.');
    assert.equal(requests[0].path, '/shard');
    assert.deepEqual(requests[0].body.args, [true, 'browser input']);
    await settle(page);
    assert.equal(requests.length, 1, 'The persistent sentinel prevents a render loop.');
    assert.deepEqual(errors, []);
    assert.deepEqual(pageErrors, []);
  }, () => fragment('Rendered browser input'));
});

test('page scope disposal cancels its queued factory while the reused element remains connected', async () => {
  const mount = `(() => {
    window.factoryCalls++;
    return () => { window.mountCalls++; };
  })()`;
  const content = `<section id="mount" ${handler('mount', mount)}>Retained element</section>
    <button ${handler('click', '() => { window.clickCalls++; }')}>Normal event</button>`;
  await fixture(content, {factoryCalls: 0, mountCalls: 0, clickCalls: 0}, async ({page, requests, errors, pageErrors}) => {
    assert.equal(await page.evaluate(() => window.mountCalls), 1);
    await page.evaluate(() => {
      const element = document.querySelector('#mount');
      const detail = {};
      window.dispatchEvent(new CustomEvent('topcoat:dev-runtime:v1', {detail}));
      if (!detail.runtime) throw new Error('The public framework refresh hook is missing.');
      // Both use RenderUnit.replace/Scope.release synchronously. Only the
      // latest live scope may evaluate its queued factory or invoke it.
      detail.runtime.replace(() => {});
      detail.runtime.replace(() => {});
      window.elementRetained = element === document.querySelector('#mount') && element.isConnected;
    });
    await settle(page);
    assert.equal(await page.evaluate(() => window.elementRetained), true);
    assert.equal(await page.evaluate(() => window.factoryCalls), 2, 'The disposed intermediate factory never evaluates.');
    assert.equal(await page.evaluate(() => window.mountCalls), 2, 'Only the latest new scope mounts.');
    await page.getByRole('button', {name: 'Normal event'}).click();
    assert.equal(await page.evaluate(() => window.clickCalls), 1, 'Scope release removes obsolete normal listeners.');
    assert.equal(requests.length, 0);
    assert.deepEqual(errors, []);
    assert.deepEqual(pageErrors, []);
  });
});

test('detached mounts are skipped and factory, synchronous and asynchronous errors stay isolated', async () => {
  const detached = `(() => {
    window.detachedFactories++;
    return () => { window.detachedCalls++; };
  })()`;
  const detach = `(() => {
    document.querySelector('#detached').remove();
    return () => {};
  })()`;
  const content = `<section id="detached" ${handler('mount', detached)}>Removed during hydration</section>
    <button ${handler('click', detach)}>Remove before queued mount</button>
    <section ${handler('mount', "(() => { throw new Error('factory mount failure'); })()")}>Factory failure</section>
    <section ${handler('mount', "() => { throw new Error('sync mount failure'); }")}>Synchronous failure</section>
    <section ${handler('mount', "async () => { throw new Error('async mount failure'); }")}>Asynchronous failure</section>
    <section ${handler('mount', '() => { window.goodCalls++; }')}>Unaffected sibling</section>`;
  await fixture(content, {detachedFactories: 0, detachedCalls: 0, goodCalls: 0}, async ({page, requests, errors, pageErrors}) => {
    assert.equal(await page.evaluate(() => window.runtimeStartupError), undefined,
      'A failing mount factory must not abort hydration.');
    assert.equal(await page.evaluate(() => window.goodCalls), 1);
    assert.equal(await page.evaluate(() => window.detachedFactories), 0, 'Detached mounts do not even evaluate their factory.');
    assert.equal(await page.evaluate(() => window.detachedCalls), 0);
    assert.equal(errors.length, 3, 'Each failed factory/callback is reported once through the runtime.');
    assert.deepEqual(pageErrors, [], 'No uncaught exception or unhandled rejection escapes.');
    assert.equal(requests.length, 0);
  });
});

test('a synchronous early mount event cannot consume initialization before later declarations', async () => {
  const mount = `(() => {
    window.factoryCalls++;
    const value = cx.signal('a').get().toString();
    return () => {
      window.mountCalls++;
      window.mountValue = value;
    };
  })()`;
  const earlyDispatch = `(() => {
    window.earlyDispatches++;
    document.querySelector('#mount').dispatchEvent(new window.Event('mount'));
    return () => {};
  })()`;
  const content = `<section id="mount" ${handler('mount', mount)}>Initial content</section>
    <button ${handler('click', earlyDispatch)}>Dispatch during hydration</button>
    ${signal('a', 'declared after early dispatch')}`;
  await fixture(content, {factoryCalls: 0, mountCalls: 0, earlyDispatches: 0}, async ({page, requests, errors, pageErrors}) => {
    assert.equal(await page.evaluate(() => window.runtimeStartupError), undefined);
    assert.deepEqual(await page.evaluate(() => ({
      earlyDispatches: window.earlyDispatches,
      factoryCalls: window.factoryCalls,
      mountCalls: window.mountCalls,
      value: window.mountValue,
    })), {
      earlyDispatches: 1,
      factoryCalls: 1,
      mountCalls: 1,
      value: 'declared after early dispatch',
    });
    assert.equal(requests.length, 0);
    assert.deepEqual(errors, []);
    assert.deepEqual(pageErrors, []);
  });
});

test('mount context exposes its owning AbortSignal and disposes global subscriptions', async () => {
  const mount = `(() => {
    if (!(cx.abortSignal instanceof AbortSignal)) throw new Error('Missing owning AbortSignal');
    window.mountSignals.push(cx.abortSignal);
    window.mountedValue = cx.signal('a').get().toString();
    window.addEventListener('scope-ping', () => { window.pings++; }, {signal: cx.abortSignal});
    return () => { window.mountCalls++; };
  })()`;
  const normal = `(() => {
    window.sharedContextHasSignal = 'abortSignal' in cx;
    return () => { window.clickCalls++; };
  })()`;
  const content = `<section id="mount" ${handler('mount', mount)}>Retained subscription owner</section>
    ${signal('a', 'later declaration')}
    <button ${handler('click', normal)}>Normal event</button>`;
  await fixture(content, {mountSignals: [], mountCalls: 0, pings: 0, clickCalls: 0}, async ({page, requests, errors, pageErrors}) => {
    assert.equal(await page.evaluate(() => window.mountCalls), 1, 'The factory receives the owning scope signal.');
    assert.equal(await page.evaluate(() => window.mountedValue), 'later declaration');
    assert.equal(await page.evaluate(() => window.sharedContextHasSignal), false, 'The shared runtime context must not be mutated.');
    await page.evaluate(() => {
      window.initialOwner = document.querySelector('#mount');
      window.dispatchEvent(new Event('scope-ping'));
    });
    assert.equal(await page.evaluate(() => window.pings), 1);
    for (const replacements of [2, 1]) {
      await page.evaluate(replacements => {
        const detail = {};
        window.dispatchEvent(new CustomEvent('topcoat:dev-runtime:v1', {detail}));
        for (let i = 0; i < replacements; i++) detail.runtime.replace(() => {});
      }, replacements);
      await settle(page);
      const state = await page.evaluate(() => {
        window.dispatchEvent(new Event('scope-ping'));
        return {
          liveSignals: window.mountSignals.filter(signal => !signal.aborted).length,
          pings: window.pings,
          calls: window.mountCalls,
          retained: window.initialOwner === document.querySelector('#mount') && window.initialOwner.isConnected,
          sharedContextHasSignal: window.sharedContextHasSignal,
        };
      });
      assert.equal(state.liveSignals, 1, 'Released scopes abort their global listeners even when the DOM node survives.');
      assert.equal(state.pings, state.calls, 'Each ping reaches exactly the current subscription.');
      assert.equal(state.retained, true);
      assert.equal(state.sharedContextHasSignal, false);
    }
    assert.equal(await page.evaluate(() => window.mountCalls), 3, 'The disposed intermediate scope never creates a subscription.');
    await page.getByRole('button', {name: 'Normal event'}).click();
    assert.equal(await page.evaluate(() => window.clickCalls), 1);
    assert.equal(requests.length, 0);
    assert.deepEqual(errors, []);
    assert.deepEqual(pageErrors, []);
  });
});
