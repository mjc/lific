const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const os = require('node:os');
const {spawn} = require('node:child_process');
const {once} = require('node:events');
const {createHash} = require('node:crypto');

const listen = server => new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const close = server => new Promise(resolve => server.close(resolve));

test('cold detail back and prefixed session requests work through a stripping reverse proxy',
  {skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH, timeout: 30000}, async () => {
    const {chromium} = await import(path.resolve(__dirname, '../../../..', 'e2e/node_modules/playwright/index.mjs'));
    const scripts = {
      '/__topcoat-shell.js': fs.readFileSync(`${__dirname}/shell.js`, 'utf8'),
      '/__topcoat-session.js': fs.readFileSync(path.resolve(__dirname, '../../session.rs'), 'utf8')
        .match(/BROWSER_SCRIPT: &str = r#"([\s\S]*?)"#;/)[1],
    };
    const upstreamPaths = [];
    const upstream = http.createServer((request, response) => {
      upstreamPaths.push(request.url);
      if (scripts[request.url]) {
        response.writeHead(200, {'content-type': 'text/javascript'}).end(scripts[request.url]);
      } else if (request.url.startsWith('/api/') || request.url.startsWith('/public/api/')) {
        response.writeHead(200, {'content-type': 'application/json'}).end(JSON.stringify({id: 1, username: 'reader'}));
      } else {
        const base = request.headers['x-forwarded-prefix'];
        response.writeHead(200, {'content-type': 'text/html'}).end(`<!doctype html><body data-lific-route="${request.url}" data-lific-base-path="${base}" data-lific-require-session="false"><script defer src="${base}/__topcoat-shell.js"></script><script defer src="${base}/__topcoat-session.js"></script></body>`);
      }
    });
    await listen(upstream);
    const proxy = http.createServer((request, response) => {
      if (!request.url.startsWith('/app/')) {response.writeHead(404).end();return;}
      const forwarded = http.request({hostname: '127.0.0.1', port: upstream.address().port,
        path: request.url.slice(4), headers: {...request.headers, 'x-forwarded-prefix': '/app'}}, result => {
        response.writeHead(result.statusCode, result.headers);result.pipe(response);
      });
      request.pipe(forwarded);
    });
    await listen(proxy);
    const origin = `http://127.0.0.1:${proxy.address().port}`;
    const browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
    try {
      const page = await browser.newPage();
      const failures = [];
      page.on('pageerror', error => failures.push(error.message));
      await page.goto(`${origin}/app/LIF/issues/LIF-9?comment=3#comment-3`);
      await page.waitForFunction(() => window.lificSession);
      assert.equal(await page.evaluate(() => lificSession.resolve('/auth/me').url), '/app/api/auth/me');
      await page.evaluate(async () => {lificSession.saveSession('fixture-token');await lificSession.request('/auth/me');});
      assert.ok(upstreamPaths.includes('/api/auth/me'));
      await page.goBack();
      await page.waitForURL(`${origin}/app/LIF/issues`);
      await page.waitForFunction(() => document.body.dataset.lificRoute === '/LIF/issues');
      await page.goForward();
      await page.waitForURL(`${origin}/app/LIF/issues/LIF-9?comment=3#comment-3`);
      await page.goto(`${origin}/app/#/public/LIF/issues/LIF-9?comment=3#comment-3`);
      await page.waitForURL(`${origin}/app/public/LIF/issues/LIF-9?comment=3#comment-3`);
      await page.waitForFunction(() => window.lificSession?.state.publicProject === 'LIF');
      await page.evaluate(() => lificSession.request('/issues/9'));
      assert.ok(upstreamPaths.includes('/public/api/projects/LIF/issues/9'));
      assert.deepEqual(failures, []);
    } finally {
      await browser.close();await close(proxy);await close(upstream);
    }
  });

test('production documents preserve browser install metadata and fragment links under a proxy prefix',
  {skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH, timeout: 45000}, async () => {
    const root = path.resolve(__dirname, '../../../..');
    const {chromium} = await import(path.join(root, 'e2e/node_modules/playwright/index.mjs'));
    const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'lific-prefix-browser-'));
    const reservation = http.createServer();
    await listen(reservation);
    const upstreamPort = reservation.address().port;
    await close(reservation);
    const config = path.join(scratch, 'lific.toml');
    fs.writeFileSync(config, `[server]\nhost = "127.0.0.1"\nport = ${upstreamPort}\ntrusted_proxies = ["127.0.0.1"]\n[database]\npath = "${path.join(scratch, 'lific.db')}"\n[backup]\nenabled = false\n[auth]\nrequired = true\nallow_signup = false\n`, {mode: 0o600});
    const server = spawn(path.join(root, 'target/debug/lific'), ['--config', config, 'start', '--init-if-missing'], {
      cwd: scratch, stdio: ['ignore', 'pipe', 'pipe'],
      env: {...process.env, LIFIC_INIT_ADMIN_NAME: 'Prefix test', LIFIC_INIT_ADMIN_PASSWORD: 'prefix-browser-password-123'},
    });
    const exited = once(server, 'exit');
    let output = '';
    server.stdout.on('data', data => {output += data;});
    server.stderr.on('data', data => {output += data;});
    const proxy = http.createServer((request, response) => {
      if (!request.url.startsWith('/app/')) {response.writeHead(404).end();return;}
      const forwarded = http.request({hostname: '127.0.0.1', port: upstreamPort,
        path: request.url.slice(4), headers: {...request.headers, 'x-forwarded-prefix': '/app'}}, result => {
        response.writeHead(result.statusCode, result.headers);result.pipe(response);
      });
      forwarded.on('error', () => response.writeHead(502).end());
      request.pipe(forwarded);
    });
    let browser;
    try {
      const deadline = Date.now() + 20000;
      let ready = false;
      while (Date.now() < deadline && server.exitCode === null) {
        try {ready = (await fetch(`http://127.0.0.1:${upstreamPort}/api/health`)).ok;} catch {}
        if (ready) break;
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      assert.ok(ready, `backend did not become ready: ${output}`);
      await listen(proxy);
      const origin = `http://127.0.0.1:${proxy.address().port}`;
      browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
      const page = await browser.newPage();
      await page.goto(`${origin}/app/login`);
      const viewport = await page.locator('meta[name="viewport"]').getAttribute('content');
      assert.match(viewport, /viewport-fit=cover/);
      assert.equal(await page.locator('meta[name="apple-mobile-web-app-title"]').getAttribute('content'), 'Lific');
      assert.equal(await page.locator('meta[name="theme-color"][media="(prefers-color-scheme: light)"]').getAttribute('content'), '#fafcfb');
      assert.equal(await page.locator('meta[name="theme-color"][media="(prefers-color-scheme: dark)"]').getAttribute('content'), '#1c221f');
      assert.equal(await page.locator('link[rel="icon"]').getAttribute('type'), 'image/png');
      assert.equal(await page.locator('link[rel="icon"]').getAttribute('href'), '/app/favicon.png');
      assert.equal(await page.locator('link[rel="apple-touch-icon"]').getAttribute('href'), '/app/apple-touch-icon.png');
      const manifestUrl = await page.locator('link[rel="manifest"]').evaluate(link => link.href);
      assert.equal(manifestUrl, `${origin}/app/manifest.webmanifest`);
      const manifestResponse = await fetch(manifestUrl);
      assert.equal(manifestResponse.status, 200);
      assert.match(manifestResponse.headers.get('content-type'), /^application\/manifest\+json(?:;|$)/);
      const manifest = await manifestResponse.json();
      assert.deepEqual(manifest, {
        name: 'Lific', short_name: 'Lific', description: 'Local-first, lightweight issue tracker',
        id: './', start_url: './', scope: './', display: 'standalone', background_color: '#eef4f1', theme_color: '#fafcfb',
        icons: [
          {src: 'icon-192.png', sizes: '192x192', type: 'image/png', purpose: 'any'},
          {src: 'icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'any'},
          {src: 'icon-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable'},
        ],
      });
      for (const key of ['id', 'start_url', 'scope']) assert.equal(new URL(manifest[key], manifestUrl).href, `${origin}/app/`);
      const iconDigests = {
        'favicon.png': '70b4eeb23be6136fbf88f605a816df0733eb2d1a88006ea6ee7e24bbf834afbb',
        'apple-touch-icon.png': '552da59dc28e6fe0e12ce09737575af3f7325351062ac0ea815203e9b25f71f0',
        'icon-192.png': '53dfea29bca68b804b2a2dc1a2d2c57e04f21730b2c554c94a83f06ef8a5682b',
        'icon-512.png': 'ee5e3941e25d0db21a9c254b398d62757ae2d91259d6ea6f7b68ac785e95d34b',
        'icon-maskable-512.png': '576460e55540229b965c63beb17fa86eac0c5b3e8380923cdf97d2454288c93e',
      };
      for (const [filename, digest] of Object.entries(iconDigests)) {
        const url = new URL(filename, manifestUrl);
        assert.equal(url.pathname, `/app/${filename}`);
        const response = await fetch(url);
        assert.equal(response.status, 200, filename);
        assert.match(response.headers.get('content-type'), /^image\/png(?:;|$)/, filename);
        assert.equal(createHash('sha256').update(Buffer.from(await response.arrayBuffer())).digest('hex'), digest, filename);
      }
      const skip = page.locator('a[href="#main-content"]');
      assert.equal(await skip.evaluate(link => new URL(link.href).pathname), '/app/login');
      await skip.evaluate(link => link.click());
      await page.waitForURL(`${origin}/app/login#main-content`);
      assert.equal((await fetch(`${origin}/app/api/health`)).status, 200);
    } finally {
      if (browser) await browser.close();
      if (proxy.listening) await close(proxy);
      if (server.exitCode === null) server.kill('SIGTERM');
      await exited;
      fs.rmSync(scratch, {recursive: true, force: true});
    }
  });
