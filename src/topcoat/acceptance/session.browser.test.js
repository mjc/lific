const {test} = require('node:test');
const assert = require('node:assert/strict');
const {startFixture} = require('./server.js');

test('production auth, websocket recovery and MCP remain reachable through the mounted router',
  {skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH, timeout: 120000}, async t => {
    const fixture = await startFixture();
    t.after(() => fixture.close());

    await t.test('real login, session replacement and logout preserve the authentication boundary', async () => {
      const page = await fixture.newPage({authenticated: false});
      const accountResponse = await fixture.api('/auth/me');
      assert.equal(accountResponse.status, 200);
      const account = await accountResponse.json();
      await page.goto(fixture.url(`/${fixture.project.identifier}/issues/${fixture.issue.identifier}`));
      await page.waitForURL(fixture.url('/login'));
      await page.locator('[data-login] [name="identity"]').fill(fixture.credentials.identity);
      await page.locator('[data-login] [name="password"]').fill('incorrect-acceptance-password');
      await page.locator('[data-login] [type="submit"]').click();
      await page.locator('[data-form-error]:visible').waitFor();
      assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')), null);
      await page.locator('[data-login] [name="password"]').fill(fixture.credentials.password);
      await page.locator('[data-login] [type="submit"]').click();
      await page.waitForURL(fixture.url('/'));
      await page.waitForFunction(id => window.lificSession?.state.user?.id === id, account.id);
      const previous = await page.evaluate(() => localStorage.getItem('lific_token'));
      assert.match(previous, /^lific_sess_/);
      const replaced = await page.evaluate(async password => {
        const result = await lificSession.request('/auth/me/refresh', {
          method: 'POST', body: JSON.stringify({password}),
        });
        if (result.ok) {
          lificSession.saveSession(result.data.token);
          await lificSession.bootstrap();
        }
        return result;
      }, fixture.credentials.password);
      assert.equal(replaced.ok, true, JSON.stringify(replaced));
      assert.notEqual(replaced.data.token, previous);
      assert.equal((await fixture.api('/auth/me', {token: previous})).status, 401);
      assert.equal((await fixture.api('/auth/me', {token: replaced.data.token})).status, 200);
      const stalePage = await fixture.newPage({authenticated: false});
      await stalePage.goto(fixture.url('/login'));
      await stalePage.evaluate(token => localStorage.setItem('lific_token', token), previous);
      await stalePage.goto(fixture.url(`/${fixture.project.identifier}/issues/${fixture.issue.identifier}`));
      await stalePage.waitForURL(fixture.url('/login'));
      assert.equal(await stalePage.evaluate(() => localStorage.getItem('lific_token')), null,
        'restoring a revoked session must clear its credential and return to login');
      await stalePage.close();
      const cookie = (await page.context().cookies()).find(cookie => cookie.name === 'lific_token');
      assert.equal(cookie.value, replaced.data.token);
      assert.equal(cookie.httpOnly, true);
      await page.reload();
      await page.waitForFunction(id => window.lificSession?.state.user?.id === id, account.id);
      assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')), replaced.data.token);
      await page.evaluate(() => lificSession.logout());
      await page.waitForURL(fixture.url('/login'));
      assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')), null);
      assert.equal((await page.context().cookies()).some(cookie => cookie.name === 'lific_token'), false);
      assert.equal((await fixture.api('/auth/me', {token: replaced.data.token})).status, 401);
      assert.equal((await fixture.api('/auth/me')).status, 200, 'another live session must survive replacement and logout');
      await page.close();
    });

    await t.test('a real websocket reconnect resumes its cursor and catches an offline committed issue update', async () => {
      const page = await fixture.newPage();
      // Observe native sockets without replacing their network or protocol behavior.
      await page.addInitScript(() => {
        window.acceptanceSockets = [];
        window.acceptanceFrames = [];
        window.acceptanceEvents = [];
        const NativeWebSocket = window.WebSocket;
        window.WebSocket = class extends NativeWebSocket {
          constructor(...args) {
            super(...args);
            window.acceptanceSockets.push(this);
            this.addEventListener('message', event => window.acceptanceEvents.push(JSON.parse(event.data)));
          }
          send(frame) {
            window.acceptanceFrames.push(JSON.parse(frame));
            super.send(frame);
          }
        };
      });
      await page.goto(fixture.url(`/${fixture.project.identifier}/issues`));
      await page.waitForFunction(projectId => window.lificSync?.state.connected &&
        window.lificSync.peekProject(projectId)?.status === 'ready', fixture.project.id);
      await page.waitForFunction(() => acceptanceEvents.some(event => event.type === 'activity.baseline'));
      const before = await page.evaluate(projectId => ({
        cursor: lificSync.peekProject(projectId).cursor,
        frames: acceptanceFrames.length,
        sockets: acceptanceSockets.length,
        url: acceptanceSockets.at(-1).url,
      }), fixture.project.id);
      assert.equal(before.url, fixture.url('/api/events/ws').replace(/^http/, 'ws'));
      const handshake = fixture.requests.find(request => request.path === `${fixture.prefix}/api/events/ws`);
      assert.equal(handshake.headers.origin, fixture.origin);
      assert.match(handshake.headers.cookie, /(?:^|;\s*)lific_token=lific_sess_/);
      assert.equal(handshake.headers.authorization, undefined);
      await page.context().setOffline(true);
      await page.evaluate(() => new Promise(resolve => {
        const socket = acceptanceSockets.at(-1);
        if (socket.readyState === WebSocket.CLOSED) return resolve();
        socket.addEventListener('close', resolve, {once: true});
        socket.close(4000, 'acceptance reconnect');
      }));
      await page.waitForFunction(() => !lificSync.state.connected);
      const title = 'Committed while the production browser was offline';
      const response = await fixture.api(`/issues/${fixture.issue.id}`, {
        method: 'PUT', body: {title, expected_seq: fixture.issue.seq},
      });
      assert.equal(response.status, 200, await response.clone().text());
      const updated = await response.json();
      assert.ok(updated.seq > before.cursor);
      await page.context().setOffline(false);
      await page.waitForFunction(({projectId, issueId, title, seq}) => {
        const model = lificSync.peekProject(projectId);
        return lificSync.state.connected && model?.cursor >= seq && model.issues.some(issue => issue.id === issueId && issue.title === title);
      }, {projectId: fixture.project.id, issueId: fixture.issue.id, title, seq: updated.seq});
      await page.getByRole('link', {name: `${fixture.issue.identifier} ${title}`, exact: true}).waitFor();
      const recovered = await page.evaluate(() => ({
        sockets: acceptanceSockets.length, frames: acceptanceFrames, events: acceptanceEvents,
      }));
      assert.ok(recovered.sockets > before.sockets);
      assert.ok(recovered.frames.slice(before.frames).some(frame => frame.type === 'resume' &&
        frame.project_id === fixture.project.id && frame.cursor >= before.cursor && frame.cursor <= updated.seq));
      // Online focus may backfill REST before reconnect. Independently prove that
      // the production websocket can replay the exact cursor held while offline.
      const replay = await page.evaluate(({projectId, issueId, cursor, seq}) => new Promise((resolve, reject) => {
        const socket = new WebSocket(LificSync.websocketUrl(window));
        const timer = setTimeout(() => { socket.close(); reject(new Error('Production cursor replay timed out.')); }, 10000);
        socket.addEventListener('open', () => socket.send(JSON.stringify({type: 'resume', project_id: projectId, cursor})));
        socket.addEventListener('message', message => {
          const event = JSON.parse(message.data);
          if (event.type !== 'issue.updated' || event.issue_id !== issueId || event.seq !== seq) return;
          clearTimeout(timer); socket.close(); resolve(event);
        });
        socket.addEventListener('error', () => { clearTimeout(timer); reject(new Error('Production cursor replay handshake failed.')); });
      }), {projectId: fixture.project.id, issueId: fixture.issue.id, cursor: before.cursor, seq: updated.seq});
      assert.equal(replay.project_id, fixture.project.id);
      assert.equal(replay.seq, updated.seq);
      await page.close();
    });

    await t.test('actual MCP initialize uses the production service and rejects missing credentials', async () => {
      const initialize = {jsonrpc: '2.0', id: 1, method: 'initialize', params: {
        protocolVersion: '2025-03-26', capabilities: {}, clientInfo: {name: 'topcoat-acceptance', version: '1'},
      }};
      const options = {method: 'POST', headers: {
        'content-type': 'application/json', accept: 'application/json, text/event-stream',
      }, body: JSON.stringify(initialize)};
      const anonymous = await fetch(fixture.url('/mcp'), options);
      assert.equal(anonymous.status, 401);
      const response = await fetch(fixture.url('/mcp'), {
        ...options, headers: {...options.headers, authorization: `Bearer ${fixture.token}`},
      });
      assert.equal(response.status, 200, await response.clone().text());
      assert.match(response.headers.get('content-type'), /^application\/json(?:;|$)/);
      const message = await response.json();
      assert.equal(message.jsonrpc, '2.0');
      assert.equal(message.id, initialize.id);
      assert.equal(typeof message.result.serverInfo.name, 'string');
      assert.equal(message.result.protocolVersion, initialize.params.protocolVersion);
      assert.ok(message.result.capabilities.tools);
    });
  });
