// Compare reference source bytes against its own immutable Git baseline.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {execFileSync} = require('node:child_process');

const originalHead = '9683d38af8e1e6f9b076439fe90d9519109b2218';

function assertOriginalSources(snapshot, files, referenceHead = originalHead) {
  const root = path.dirname(fs.realpathSync(snapshot));
  assert.ok(fs.existsSync(path.join(root, '.git')), `Pinned reference checkout required: ${root}`);
  for (const file of files) {
    assert.deepEqual(fs.readFileSync(path.join(snapshot, file)),
      execFileSync('git', ['-C', root, 'show', `${referenceHead}:web/${file}`]),
      `Unmodified reference ${file}`);
  }
}

module.exports = {originalHead, assertOriginalSources};
