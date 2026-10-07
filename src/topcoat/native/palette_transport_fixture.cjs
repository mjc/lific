// Delay actual palette output without replacing its server, credentials,
// render inputs or frames. Every released byte came from connectToServer().
const assert = require('node:assert/strict');

async function paletteTransport(page, prefix) {
  let armed;
  let candidate;
  const delivered = [];
  const deliveryObservers = [];
  const runPaths = new Map();
  await page.exposeFunction('__lificPaletteFrameReceived', message => {
    const envelope = JSON.parse(message);
    const frame = {...envelope.frame, run: envelope.run, path: runPaths.get(envelope.run)};
    delivered.push(frame);
    for (const observer of deliveryObservers) observer(frame);
  });
  // WebSocketRoute.send is delivered by Playwright's page shim, so CDP's
  // framereceived event cannot prove receipt. Observe actual message delivery.
  await page.addInitScript(({prefix}) => {
    const dispatchEvent = EventTarget.prototype.dispatchEvent;
    EventTarget.prototype.dispatchEvent = function(event) {
      if (event instanceof MessageEvent && typeof this.url === 'string'
          && new URL(this.url, location.href).pathname === `${prefix}/`) {
        void window.__lificPaletteFrameReceived(event.data);
      }
      return dispatchEvent.call(this, event);
    };
    const NativeWebSocket = window.WebSocket;
    window.WebSocket = new Proxy(NativeWebSocket, {
      construct(target, argumentsList, newTarget) {
        const socket = Reflect.construct(target, argumentsList, newTarget);
        if (new URL(argumentsList[0], location.href).pathname === `${prefix}/`) {
          socket.addEventListener('message', event => {
            void window.__lificPaletteFrameReceived(event.data);
          });
        }
        return socket;
      },
    });
  }, {prefix});
  await page.routeWebSocket(url => url.pathname === `${prefix}/`, socket => {
    const server = socket.connectToServer();
    let closed = false;
    const closedObservers = [];
    const markClosed = () => {
      closed = true;
      for (const observer of closedObservers) observer();
    };
    socket.onClose((code, reason) => {markClosed(); return server.close({code, reason});});
    server.onClose((code, reason) => {markClosed(); return socket.close({code, reason});});
    socket.onMessage(message => {
      const request = JSON.parse(message.toString());
      if (request.path) {
        runPaths.set(request.run, request.path);
        if (armed && request.path === '/__native_home/palette') {
          candidate = {run: request.run, messages: []};
        }
      }
      server.send(message);
    });
    server.onMessage(message => {
      const envelope = JSON.parse(message.toString());
      if (!armed || !candidate || envelope.run !== candidate.run) {
        socket.send(message);
        return;
      }
      candidate.messages.push(message);
      const frame = envelope.frame;
      if (!frame || !['snapshot', 'redirect', 'error'].includes(frame.t)) return;
      if (frame.t === 'snapshot' && frame.html.includes(armed.fragment)) {
        const held = armed;
        armed = undefined;
        const run = candidate.run;
        const messages = candidate.messages;
        candidate = undefined;
        const received = new Promise(resolve => deliveryObservers.push(actual => {
          if (actual.run === run && actual.t === 'snapshot' && actual.html.includes(held.fragment)) resolve();
        }));
        held.resolve({
          run,
          received,
          closed: new Promise(resolve => {
            if (closed) resolve(); else closedObservers.push(resolve);
          }),
          release() {
            if (closed) return false;
            for (const actual of messages) socket.send(actual);
            return true;
          },
        });
      } else {
        for (const actual of candidate.messages) socket.send(actual);
        candidate = undefined;
      }
    });
  });
  return {
    hold(fragment) {
      assert.equal(armed, undefined, 'Only one actual render is held at a time.');
      return new Promise(resolve => {armed = {fragment, resolve};});
    },
    delivered,
  };
}

module.exports = {paletteTransport};
