const fs = require('node:fs');
const path = require('node:path');
const {spawnSync} = require('node:child_process');

function collectTests(root, mode) {
  if (!['unit', 'browser', 'all'].includes(mode)) throw new Error('Choose unit, browser or all');
  const files = [];
  const visit = directory => {
    for (const entry of fs.readdirSync(directory, {withFileTypes: true})) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(file);
      else if (entry.isFile() && /\.test\.(?:js|mjs)$/.test(entry.name)) {
        const browser = /\.browser\.test\./.test(entry.name);
        if (mode === 'all' || browser === (mode === 'browser')) files.push(file);
      }
    }
  };
  visit(root);
  return files.sort();
}

if (require.main === module) {
  const [mode = 'unit', results] = process.argv.slice(2);
  const files = collectTests(__dirname, mode);
  if (!files.length) throw new Error(`No ${mode} tests found`);
  const args = ['--test', '--test-concurrency=1', '--test-reporter=spec'];
  if (results) args.push(`--test-reporter=${path.join(__dirname, 'reporter.js')}`,
    '--test-reporter-destination=stdout', `--test-reporter-destination=${path.resolve(results)}`);
  const result = spawnSync(process.execPath, [...args, ...files], {stdio: 'inherit'});
  if (result.error) throw result.error;
  process.exitCode = result.status ?? 1;
}

module.exports = {collectTests};
