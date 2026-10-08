'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const {handlerFixture} = require('./handler_fixture.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
const unbox = value => {
  if (value && typeof value.dehydrate === 'function') return unbox(value.dehydrate());
  if (value && typeof value === 'object' && Object.hasOwn(value, 'v')) return unbox(value.v);
  return value;
};

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}

function fixture(testCase, fetch) {
  const f = handlerFixture(testCase.signals, fetch, input.browser_source);
  // Browser I/O and its errors share the window's realm. The fake I/O lives
  // in Node, so expose its constructor to the evaluated browser primitive.
  f.context.Error = Error;
  const signal = f.cx.signal.bind(f.cx);
  const referenced = new Set();
  f.cx.signal = id => { referenced.add(id); return signal(id); };
  const anchors = [], appended = [], revoked = [], blobs = [];
  f.context.document.createElement = tag => {
    assert.equal(tag, 'a');
    const anchor = {
      href: '', download: '', removed: false,
      click() {
        anchor.clicked = true;
        if (testCase.throwAnchorClick) throw new Error('anchor click failed');
      },
      remove() { anchor.removed = true; },
    };
    anchors.push(anchor);
    return anchor;
  };
  f.context.document.body = {appendChild(anchor) { appended.push(anchor); }};
  f.context.URL = {
    createObjectURL(blob) {
      const url = `blob:export-${revoked.length + 1}`;
      blobs.push(blob);
      return url;
    },
    revokeObjectURL(url) { revoked.push(url); },
  };
  const values = () => Object.fromEntries([...referenced].map(id => [id, unbox(signal(id).get().dehydrate())]));
  const initial = () => Object.fromEntries([...referenced].map(id => [id, unbox(testCase.signals[id])]));
  const invoke = source => f.handler(source)(f.cx.event(new f.context.Event('click')));
  const invokeMount = () => f.handler(testCase.mount)(f.cx.event(new f.context.Event('mount')));
  const findSignal = predicate => [...referenced].find(id => predicate(unbox(testCase.signals[id])));
  return {...f, anchors, appended, revoked, blobs, invoke, invokeMount, values, initial, findSignal, referenced};
}

function response({status = 200, filename = 'page with spaces.md', blob = {bytes: 'committed bytes'}, blobImpl} = {}) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: {get(name) {
      assert.equal(name, 'content-disposition');
      return `attachment; filename="${filename}"`;
    }},
    async blob() {
      if (blobImpl) return blobImpl();
      return blob;
    },
  };
}

async function waitFor(predicate, message) {
  for (let index = 0; index < 20; index++) {
    if (predicate()) return;
    await flush();
  }
  assert.fail(message);
}

