// Real production Home and pinned master, using one disposable authenticated fixture.
// node home.browser.test.cjs <fixture-origin> <session-token> [pinned-master-web-directory]
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {createHash} = require('node:crypto');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {mountedProxy, launchBrowser} = require('./browser_fixture.cjs');

const snapshot = process.argv[4];
const output = '/tmp/lific-native-home-production';
const fixedTime = '2026-10-03T16:00:00Z';
const runtimeVersion = createHash('sha256').update(fs.readFileSync(path.resolve(__dirname, '../assets/runtime.js'))).digest('hex');
function checkpoint(message) {
  const line = `${new Date().toISOString()} ${message}\n`;
  process.stderr.write(line);
  fs.appendFileSync(path.join(output, 'progress.log'), line);
}

async function bounded(label, operation, timeout = 15000) {
  checkpoint(`${label}: begin`);
  let timer;
  try {
    const value = await Promise.race([
      operation(),
      new Promise((_, reject) => {timer = setTimeout(() => reject(new Error(`${label} exceeded ${timeout}ms`)), timeout);}),
    ]);
    checkpoint(`${label}: done`);
    return value;
  } finally {clearTimeout(timer);}
}
const recents = [
  {type: 'issue', routeId: 'ACC-1', identifier: 'ACC-1', project: 'ACC', ts: Date.parse(fixedTime),
    title: '<img src=x onerror="globalThis.__homeInjected=true"> & recent issue'},
  {type: 'page', routeId: '1', identifier: 'ACC-DOC-1', project: 'ACC', ts: Date.parse(fixedTime) - 1000,
    title: '<script>globalThis.__homeInjected=true</script> recent page'},
  {type: 'plan', routeId: '1', identifier: 'ACC-PLAN-1', project: 'ACC', ts: Date.parse(fixedTime) - 2000,
    title: 'Plan "quote" & <b>text</b>'},
];

async function newContext(browser, origin, token, viewport, theme, master = false) {
  const context = await browser.newContext({
    viewport: {width: viewport.width, height: viewport.height}, hasTouch: viewport.name === 'phone',
    isMobile: viewport.name === 'phone', colorScheme: theme, reducedMotion: 'reduce',
    locale: 'en-US', timezoneId: 'America/Denver',
  });
  if (!master) await context.addCookies([{name: 'lific_token', value: token, url: origin, httpOnly: true, sameSite: 'Lax'}]);
  await context.addInitScript(({token, theme, master, recents}) => {
    if (master) localStorage.setItem('lific_token', token);
    localStorage.setItem('lific_theme', theme);
    localStorage.setItem('lific_motion', 'reduced');
    localStorage.setItem('lific_font_scale', 'md');
    localStorage.setItem('lific_recents', JSON.stringify(recents));
    // Native Home ships only the framework script. Set the browser's existing
    // appearance primitive directly for these fixtures; application state stays Rust.
    const applyTheme = () => {if (document.documentElement) document.documentElement.dataset.theme = theme;};
    applyTheme();
    document.addEventListener('DOMContentLoaded', applyTheme, {once: true});
  }, {token, theme, master, recents});
  const page = await context.newPage();
  await page.clock.setFixedTime(fixedTime);
  page.setDefaultTimeout(15000);
  const errors = [], consoleErrors = [], requests = [], frames = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => {if (message.type() === 'error') consoleErrors.push(message.text());});
  context.on('request', request => requests.push({method: request.method(), url: request.url()}));
  page.on('websocket', socket => {
    socket.on('framesent', ({payload}) => frames.push({url: socket.url(), payload: payload.toString()}));
  });
  return {context, page, errors, consoleErrors, requests, frames};
}

function assertOutsideRest(requests) {
  assert.equal(requests.some(({url}) => new URL(url).pathname.split('/').includes('api')), false,
    'Production native Home makes no REST requests, including other-origin or unmounted requests.');
}

async function geometry(page) {
  return page.evaluate(() => [...document.querySelectorAll('main, aside, h1, h2, header, button')].slice(0, 55).map(element => ({
    tag: element.tagName, text: element.textContent.trim().slice(0, 100),
    rect: element.getBoundingClientRect().toJSON(), font: getComputedStyle(element).font,
    background: getComputedStyle(element).backgroundColor,
  })));
}

