'use strict';

const assert = require('node:assert/strict');
const vm = require('node:vm');

function emittedShard(marker, context, cx, plain) {
  const prefix = '::topcoat::shard::start(';
  assert.ok(marker.startsWith(prefix), 'the argument source is an emitted Topcoat shard marker');
  const pathEnd = marker.indexOf('\", \"', prefix.length);
  assert.notEqual(pathEnd, -1, 'the shard marker contains its path and identity');
  const path = marker.slice(prefix.length + 1, pathEnd);
  const identityStart = pathEnd + 4;
  const identityEnd = marker.indexOf('\", [', identityStart);
  assert.notEqual(identityEnd, -1, 'the shard marker contains emitted argument expressions');
  const identity = marker.slice(identityStart, identityEnd);
  const expressionList = marker.slice(identityEnd + 4, marker.lastIndexOf('])'));
  const expressions = [];
  let offset = 0;
  while (offset < expressionList.length) {
    while (expressionList[offset] === ' ' || expressionList[offset] === ',') offset += 1;
    if (offset >= expressionList.length) break;
    assert.equal(expressionList[offset], '\"', 'shard argument expressions are quoted');
    offset += 1;
    const end = expressionList.indexOf('\"', offset);
    assert.notEqual(end, -1, 'shard argument expression closes');
    expressions.push(expressionList.slice(offset, end)
      .replaceAll('&quot;', '\"').replaceAll('&amp;', '&'));
    offset = end + 1;
  }
  return {
    path,
    identity,
    args: expressions.map(source => plain(vm.runInNewContext(`cx => (${source})`, context)(cx))),
  };
}

module.exports = {emittedShard};
