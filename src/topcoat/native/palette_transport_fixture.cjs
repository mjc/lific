// Delay actual palette output without replacing its server, credentials,
// render inputs or frames. Every released byte came from connectToServer().
const assert = require('node:assert/strict');

async function paletteTransport(page, prefix) {
  let armed;
  const delivered = [];
  const deliveryObservers = [];
  await page.exposeFunction('__lificPaletteFrameReceived', message => {
    const frame = JSON.parse(message);
    delivered.push(frame);
    for (const observer of deliveryObservers) observer(frame);
  });
  // WebSocketRoute.send is delivered by Playwright's page shim, so CDP's
  // framereceived event cannot prove receipt. Observe the browser's actual
  // message event; preserve every constructor argument and returned socket.
  await page.addInitScript(({prefix}) => {
    const dispatchEvent = EventTarget.prototype.dispatchEvent;
    EventTarget.prototype.dispatchEvent = function(event) {
      // Playwright may install its socket shim after the constructor observer.
      // The shim source delivers through EventTarget.dispatchEvent, so observe
      // that actual delivery too, preserving the original dispatch unchanged.
      if (event instanceof MessageEvent && typeof this.url === 'string'
          && new URL(this.url, location.href).pathname === `${prefix}/__native_home/palette`) {
        void window.__lificPaletteFrameReceived(event.data);
      }
      return dispatchEvent.call(this, event);
    };
    const NativeWebSocket = window.WebSocket;
    window.WebSocket = new Proxy(NativeWebSocket, {
      construct(target, argumentsList, newTarget) {
        const socket = Reflect.construct(target, argumentsList, newTarget);
        if (new URL(argumentsList[0], location.href).pathname === `${prefix}/__native_home/palette`) {
          socket.addEventListener('message', event => {
            void window.__lificPaletteFrameReceived(event.data);
          });
        }
        return socket;
      },
    });
  }, {prefix});
  await page.routeWebSocket(url => url.pathname === `${prefix}/__native_home/palette`, socket => {
    const server = socket.connectToServer();
    let candidate = [];
    let closed = false;
    const closedObservers = [];
    const markClosed = () => {
      closed = true;
      for (const observer of closedObservers) observer();
    };
    socket.onClose((code, reason) => {markClosed(); return server.close({code, reason});});
    server.onClose((code, reason) => {markClosed(); return socket.close({code, reason});});
    server.onMessage(message => {
      const frame = JSON.parse(message.toString());
      if (!armed) {socket.send(message); return;}
      if (frame.t === 'run') {
        for (const previous of candidate) socket.send(previous);
        candidate = [message];
        return;
      }
      if (candidate.length === 0) {socket.send(message); return;}
      candidate.push(message);
      if (frame.t === 'snapshot' || frame.t === 'redirect' || frame.t === 'error') {
        if (frame.t === 'snapshot' && frame.html.includes(armed.fragment)) {
          const held = armed;
          armed = undefined;
          const frames = candidate;
          candidate = [];
          const run = JSON.parse(frames[0].toString()).id;
          assert.ok(Number.isInteger(run), 'Hold an actual announced framework run.');
          let receivingHeldRun = false;
          const received = new Promise(resolve => deliveryObservers.push(actual => {
            if (actual.t === 'run') receivingHeldRun = actual.id === run;
            if (receivingHeldRun && actual.t === 'snapshot' && actual.html.includes(held.fragment)) resolve();
          }));
          held.resolve({
            run,
            received,
            closed: new Promise(resolve => {
              if (closed) resolve(); else closedObservers.push(resolve);
            }),
            release() {
              if (closed) return false;
              for (const actual of frames) socket.send(actual);
              return true;
            },
          });
        } else {
          for (const actual of candidate) socket.send(actual);
          candidate = [];
        }
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
