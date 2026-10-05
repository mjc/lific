const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const {readDevToolsPort} = require('./browser_fixture.cjs');

test('Windows DevTools discovery retries transient busy then reads actual file bytes', async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'lific-devtools-'));
  const file = path.join(directory, 'DevToolsActivePort');
  const read = fs.readFile;
  let calls = 0;
  try {
    await fs.writeFile(file, '34567\n/devtools/browser/actual\n');
    fs.readFile = async (...args) => {
      if (args[0] === file && ++calls === 1) {
        throw Object.assign(new Error('actual Windows busy classification'), {code: 'EBUSY'});
      }
      return read(...args);
    };
    assert.equal(await readDevToolsPort(file, 'win32'), '34567');
    assert.equal(calls, 2);
  } finally {
    fs.readFile = read;
    await fs.rm(directory, {recursive: true, force: true});
  }
});

for (const [platform, code] of [['win32', 'EACCES'], ['linux', 'EBUSY']]) {
  test(`DevTools discovery propagates ${code} on ${platform}`, async () => {
    const read = fs.readFile;
    const error = Object.assign(new Error('not a readiness failure'), {code});
    fs.readFile = async () => { throw error; };
    try {
      await assert.rejects(readDevToolsPort('unused', platform), actual => actual === error);
    } finally {
      fs.readFile = read;
    }
  });
}
