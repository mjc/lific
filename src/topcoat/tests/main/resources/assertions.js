const assert = require('node:assert/strict');
const {test, describe, beforeEach, afterEach} = require('node:test');
function expect(actual) {
  const matchers = {
    toBe: expected => assert.strictEqual(actual, expected),
    toEqual: expected => assert.deepStrictEqual(structuredClone(actual), structuredClone(expected)),
    toBeNull: () => assert.strictEqual(actual, null),
    toBeUndefined: () => assert.strictEqual(actual, undefined),
    toHaveLength: expected => assert.strictEqual(actual.length, expected),
    toBeLessThan: expected => assert.ok(actual < expected, `${actual} < ${expected}`),
    toBeLessThanOrEqual: expected => assert.ok(actual <= expected, `${actual} <= ${expected}`),
    toBeGreaterThan: expected => assert.ok(actual > expected, `${actual} > ${expected}`),
    toBeGreaterThanOrEqual: expected => assert.ok(actual >= expected, `${actual} >= ${expected}`),
    toMatchObject: expected => {
      for (const [key,value] of Object.entries(expected)) assert.deepStrictEqual(actual[key],value);
    },
  };
  const inverse = {
    toBeNull: () => assert.notStrictEqual(actual, null),
    toBe: expected => assert.notStrictEqual(actual, expected),
  };
  return {...matchers, not: inverse, rejects: {toThrow: message => assert.rejects(actual, error => {
    assert.ok(String(error.message).includes(message), `${error.message} must include ${message}`);
    return true;
  })}};
}
module.exports = {test, describe, beforeEach, afterEach, expect};
