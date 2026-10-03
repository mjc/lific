const { afterEach, beforeEach, describe, expect, test } = require("./assertions.js");

class MemoryStorage {
  store = new Map;
  getItem(key) {
    return this.store.has(key) ? this.store.get(key) : null;
  }
  setItem(key, value) {
    this.store.set(key, String(value));
  }
  removeItem(key) {
    this.store.delete(key);
  }
}
let storage;
let calls;
let reply;
const originalFetch = globalThis.fetch;
beforeEach(() => {
  storage = new MemoryStorage;
  globalThis.localStorage = storage;
  globalThis.window ??= {
    location: { origin: "http://localhost" }
  };
  calls = [];
  reply = { status: 200, body: {} };
  globalThis.fetch = async (url, options = {}) => {
    const headers = options.headers ?? {};
    calls.push({ url, auth: headers["Authorization"] });
    return {
      ok: reply.status >= 200 && reply.status < 300,
      status: reply.status,
      json: async () => reply.body
    };
  };
});
afterEach(() => {
  globalThis.fetch = originalFetch;
});
async function api() {
  return require("./auth-adapter.js");
}
describe("changePassword", () => {
  test("adopts the replacement session so the next request is authenticated", async () => {
    const { changePassword, me } = await api();
    storage.setItem("lific_token", "lific_sess_old");
    reply = {
      status: 200,
      body: { ok: true, token: "lific_sess_new", expires_at: "2099-01-01T00:00:00Z" }
    };
    const result = await changePassword({
      current_password: "old",
      new_password: "new-password"
    });
    expect(result.ok).toBe(true);
    expect(storage.getItem("lific_token")).toBe("lific_sess_new");
    reply = { status: 200, body: {} };
    await me();
    expect(calls.at(-1)?.auth).toBe("Bearer lific_sess_new");
  });
  test("leaves the current session alone when the change is rejected", async () => {
    const { changePassword } = await api();
    storage.setItem("lific_token", "lific_sess_old");
    reply = { status: 400, body: { error: "current password is incorrect" } };
    const result = await changePassword({
      current_password: "wrong",
      new_password: "new-password"
    });
    expect(result.ok).toBe(false);
    expect(storage.getItem("lific_token")).toBe("lific_sess_old");
  });
});
describe("revokeAllSessions", () => {
  test("clears the local session once the server confirms the revocation", async () => {
    const { revokeAllSessions } = await api();
    storage.setItem("lific_token", "lific_sess_old");
    reply = { status: 200, body: { revoked: true } };
    const result = await revokeAllSessions();
    expect(result.ok).toBe(true);
    expect(storage.getItem("lific_token")).toBeNull();
  });
  test("keeps the local session when the request fails, so the retry has a credential", async () => {
    const { revokeAllSessions } = await api();
    storage.setItem("lific_token", "lific_sess_old");
    reply = { status: 500, body: { error: "database error" } };
    const result = await revokeAllSessions();
    expect(result.ok).toBe(false);
    expect(storage.getItem("lific_token")).toBe("lific_sess_old");
  });
  test("keeps the local session when the server is unreachable", async () => {
    const { revokeAllSessions } = await api();
    storage.setItem("lific_token", "lific_sess_old");
    globalThis.fetch = async () => {
      throw new TypeError("network down");
    };
    const result = await revokeAllSessions();
    expect(result.ok).toBe(false);
    expect(storage.getItem("lific_token")).toBe("lific_sess_old");
  });
});
