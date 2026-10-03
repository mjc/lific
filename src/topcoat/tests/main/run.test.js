const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {collectTests} = require('./run.js');

test('main test runner includes every group and separates browser tests', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'lific-test-discovery-'));
  try {
    const names = ['routing.browser.test.js', 'shell/a.test.js', 'issues/b.browser.test.mjs',
      'resources/c.test.mjs', 'resources/adapter.js'];
    for (const name of names) {
      const file = path.join(root, name);
      fs.mkdirSync(path.dirname(file), {recursive: true});
      fs.writeFileSync(file, '');
    }
    const relative = mode => collectTests(root, mode).map(file => path.relative(root, file));
    assert.deepEqual(relative('unit'), ['resources/c.test.mjs', 'shell/a.test.js']);
    assert.deepEqual(relative('browser'), ['issues/b.browser.test.mjs', 'routing.browser.test.js']);
    assert.equal(relative('all').length, 4);
    assert.throws(() => collectTests(root, 'typo'), /unit, browser or all/);
  } finally {
    fs.rmSync(root, {recursive: true, force: true});
  }
});
