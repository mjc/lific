'use strict';

const assert = require('node:assert/strict');
const vm = require('node:vm');

function emittedShard(marker, context, cx, plain) {
  const jsonPrefix = '::topcoat::shard::start-json(';
  const legacyPrefix = '::topcoat::shard::start(';
  assert.ok(
    marker.startsWith(jsonPrefix) || marker.startsWith(legacyPrefix),
    'the argument source is an emitted Topcoat shard marker',
  );

  let path;
  let identity;
  let expressions;
  if (marker.startsWith(jsonPrefix)) {
    const payload = marker.slice(jsonPrefix.length, marker.lastIndexOf(')'));
    [path, identity, expressions] = JSON.parse(payload);
    assert.equal(typeof path, 'string', 'the JSON shard payload has a path');
    assert.equal(typeof identity, 'string', 'the JSON shard payload has an identity');
    assert.ok(Array.isArray(expressions), 'the JSON shard payload has expression sources');
    assert.ok(expressions.every(expression => typeof expression === 'string'));
  } else {
    const payload = marker.slice(legacyPrefix.length, marker.lastIndexOf(')'));
    const match = /^("(?:\\.|[^"\\])*"), ("(?:\\.|[^"\\])*"), \[([\s\S]*)\]$/.exec(payload);
    assert.ok(match, 'the legacy shard marker contains path, identity, and expressions');
    path = JSON.parse(match[1]);
    identity = JSON.parse(match[2]);
    const expressionList = match[3];
    expressions = [];
    const quoted = /"((?:\\.|[^"\\])*)"/g;
    for (const expression of expressionList.matchAll(quoted)) {
      expressions.push(expression[1]
        .replaceAll('&quot;', '\"')
        .replaceAll('&lt;', '<')
        .replaceAll('&gt;', '>')
        .replaceAll('&amp;', '&'));
    }
  }
  return {
    path,
    identity,
    args: expressions.map(source => plain(vm.runInNewContext(`cx => (${source})`, context)(cx))),
  };
}

module.exports = {emittedShard};
