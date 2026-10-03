const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const {spawn, execFileSync} = require('node:child_process');
const {once} = require('node:events');
const {pathToFileURL} = require('node:url');

const root = path.resolve(__dirname, '../../..');
const listen = server => new Promise((resolve, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', () => {
    server.removeListener('error', reject);
    resolve(server.address().port);
  });
});

async function startFixture({binary = process.env.LIFIC_BIN || path.join(root, 'target/debug/lific'), seed = true} = {}) {
  binary = path.resolve(binary);
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'lific-acceptance-'));
  const prefix = '/app';
  const credentials = {identity: 'acceptance-admin', password: 'acceptance-password-123'};
  const sockets = new Set();
  const requests = [];
  let server, exited, browser, token, startupError;
  let output = '';
  let closed = false;
  const reservation = http.createServer();
  const upstreamPort = await listen(reservation);
  await new Promise(resolve => reservation.close(resolve));
  const target = request => ({
    hostname: '127.0.0.1', port: upstreamPort,
    method: request.method, path: request.url.slice(prefix.length),
    headers: {...request.headers, 'x-forwarded-prefix': prefix},
  });
  const proxy = http.createServer((request, response) => {
    if (!request.url.startsWith(`${prefix}/`)) {
      response.writeHead(404).end();
      return;
    }
    requests.push({path: request.url, method: request.method, headers: {...request.headers}});
    const forwarded = http.request(target(request), result => {
      response.writeHead(result.statusCode, result.headers);
      result.pipe(response);
    });
    forwarded.on('error', error => {
      if (!response.headersSent) response.writeHead(502);
      response.end(error.message);
    });
    request.pipe(forwarded);
  });
  const track = socket => {
    sockets.add(socket);
    socket.once('close', () => sockets.delete(socket));
  };
  proxy.on('connection', track);
  proxy.on('upgrade', (request, socket, head) => {
    if (!request.url.startsWith(`${prefix}/`)) {
      socket.end('HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n');
      return;
    }
    requests.push({path: request.url, method: request.method, headers: {...request.headers}});
    const forwarded = http.request(target(request));
    forwarded.on('upgrade', (response, upstream, upstreamHead) => {
      track(upstream);
      const headers = response.rawHeaders.reduce((lines, value, index, values) =>
        index % 2 === 0 ? [...lines, `${value}: ${values[index + 1]}`] : lines, []);
      socket.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\n${headers.join('\r\n')}\r\n\r\n`);
      if (upstreamHead.length) socket.write(upstreamHead);
      if (head.length) upstream.write(head);
      socket.on('error', () => upstream.destroy());
      upstream.on('error', () => socket.destroy());
      socket.pipe(upstream).pipe(socket);
    });
    forwarded.on('response', response => {
      socket.end(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\nConnection: close\r\n\r\n`);
      response.resume();
    });
    forwarded.on('error', () => socket.destroy());
    forwarded.end();
  });

  const close = async () => {
    if (closed) return;
    closed = true;
    if (browser) await browser.close();
    for (const socket of sockets) socket.destroy();
    if (proxy.listening) await new Promise(resolve => proxy.close(resolve));
    if (server?.pid && server.exitCode === null && server.signalCode === null) {
      server.kill('SIGTERM');
      const timer = setTimeout(() => server.kill('SIGKILL'), 5000);
      try { await exited; } finally { clearTimeout(timer); }
    }
    fs.rmSync(scratch, {recursive: true, force: true});
  };

  try {
    const proxyPort = await listen(proxy);
    const origin = `http://127.0.0.1:${proxyPort}`;
    const url = route => `${origin}${prefix}${route}`;
    const config = path.join(scratch, 'lific.toml');
    const database = path.join(scratch, 'lific.db');
    fs.writeFileSync(config, `[server]\nhost = "127.0.0.1"\nport = ${upstreamPort}\npublic_url = "${origin}${prefix}"\ntrusted_proxies = ["127.0.0.1"]\n[database]\npath = "${database}"\n[backup]\nenabled = false\n[auth]\nrequired = true\nallow_signup = false\n`, {mode: 0o600});
    const env = {...process.env, LIFIC_INIT_ADMIN_NAME: 'Acceptance Admin', LIFIC_INIT_ADMIN_PASSWORD: credentials.password};
    delete env.LIFIC_API_KEY;
    server = spawn(path.resolve(binary), ['--config', config, 'start', '--init-if-missing'],
      {cwd: scratch, env, stdio: ['ignore', 'pipe', 'pipe']});
    exited = once(server, 'exit').catch(error => { startupError = error; });
    const capture = chunk => { output = (output + chunk.toString()).slice(-16384); };
    server.stdout.on('data', capture);
    server.stderr.on('data', capture);
    const deadline = Date.now() + 30000;
    let ready = false;
    while (Date.now() < deadline && !startupError && server.exitCode === null && server.signalCode === null) {
      try { ready = (await fetch(url('/api/health'), {signal: AbortSignal.timeout(1000)})).ok; } catch {}
      if (ready) break;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert.ok(ready, `acceptance server did not become healthy: ${startupError ?? output}`);
    const api = (route, {method = 'GET', body, token: bearer = token, headers = {}} = {}) => {
      const requestHeaders = new Headers(headers);
      if (bearer) requestHeaders.set('authorization', `Bearer ${bearer}`);
      const form = body instanceof FormData;
      if (body !== undefined && !form && !requestHeaders.has('content-type')) requestHeaders.set('content-type', 'application/json');
      return fetch(url(`/api${route}`), {
        method, headers: requestHeaders,
        body: body === undefined ? undefined : form ? body : JSON.stringify(body),
        signal: AbortSignal.timeout(10000),
      });
    };
    const create = async (route, body) => {
      const response = await api(route, {method: 'POST', body});
      assert.ok(response.ok, `${route}: ${response.status} ${await response.clone().text()}`);
      return response.json();
    };
    const login = await create('/auth/login', credentials);
    token = login.token;
    assert.match(token, /^lific_sess_/);
    let project, issue, page, module, plan;
    if (seed) {
      project = await create('/projects', {name: 'Acceptance project', identifier: 'ACC', description: 'Real executable smoke checks'});
      issue = await create('/issues', {project_id: project.id, title: 'Acceptance issue', description: '# Issue body\nLive smoke content'});
      page = await create('/pages', {project_id: project.id, title: 'Acceptance page', content: '# Page body\nLive page content'});
      module = await create('/modules', {project_id: project.id, name: 'Acceptance module', description: 'Live module content'});
      plan = await create('/plans', {project_id: project.id, title: 'Acceptance plan', steps: [{title: 'Live step', issue_id: issue.id}]});
    }
    const cli = args => execFileSync(binary, ['--config', config, '--db', database, ...args],
      {cwd: scratch, env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 10000});
    const {chromium} = await import(pathToFileURL(path.join(root, 'e2e/node_modules/playwright/index.mjs')).href);
    browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
    const newPage = async ({authenticated = true} = {}) => {
      const context = await browser.newContext();
      if (authenticated) {
        await context.addCookies([{name: 'lific_token', value: token, url: origin, httpOnly: true, sameSite: 'Lax'}]);
        await context.addInitScript(value => localStorage.setItem('lific_token', value), token);
      }
      return context.newPage();
    };
    return {browser, origin, prefix, url, credentials, token, project, issue, page, module, plan,
      api, newPage, requests, scratch, config, database, cli, close};
  } catch (error) {
    await close();
    throw error;
  }
}

module.exports = {startFixture};
