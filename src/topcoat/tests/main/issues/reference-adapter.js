const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {setMaxListeners} = require('node:events');
const {sessionFixture} = require('../shell/session-harness.js');

// Observe the current hover-reference controller. This adapter supplies DOM and
// clock boundaries; it adds no cache, request queue, refresh listener or retry.
function createFixture(window) {
  // Node warns at ten DOM listeners; browsers impose no equivalent limit.
  setMaxListeners(0, window);
  let session;
  const currentSession = () => session ||= sessionFixture().session;
  const direct = async (route, signal) => currentSession().request(route, {signal});
  const fetchIssueCached = async (identifier, signal) => {
    const result = await direct(`/issues/resolve/${encodeURIComponent(identifier)}`, signal);
    return result.ok ? {status: 'ok', issue: result.data} : {status: 'unavailable'};
  };
  const fetchModuleCached = async (id, signal) => {
    const result = await direct(`/modules/${id}`, signal);
    return result.ok ? result.data : null;
  };
  const source = fs.readFileSync(path.resolve(__dirname, '../../../plans/assets/picker.js'), 'utf8');
  function subscribeIssueStatus(identifier, subscriber) {
    let issue;
    const document = {createElement: () => ({dataset: {}, style: {}, setAttribute() {},
      offsetWidth: 100, offsetHeight: 50, remove() {}})};
    const link = Object.assign(new EventTarget(), {dataset: {issueIdent: identifier}, isConnected: true,
      getBoundingClientRect: () => ({left: 10, bottom: 20})});
    const container = {querySelectorAll: () => [link], append: () => subscriber({status: 'ok', issue})};
    let sequence = 0;
    const context = {document, location: window.location, innerWidth: 1000, innerHeight: 700,
      Event, URL, console, setTimeout(callback) {callback(); return ++sequence;}, clearTimeout() {},
      addEventListener: window.addEventListener.bind(window),
      removeEventListener: window.removeEventListener.bind(window),
      dispatchEvent: window.dispatchEvent.bind(window)};
    vm.runInNewContext(source, context, {filename: 'plans/assets/picker.js'});
    const controller = context.LificTopcoatIssuePicker.bindReferences(container, {
      request: async route => {
        const result = await direct(route);
        if (!result.ok) throw new Error(result.error);
        issue = result.data;
        return issue;
      },
    });
    link.dispatchEvent(new Event('focus'));
    return () => controller.dispose();
  }
  // The current reference binding listens to no cache-invalidation event.
  // Dispatch the actual realtime event and let the production listeners react.
  const invalidateReferenceCache = () => window.dispatchEvent(new CustomEvent('lific:realtime',
    {detail: {type: 'issue.updated'}}));
  return {fetchIssueCached, fetchModuleCached, subscribeIssueStatus, invalidateReferenceCache};
}

module.exports = {createFixture};