async function diagnostics(state, proxy, name) {
  const report = {
    errors: state.errors, consoleErrors: state.consoleErrors, requests: state.requests,
    proxyRequests: proxy.requests, sockets: proxy.sockets, frames: state.frames,
    dom: await state.page.evaluate(() => ({
      greeting: document.getElementById('native-home-greeting')?.textContent,
      date: document.getElementById('native-home-date')?.textContent,
      collapsed: document.querySelector('.native-home-shell')?.getAttribute('data-collapsed'),
      handlers: [...document.querySelectorAll('[data-topcoat-on\\:click]')].map(element => ({id: element.id, handler: element.getAttribute('data-topcoat-on:click')})),
      html: document.body.innerHTML.slice(0, 80000),
    })).catch(error => ({diagnosticError: error.message})),
  };
  fs.writeFileSync(path.join(output, `${name}-failure.json`), JSON.stringify(report, null, 2));
  await state.page.screenshot({path: path.join(output, `${name}-failure.png`)}).catch(() => {});
  return report;
}

async function assertNativeHome(state, proxy, prefix) {
  const {page} = state;
  checkpoint(`native ${prefix || 'root'}: navigation`);
  const response = await page.goto(`${proxy.origin}${prefix}/`);
  assert.equal(response.status(), 200);
  const initial = await response.text();
  for (const text of ['data-native-home', 'Visible active initial work', 'Visible todo initial work', 'Visible project']) {
    assert.ok(initial.includes(text), `Initial server HTML must already contain ${text}.`);
  }
  assert.equal(initial.includes('Private hidden'), false, 'Initial HTML must not contain inaccessible project or issue data.');
  assert.equal(initial.includes('Loading your dashboard'), false);
  assert.deepEqual(await page.locator('script[src]').evaluateAll(elements => elements.map(element => element.getAttribute('src'))),
    [`${prefix}/__topcoat-runtime.js?v=${runtimeVersion}`], 'The production Home loads only the framework runtime.');
  await page.locator('#native-home-greeting').filter({hasText: 'Good morning, viewer'}).waitFor();
  assert.equal(await page.locator('#native-home-greeting').textContent(), 'Good morning, viewer');
  assert.equal(await page.locator('#native-home-date').textContent(), 'Saturday, October 3');
  assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')), null,
    'Native Home uses the cookie session without seeding the legacy browser token.');
  checkpoint(`native ${prefix || 'root'}: local clock rendered`);
  const rows = page.locator('[data-home-section="recents"] a');
  assert.equal(await rows.count(), 3);
  for (const [index, entry] of recents.entries()) {
    const segment = {issue: 'issues', page: 'pages', plan: 'plans'}[entry.type];
    assert.ok((await rows.nth(index).textContent()).includes(entry.title));
    assert.equal(await rows.nth(index).getAttribute('href'), `${prefix}/ACC/${segment}/${entry.routeId}`);
  }
  assert.equal(await page.locator('[data-home-section="recents"] img, [data-home-section="recents"] script, [data-home-section="recents"] b').count(), 0);
  assert.equal(await page.evaluate(() => globalThis.__homeInjected), undefined);
  assert.equal(proxy.requests.some(request => request.method === 'POST'), false,
    'Initial browser clock/storage collection does not invoke a procedure or HTTP render.');
  assertOutsideRest(state.requests);
  assert.ok(proxy.sockets.some(url => url === `${prefix}/__native_home/content`), 'Browser-local initialization uses the real mounted content socket.');
  const shell = page.locator('.native-home-shell');
  assert.equal(await shell.getAttribute('data-collapsed'), 'false');
  await page.locator('#native-home-collapse').click();
  await page.waitForFunction(() => document.querySelector('.native-home-shell')?.dataset.collapsed === 'true');
  assert.equal(await page.locator('#native-home-collapse').getAttribute('aria-expanded'), 'false');
  await page.locator('#native-home-collapse').click();
  await page.waitForFunction(() => document.querySelector('.native-home-shell')?.dataset.collapsed === 'false');
  const dialog = page.getByRole('dialog', {name: 'Jump to project'});
  await page.locator('#native-home-quick-jump').click();
  await dialog.waitFor({state: 'visible'});
  const results = page.locator('.native-home-palette-results');
  await results.getByRole('link', {name: /Visible project/}).waitFor();
  assert.equal(await results.getByRole('link', {name: /Private hidden/}).count(), 0);
  await page.locator('#native-home-palette-query').fill('private');
  await results.getByText('No matching projects', {exact: true}).waitFor();
  await page.locator('#native-home-palette-query').fill('ACC');
  const visibleProject = results.getByRole('link', {name: /Visible project/});
  await visibleProject.waitFor();
  assert.equal(await visibleProject.getAttribute('href'), `${prefix}/ACC/overview`);
  await page.locator('#native-home-palette-close').click();
  await dialog.waitFor({state: 'hidden'});
  await page.locator('#native-home-palette-open').click();
  await dialog.waitFor({state: 'visible'});
  await page.locator('#native-home-palette-close').click();
  await dialog.waitFor({state: 'hidden'});
  assertOutsideRest(state.requests);
  assert.deepEqual(state.errors, [], 'Rust-authored native handlers must execute without browser errors.');
  assert.deepEqual(state.consoleErrors, [], 'Framework handler evaluation and cross-shard signals must not fail silently.');
  checkpoint(`native ${prefix || 'root'}: controls and catalog passed`);
}

