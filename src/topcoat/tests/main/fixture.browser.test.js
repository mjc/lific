const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {startFixture} = require('../../acceptance/server.js');

test('empty real instances support operator CLI setup and remove their scratch data', {timeout: 60000}, async () => {
  const fixture = await startFixture({seed: false});
  try {
    assert.equal(fixture.project, undefined);
    const projects = await fixture.api('/projects');
    assert.equal(projects.status, 200);
    assert.deepEqual(await projects.json(), []);
    assert.ok(fs.existsSync(fixture.config));
    assert.ok(fs.existsSync(fixture.database));
    fixture.cli(['user', 'create', '--username', 'main-test-user', '--email', 'main-test@example.test',
      '--password', 'main-test-password-123', '--json']);
    const login = await fixture.api('/auth/login', {method: 'POST', token: null,
      body: {identity: 'main-test-user', password: 'main-test-password-123'}});
    assert.equal(login.status, 200);
    assert.match((await login.json()).token, /^lific_sess_/);
  } finally {
    await fixture.close();
  }
  assert.equal(fs.existsSync(fixture.scratch), false);
});
