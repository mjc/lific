const {test} = require('node:test');
const assert = require('node:assert/strict');
const {startFixture} = require('../../acceptance/server.js');

// Port of web/tests/viteConfig.test.ts: both API namespaces reach Rust.
// Topcoat serves them directly, so exercise their responses rather than Vite config.
test('private and public API routes reach Rust through the mounted frontend', {timeout: 60000}, async () => {
  const fixture = await startFixture();
  try {
    const published = await fixture.api(`/projects/${fixture.project.id}`, {
      method: 'PUT', body: {is_public: true},
    });
    assert.equal(published.status, 200);
    const page = await fixture.newPage();
    await page.goto(fixture.url('/'));
    const responses = await page.evaluate(async ({prefix, identifier, token}) => {
      const privateResponse = await fetch(`${prefix}/api/projects`, {
        headers: {authorization: `Bearer ${token}`},
      });
      const publicResponse = await fetch(`${prefix}/public/api/projects/${identifier}`);
      return Promise.all([privateResponse, publicResponse].map(async response => ({
        status: response.status, type: response.headers.get('content-type'), body: await response.json(),
      })));
    }, {prefix: fixture.prefix, identifier: fixture.project.identifier, token: fixture.token});
    for (const response of responses) {
      assert.equal(response.status, 200);
      assert.match(response.type, /application\/json/);
    }
    assert.ok(responses[0].body.some(project => project.id === fixture.project.id));
    assert.equal(responses[1].body.identifier, fixture.project.identifier);
    const anonymous = await fixture.newPage({authenticated: false});
    await anonymous.goto(fixture.url(`/public/${fixture.project.identifier}`));
    const statuses = await anonymous.evaluate(async ({prefix, identifier}) => {
      return Promise.all([`${prefix}/api/projects`, `${prefix}/public/api/projects/${identifier}`]
        .map(async route => (await fetch(route)).status));
    }, {prefix: fixture.prefix, identifier: fixture.project.identifier});
    assert.deepEqual(statuses, [401, 200]);
  } finally {
    await fixture.close();
  }
});
