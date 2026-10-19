const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const baseline = require('./baseline.json');
const root = path.resolve(__dirname, '../../../..');

test('every original frontend source and named case has a forward-port mapping', () => {
  const inventories = ['inventory.json', ...['shell', 'issues', 'resources'].map(group => `${group}/inventory.json`)];
  const mapped = new Map();
  for (const name of inventories) {
    const inventory = JSON.parse(fs.readFileSync(path.join(__dirname, name), 'utf8'));
    assert.equal(inventory.baseline, baseline.baseline, `${name}: baseline changed`);
    for (const entry of inventory.files) {
      assert.equal(mapped.has(entry.source), false, `Duplicate mapping: ${entry.source}`);
      mapped.set(entry.source, entry);
      assert.ok(fs.existsSync(path.join(root, entry.target)), `Missing translated target: ${entry.target}`);
    }
  }
  assert.deepEqual([...mapped.keys()].sort(), baseline.sources.map(entry => entry.source).sort());
  for (const original of baseline.sources) {
    const entry = mapped.get(original.source);
    const cases = [...entry.cases];
    for (const title of original.case_titles) {
      // Template expressions are compared in the independent source review.
      if (title.includes('${')) continue;
      const index = cases.indexOf(title);
      assert.ok(index >= 0, `${original.source}: unmapped original case ${title}`);
      cases.splice(index, 1);
    }
  }
});