test('production native Home uses Rust state and mounted transport; capture paired original Home', {timeout: 240000}, async t => {
  assert.ok(process.argv[2] && process.argv[3], 'Supply fixture-origin and session-token as arguments.');

  const upstream = new URL(process.argv[2]), token = process.argv[3];
  assert.equal(upstream.protocol, 'http:', 'The ephemeral fixture is a local HTTP server.');
  assert.ok(['127.0.0.1', 'localhost', '[::1]'].includes(upstream.hostname), 'Use the disposable loopback fixture.');
  fs.mkdirSync(output, {recursive: true});
  fs.writeFileSync(path.join(output, 'progress.log'), '');
  checkpoint(`start native Home; optional master ${Boolean(snapshot)}`);

  const browser = await launchBrowser();
  const desktop = {name: 'desktop', width: 1440, height: 900};
  const phone = {name: 'phone', width: 390, height: 844};
  const report = [];
  let vite;
  const viteProxySockets = new Set();
  try {
    for (const prefix of ['', '/app', '/ACC']) await t.test(prefix || 'root', async () => {
      const proxy = await mountedProxy(upstream, prefix);
      const state = await newContext(browser, proxy.origin, token, desktop, 'light');
      try {
        await assertNativeHome(state, proxy, prefix);
      } catch (error) {
        const failure = await diagnostics(state, proxy, `home-${prefix.slice(1) || 'root'}`);
        throw new Error(`Production Home ${prefix || 'root'} failed; diagnostic artifact saved: ${JSON.stringify({errors: failure.errors, consoleErrors: failure.consoleErrors, sockets: failure.sockets, greeting: failure.dom.greeting})}`, {cause: error});
      } finally {await state.context.close(); await proxy.close();}
    });
    await t.test('dark panel, palette and input use the actual dark surface tokens', async () => {
      checkpoint('dark surface regression: begin');
      const proxy = await mountedProxy(upstream, '');
      const state = await newContext(browser, proxy.origin, token, desktop, 'dark');
      try {
        await state.page.goto(`${proxy.origin}/`);
        await state.page.locator('#native-home-greeting').filter({hasText: 'Good morning, viewer'}).waitFor();
        await state.page.locator('#native-home-quick-jump').click();
        await state.page.getByRole('dialog', {name: 'Jump to project'}).waitFor({state: 'visible'});
        const backgrounds = await state.page.evaluate(() =>
          ['.native-home-panel', '.native-home-palette', '.native-home-palette input'].map(selector => ({
            selector, background: getComputedStyle(document.querySelector(selector)).backgroundColor,
          })));
        await state.page.screenshot({path: path.join(output, 'home-dark-surfaces-native.png')});
        for (const {selector, background} of backgrounds) {
          assert.ok(['rgb(13, 17, 16)', 'rgb(37, 44, 41)'].includes(background),
            `${selector} must use an existing dark background/surface token, got ${background}.`);
        }
        assert.deepEqual(state.errors, []);
        checkpoint('dark surface regression: passed');
      } catch (error) {
        await diagnostics(state, proxy, 'home-dark-surfaces');
        throw error;
      } finally {await state.context.close(); await proxy.close();}
    });
    for (const prefix of ['', '/app', '/ACC']) for (const route of ['/ACC/issues', '/public/ACC/issues']) {
      await t.test(`legacy bookmark ${prefix || 'root'} ${route}`, async () => {
        checkpoint(`bookmark ${prefix || 'root'} ${route}: begin`);
        const proxy = await mountedProxy(upstream, prefix);
        const state = await newContext(browser, proxy.origin, token, desktop, 'light');
        try {
          // The target is still an existing hybrid detail/list route. Its
          // established session bridge reads localStorage, unlike native Home.
          await state.page.addInitScript(token => localStorage.setItem('lific_token', token), token);
          await state.page.goto(`${proxy.origin}${prefix}/#${route}?from=bookmark#details`);
          await state.page.waitForURL(url => url.pathname === `${prefix}${route}` && url.search === '?from=bookmark' && url.hash === '#details');
          const canonical = new URL(state.page.url());
          assert.equal(canonical.pathname, `${prefix}${route}`);
          assert.equal(canonical.search, '?from=bookmark');
          assert.equal(canonical.hash, '#details');
          checkpoint(`bookmark ${prefix || 'root'} ${route}: canonical URL passed`);
        } catch (error) {
          await diagnostics(state, proxy, `home-bookmark-${prefix.slice(1) || 'root'}-${route.includes('/public/') ? 'public' : 'private'}`);
          throw error;
        } finally {await state.context.close(); await proxy.close();}
      });
    }
    let masterOrigin;
    if (snapshot) {
      const {createServer} = await bounded('import pinned Vite', () => import(pathToFileURL(path.join(snapshot, 'node_modules/vite/dist/node/index.js')).href));
      const configure = proxy => proxy.on('open', socket => {
        // Vite's HTTP server tracks browser sockets. Its proxy's upstream
        // realtime socket is separately owned and also needs disposal.
        viteProxySockets.add(socket);
        socket.once('close', () => viteProxySockets.delete(socket));
      });
      vite = await createServer({root: snapshot, logLevel: 'silent', configFile: path.join(snapshot, 'vite.config.ts'), server: {
        host: '127.0.0.1', port: 0, strictPort: false, proxy: {
          '/api': {target: upstream.origin, ws: true, configure},
          '/public/api': {target: upstream.origin, ws: true, configure},
        },
      }});
      await bounded('start pinned Vite', () => vite.listen());
      masterOrigin = `http://127.0.0.1:${vite.httpServer.address().port}`;
    }
    const proxy = await mountedProxy(upstream, '');
    try {
      for (const viewport of [desktop, phone]) for (const theme of ['light', 'dark']) for (const implementation of masterOrigin ? ['native', 'master'] : ['native']) {
        const master = implementation === 'master', origin = master ? masterOrigin : proxy.origin;
        const state = await newContext(browser, origin, token, viewport, theme, master);
        const name = `home-${viewport.name}-${theme}-${implementation}`;
        try {
          checkpoint(`${name}: navigate`);
          await state.page.goto(master ? `${origin}/#/` : `${origin}/`, {waitUntil: 'networkidle'});
          if (master) {
            await state.page.getByRole('heading', {name: 'Good morning, viewer', exact: true}).waitFor();
            await state.page.getByText('Visible active initial work', {exact: true}).waitFor();
            await state.page.getByText('Visible todo initial work', {exact: true}).waitFor();
          } else {
            await state.page.locator('#native-home-greeting').filter({hasText: 'Good morning, viewer'}).waitFor();
            assertOutsideRest(state.requests);
          }
          await bounded(`${name}: fonts`, () => state.page.evaluate(() => document.fonts.ready));
          await state.page.screenshot({path: path.join(output, `${name}.png`)});
          report.push({name, errors: state.errors, consoleErrors: state.consoleErrors, geometry: await geometry(state.page)});
          assert.deepEqual(state.errors, [], `${name} must render without browser errors.`);
          checkpoint(`${name}: screenshot and geometry saved`);
        } finally {await state.context.close();}
      }
    } finally {await proxy.close();}
  } finally {
    fs.writeFileSync(path.join(output, 'geometry.json'), JSON.stringify(report, null, 2));
    checkpoint('capture report saved; cleanup begins');
    for (const socket of viteProxySockets) socket.destroy();
    try {
      if (vite) await bounded('close pinned Vite', () => vite.close());
    } finally {await bounded('close Chromium', () => browser.close());}
    checkpoint(`cleanup done; active resource types ${JSON.stringify(process.getActiveResourcesInfo())}`);
  }
});
