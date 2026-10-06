const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {execFileSync} = require('node:child_process');
const {assertOriginalSources} = require('./original_source_fixture.cjs');

const file = 'src/routes/Reference.svelte';
const referenceHead = 'refs/tags/reference-fixture';

function checkout(directory, body, gitDirectory) {
  const args = ['init', '--quiet'];
  if (gitDirectory) args.push(`--separate-git-dir=${gitDirectory}`);
  execFileSync('git', [...args, directory]);
  const snapshot = path.join(directory, 'web');
  fs.mkdirSync(path.dirname(path.join(snapshot, file)), {recursive: true});
  fs.writeFileSync(path.join(snapshot, file), body);
  execFileSync('git', ['-C', directory, '-c', 'core.autocrlf=false', 'add', '--', 'web']);
  const tree = execFileSync('git', ['-C', directory, 'write-tree'], {encoding: 'utf8'}).trim();
  execFileSync('git', ['-C', directory, 'update-ref', referenceHead, tree]);
  return snapshot;
}

function temporaryDirectory(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'lific-original-source-'));
  t.after(() => fs.rmSync(directory, {recursive: true, force: true}));
  return directory;
}

function checkFrom(directory, snapshot) {
  execFileSync(process.execPath, ['-e', `
    const {assertOriginalSources} = require(process.argv[1]);
    assertOriginalSources(process.argv[2], [process.argv[3]], process.argv[4]);
  `, require.resolve('./original_source_fixture.cjs'), snapshot, file, referenceHead],
  {cwd: directory, stdio: 'pipe'});
}

test('original source checks read the reference checkout from a different application repository', t => {
  const directory = temporaryDirectory(t);
  const application = path.join(directory, 'application');
  checkout(application, '<h1>Application repository</h1>\n');
  const snapshot = checkout(path.join(application, '.devenv/original-reference'), '<h1>Original source</h1>\n');
  assert.doesNotThrow(() => checkFrom(application, snapshot));
});

test('original source checks support checkouts whose .git is a file', t => {
  const directory = temporaryDirectory(t);
  const application = path.join(directory, 'application');
  checkout(application, '<h1>Application repository</h1>\n');
  const snapshot = checkout(path.join(directory, 'original-reference'), '<h1>Original source</h1>\n',
    path.join(directory, 'reference-git'));
  assert.ok(fs.statSync(path.join(path.dirname(snapshot), '.git')).isFile());
  assert.doesNotThrow(() => checkFrom(application, snapshot));
});

test('original source checks reject working source changed from the pinned Git tree', t => {
  const directory = temporaryDirectory(t);
  const snapshot = checkout(path.join(directory, 'original-reference'), '<h1>Original source</h1>\n');
  fs.writeFileSync(path.join(snapshot, file), '<h1>Changed source</h1>\n');
  assert.throws(() => assertOriginalSources(snapshot, [file], referenceHead),
    error => error.code === 'ERR_ASSERTION' && error.message.includes(`Unmodified reference ${file}`));
});

test('original source checks reject a snapshot without its own checkout instead of using the enclosing repository', t => {
  const directory = temporaryDirectory(t);
  const application = path.join(directory, 'application');
  checkout(application, '<h1>Original source</h1>\n');
  const snapshot = path.join(application, '.devenv/original-reference/web');
  fs.mkdirSync(path.dirname(path.join(snapshot, file)), {recursive: true});
  fs.writeFileSync(path.join(snapshot, file), '<h1>Original source</h1>\n');
  assert.throws(() => assertOriginalSources(snapshot, [file], referenceHead),
    error => error.code === 'ERR_ASSERTION' && error.message.includes('Pinned reference checkout required'));
});
