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
test("archive fetch returns bytes without saving and forwards cancellation and session", async () => {
  const { downloadProjectArchive } = require('./archive-adapter.js');
  localStorage.setItem("lific_token", "lific_sess_test");
  const controller = new AbortController;
  let received;
  globalThis.fetch = async (url, options) => {
    received={url,options};
    return new Response("archive bytes", { headers: { "Content-Type": "application/gzip" } });
  };
  const result = await downloadProjectArchive("ARC", controller.signal);
  const {url,options}=received;
  expect(url).toBe("/api/project-archives/ARC");
  expect(options.signal).toBe(controller.signal);
  expect(options.cache).toBe("no-store");
  expect(new Headers(options.headers).get("Authorization")).toBe("Bearer lific_sess_test");

  expect(result.ok).toBe(true);
  if (result.ok) {
    expect(result.filename).toBe("ARC.lific.tar.gz");
    expect(await result.blob.text()).toBe("archive bytes");
  }
});
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
