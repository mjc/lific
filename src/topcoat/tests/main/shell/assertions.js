// Node assertion vocabulary for preserving the original Bun test assertions.
const assert = require('node:assert/strict');
const node = require('node:test');
function equal(actual, expected) {
  if(expected && expected.__match) return assert.match(actual,expected.__match);
  if(expected && typeof expected==='object' && !Array.isArray(expected)) {
    assert.deepEqual(Object.keys(actual).sort(),Object.keys(expected).sort());
    for(const key of Object.keys(expected)) equal(actual[key],expected[key]);
  } else assert.deepEqual(JSON.parse(JSON.stringify(actual)),JSON.parse(JSON.stringify(expected)));
}
function expect(actual) {
  const checks={toBe:x=>assert.equal(actual,x),toEqual:x=>equal(actual,x),toBeNull:()=>assert.equal(actual,null),toBeUndefined:()=>assert.equal(actual,undefined),toBeTruthy:()=>assert.ok(actual),toBeFalsy:()=>assert.ok(!actual),toHaveLength:x=>assert.equal(actual.length,x),toContain:x=>assert.ok(actual.includes(x)),toBeGreaterThan:x=>assert.ok(actual>x,`${actual} > ${x}`),toBeLessThan:x=>assert.ok(actual<x,`${actual} < ${x}`),toBeGreaterThanOrEqual:x=>assert.ok(actual>=x),toBeLessThanOrEqual:x=>assert.ok(actual<=x),toBeCloseTo:(x,n=2)=>assert.ok(Math.abs(actual-x)<10**-n/2,`${actual} ~= ${x}`),toThrow:()=>assert.throws(actual)};
  checks.not={toThrow:()=>assert.doesNotThrow(actual),toBe:x=>assert.notEqual(actual,x)};
  checks.resolves={toBeTruthy:async()=>assert.ok(await actual)};
  return checks;
}
expect.stringMatching=regex=>({__match:regex});
const test=(name,fn)=>node.test(name,fn);
test.each=values=>(name,fn)=>values.forEach(value=>test(name.replace('%s',String(value)),()=>fn(value)));
module.exports={test,describe:node.describe,beforeEach:node.beforeEach,afterEach:node.afterEach,expect};
