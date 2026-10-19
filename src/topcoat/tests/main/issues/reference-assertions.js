const nodeTest = require('node:test');
const {expect: matcher, assert} = require('./harness.js');

function expect(actual) {
  const result = matcher(actual);
  result.toHaveBeenCalledTimes = count => assert.equal(actual.mock.calls.length, count);
  return result;
}

function test(name, run) {
  return nodeTest.test(name, {timeout: 2000}, async () => {
    let timer;
    try {
      await Promise.race([run(), new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`Reference behavior did not deliver: ${name}`)), 1000);
      })]);
    } finally {
      clearTimeout(timer);
    }
  });
}

function spyOn(object, key) {
  const original = object[key];
  const spy = (...args) => {spy.mock.calls.push(args); return spy.implementation?.(...args);};
  spy.mock = {calls: []};
  spy.mockImplementation = implementation => {spy.implementation = implementation; return spy;};
  spy.mockRestore = () => {object[key] = original;};
  object[key] = spy;
  return spy;
}

module.exports = {...nodeTest, test, expect, spyOn};
