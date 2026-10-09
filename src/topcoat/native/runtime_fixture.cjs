'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const runtimeSource = fs.readFileSync(path.join(__dirname, '../assets/runtime.js'), 'utf8');
const startupPattern = /var (\w+)=new (\w+);(\w+)\.start\(document\);\3\.page\.listenForDevRefresh\(\);/g;
const startupMatches = [...runtimeSource.matchAll(startupPattern)];
assert.equal(startupMatches.length, 1, 'the packaged runtime has one recognizable startup');
const runtimeBootstrap = startupMatches[0][0];
const runtimeInstance = startupMatches[0][1];

// Semantic test names keep minifier-specific identifiers in one place.
const runtimeTypes = Object.freeze({
  Runtime: 've',
  mounted: 'topcoatMountedEndpoint',
  logical: 'topcoatLogicalEndpoint',
  mountedEndpoint: 'topcoatMountedEndpoint',
  logicalEndpoint: 'topcoatLogicalEndpoint',
  Connection: 'me',
  Unit: '_',
  RenderUnit: '_',
  Scope: 'E',
  Context: 'fe',
  Registry: 'ye',
  Procedure: 'ce',
  Navigation: 'Q',
  Page: 'ge',
  RenderFrame: 'Y',
  String: 'H',
  Owned: 'S',
  Vec: 'W',
  Integer: 'w',
  Signal: 'pe',
  Event: 'le',
  NativeEvent: 'le',
  Effect: 'ne',
  flush: 'St',
  parseComment: 'Ge',
  signalJSON: 'JSON.parse',
  beforeNavigationCommit: 'topcoatBeforeNavigationCommit',
});

function fixtureRuntime(exposedTypes, globalName = 'fixture', extraProperties = {}) {
  const properties = exposedTypes.map(name => {
    const identifier = runtimeTypes[name];
    assert.ok(identifier, `unknown packaged runtime test type: ${name}`);
    return `${name}:${identifier}`;
  });
  for (const [name, expression] of Object.entries(extraProperties)) {
    properties.push(`${name}:${expression}`);
  }
  const replacement = `globalThis.${globalName}={${properties.join(',')}};`;
  return runtimeSource.replace(runtimeBootstrap, replacement);
}

function startedRuntimeWith(afterStartup) {
  const statement = typeof afterStartup === 'function' ? afterStartup(runtimeInstance) : afterStartup;
  return runtimeSource.replace(runtimeBootstrap, `${runtimeBootstrap}${statement}`);
}

module.exports = {
  runtimeSource,
  runtimeBootstrap,
  runtimeInstance,
  runtimeTypes,
  fixtureRuntime,
  startedRuntimeWith,
};