async function runCase(testCase) {
  const requests = [];
  const fetchPending = deferred();
  const f = fixture(testCase, (url, options) => {
    requests.push({url: String(url), options});
    return fetchPending.promise;
  });
  f.invokeMount();
  f.invoke(testCase.click);
  await flush();
  assert.equal(requests.length, 1, `${testCase.kind} starts one native download`);
  assert.equal(requests[0].url, testCase.endpoint, `${testCase.kind} uses the mounted native endpoint`);
  assert.equal(requests[0].options.method, undefined, 'native export uses the browser GET default');
  assert.equal(requests[0].options.redirect, 'error');
  assert.equal(requests[0].options.signal, f.controller.signal);
  const busyId = f.findSignal(value => value === false);
  assert.ok(busyId, `${testCase.kind} handler owns a false busy signal`);
  assert.equal(f.values()[busyId], true, 'the emitted click handler sets pending state');
  f.invoke(testCase.click);
  assert.equal(requests.length, 1, 'a duplicate click does not issue another GET');
  const committedBlob = {bytes: 'committed bytes'};
  fetchPending.resolve(response({blob: committedBlob}));
  await waitFor(() => f.values()[busyId] === false, 'success settles the actual Rust busy signal');
  const errorId = f.findSignal(value => value === '');
  assert.ok(errorId, `${testCase.kind} handler owns an error signal`);
  assert.equal(f.values()[errorId], '');
  assert.equal(f.anchors.length, 1);
  assert.equal(f.anchors[0].download, 'page with spaces.md', 'the actual response filename is used');
  assert.equal(f.anchors[0].href, 'blob:export-1');
  assert.equal(f.anchors[0].clicked, true);
  assert.deepEqual(f.blobs, [committedBlob], 'the response body reaches the browser download boundary');
  assert.equal(f.anchors[0].removed, true, 'the temporary anchor is removed');
  assert.deepEqual(f.revoked, ['blob:export-1'], 'the object URL is revoked');
  assert.deepEqual(f.appended, f.anchors);

  async function failureAndRetry(name, failFetch, expectedMessage) {
    let calls = 0;
    const retry = fixture(testCase, (url, options) => {
      calls++;
      assert.equal(String(url), testCase.endpoint);
      assert.equal(options.method, undefined, 'retry remains a GET');
      assert.equal(options.redirect, 'error');
      if (calls === 1) return failFetch();
      return Promise.resolve(response());
    });
    retry.invokeMount();
    // The failed request writes a real Rust error signal; retry must clear it before I/O completes.
    retry.invoke(testCase.click);
    const errorSignal = [...retry.referenced].find(id => unbox(testCase.signals[id]) === '');
    const busySignal = [...retry.referenced].find(id => unbox(testCase.signals[id]) === false);
    assert.ok(errorSignal && busySignal);
    await waitFor(() => retry.values()[errorSignal] === expectedMessage && retry.values()[busySignal] === false,
      `${name} reaches the completion path`);
    assert.equal(retry.values()[errorSignal], expectedMessage, `${name} is rendered by Rust error state`);
    assert.equal(retry.values()[busySignal], false, `${name} clears busy state`);
    retry.invoke(testCase.click);
    assert.equal(retry.values()[errorSignal], '', 'retry clears the prior error before I/O completes');
    assert.equal(retry.values()[busySignal], true);
    await waitFor(() => retry.values()[busySignal] === false, 'retry success clears busy state');
    assert.equal(retry.values()[errorSignal], '');
    assert.equal(calls, 2, 'the failed export can be retried');
    assert.equal(retry.anchors.length, 1);
    assert.equal(retry.anchors[0].removed, true);
    assert.equal(retry.revoked.length, 1);
  }

  await failureAndRetry('HTTP failure', () => Promise.resolve(response({status: 429})), 'HTTP 429');
  await failureAndRetry('network failure', () => Promise.reject(new Error('network unavailable')), 'network unavailable');
  await failureAndRetry('blob failure', () => Promise.resolve(response({blobImpl: async () => { throw new Error('blob unavailable'); }})), 'blob unavailable');

  const clickFailure = fixture({...testCase, throwAnchorClick: true}, async () => response());
  clickFailure.invokeMount();
  clickFailure.invoke(testCase.click);
  const busyAfterClick = [...clickFailure.referenced].find(id => unbox(testCase.signals[id]) === false);
  const errorAfterClick = [...clickFailure.referenced].find(id => unbox(testCase.signals[id]) === '');
  await waitFor(() => clickFailure.values()[busyAfterClick] === false, 'anchor failure settles busy state');
  assert.equal(clickFailure.values()[errorAfterClick], 'anchor click failed');
  assert.equal(clickFailure.anchors[0].removed, true, 'anchor cleanup runs when click throws');
  assert.deepEqual(clickFailure.revoked, ['blob:export-1'], 'URL cleanup runs when click throws');

  for (const phase of ['fetch', 'blob']) {
    const pendingFetch = deferred(), pendingBlob = deferred();
    let blobStarted = false;
    const disposed = fixture(testCase, async () => {
      if (phase === 'fetch') return pendingFetch.promise;
      return response({blobImpl: () => { blobStarted = true; return pendingBlob.promise; }});
    });
    disposed.invokeMount();
    disposed.invoke(testCase.click);
    await flush();
    if (phase === 'blob') await waitFor(() => blobStarted, 'blob read starts before retirement');
    const beforeRetirement = disposed.values();
    disposed.controller.abort();
    if (phase === 'fetch') pendingFetch.resolve(response());
    else pendingBlob.resolve({bytes: 'late bytes'});
    await flush();
    assert.deepEqual(disposed.values(), beforeRetirement, `${phase} completion after disposal writes no signals`);
    assert.equal(disposed.anchors.length, 0, `${phase} completion after disposal starts no download`);
    assert.equal(disposed.revoked.length, 0);
  }

  const restored = fixture(testCase, () => new Promise(() => {}));
  restored.invoke(testCase.click);
  const staleBusy = [...restored.referenced].find(id => unbox(testCase.signals[id]) === false);
  assert.ok(staleBusy);
  restored.cx.signal(staleBusy).set(restored.cx.hydrate(true));
  restored.invokeMount();
  assert.equal(restored.values()[staleBusy], false, 'the actual mount handler resets a restored busy state');
  return {kind: testCase.kind, endpoint: testCase.endpoint};
}

(async () => {
  assert.ok(input.cases.length >= 2, 'Rust supplies actual Issue and Page SSR handlers');
  const kinds = new Set(input.cases.map(testCase => testCase.kind));
  assert.ok(kinds.has('issue') && kinds.has('page'));
  const results = [];
  for (const testCase of input.cases) results.push(await runCase(testCase));
  process.stdout.write(JSON.stringify({passed: true, cases: results}));
})().catch(error => {
  process.stderr.write(`${error.stack}\n`);
  process.exitCode = 1;
});
