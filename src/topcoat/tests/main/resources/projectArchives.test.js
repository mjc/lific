// Ported from web/tests/projectArchives.test.ts on master 9683d38a.
const { afterEach, beforeEach, expect, test }=require('./assertions.js');
let restore;
beforeEach(() => {
  const { fetch, window, localStorage: storage, document } = globalThis;
  const values = new Map;
  Object.assign(globalThis, {
    window: Object.assign(new EventTarget, { location: { origin: "http://localhost" } }),
    localStorage: {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => values.set(key, value),
      removeItem: (key) => values.delete(key)
    },
    document: { createElement: () => {
      throw new Error("Downloading bytes must not save a file");
    } }
  });
  restore = () => Object.assign(globalThis, { fetch, window, localStorage: storage, document });
});
afterEach(() => restore());
test("session observers see same-tab refresh and logout and unsubscribe cleanly", async () => {
  const { onSessionChange, saveSession, clearSession } = require('./archive-adapter.js');
  const seen = [];
  const unsubscribe = onSessionChange(() => seen.push(localStorage.getItem("lific_token")));
  try {
    saveSession("first");
    saveSession("refreshed");
    clearSession();
    expect(seen).toEqual(["first", "refreshed", null]);
  } finally {
    unsubscribe();
  }
  saveSession("later");
  expect(seen).toHaveLength(3);
});
