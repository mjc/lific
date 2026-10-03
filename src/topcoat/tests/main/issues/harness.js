const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const root = path.resolve(__dirname, '../../../../..');
const {expect} = require(path.join(root, 'e2e/node_modules/playwright/test'));
function product(file, namespace, transform = value => value, globals = {}) {
  const context = {URL, URLSearchParams, AbortController, Headers, FormData, Blob, TextDecoder,
    console, setTimeout, clearTimeout, setInterval, clearInterval, ...globals};
  context.globalThis = context;
  vm.runInNewContext(transform(fs.readFileSync(path.join(root, 'src/topcoat', file), 'utf8')), context, {filename:file});
  return context[namespace] || context.module?.exports;
}
function expose(source, marker, addition) {
  assert.equal(source.split(marker).length, 2, `Production instrumentation marker changed: ${marker}`);
  return source.replace(marker, addition + marker);
}
function bounded(promise, label, timeout = 150) {
  let timer;
  return Promise.race([promise, new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${label} did not settle while the next save was still in flight`)), timeout);
  })]).finally(() => clearTimeout(timer));
}
module.exports = {...require('node:test'), assert, expect, root, product, expose, bounded};
