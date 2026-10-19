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
let bodies;
let replies;
const originalFetch = globalThis.fetch;
function reply(path, ...queued) {
  replies.set(path, queued);
}
beforeEach(() => {
  storage = new MemoryStorage;
  globalThis.localStorage = storage;
  globalThis.window ??= {
    location: { origin: "http://localhost" }
  };
  calls = [];
  bodies = [];
  replies = new Map;
  globalThis.fetch = async (url, options = {}) => {
    calls.push(url);
    bodies.push(options.body);
    const queued = [...replies.entries()].find(([path]) => url.endsWith(path))?.[1];
    const next = queued && queued.length > 1 ? queued.shift() : queued?.[0];
    const chosen = next ?? { status: 200, body: {} };
    return {
      ok: chosen.status >= 200 && chosen.status < 300,
      status: chosen.status,
      json: async () => chosen.body
    };
  };
});
afterEach(() => {
  globalThis.fetch = originalFetch;
});
async function mod() {
  return require("./reauth-adapter.js");
}
const STALE = { status: 403, body: { error: "recent authentication required" } };
const session = (id, token) => ({
  status: 200,
  body: { user: { id, username: "blake" }, token, expires_at: "2099-01-01T00:00:00Z" }
});
describe("needsReauth", () => {
  test("matches only the staleness refusal, not other 403s", async () => {
    const { needsReauth } = await mod();
    expect(needsReauth({ ok: false, error: "recent authentication required", status: 403 })).toBe(true);
    expect(needsReauth({ ok: false, error: "only an admin can do this", status: 403 })).toBe(false);
    expect(needsReauth({ ok: false, error: "authentication required", status: 403 })).toBe(false);
    expect(needsReauth({ ok: false, error: "recent authentication required", status: 401 })).toBe(false);
  });
});
describe("password re-authentication", () => {
  test("saves the fresh session so the retry carries it", async () => {
    const { reauthenticateWithPassword } = await mod();
    storage.setItem("lific_token", "lific_sess_stale");
    reply("/auth/me/refresh", session(7, "lific_sess_fresh"));
    const outcome = await reauthenticateWithPassword("hunter2", 7);
    expect(outcome.ok).toBe(true);
    expect(storage.getItem("lific_token")).toBe("lific_sess_fresh");
  });
  test("a wrong password leaves the existing session in place", async () => {
    const { reauthenticateWithPassword } = await mod();
    storage.setItem("lific_token", "lific_sess_stale");
    reply("/auth/me/refresh", { status: 401, body: { error: "invalid credentials" } });
    const outcome = await reauthenticateWithPassword("wrong", 7);
    expect(outcome).toEqual({
      ok: false,
      error: "invalid credentials",
      recoverable: false
    });
    expect(storage.getItem("lific_token")).toBe("lific_sess_stale");
  });
  test("a network failure leaves the existing session in place", async () => {
    const { reauthenticateWithPassword } = await mod();
    storage.setItem("lific_token", "lific_sess_stale");
    globalThis.fetch = async () => {
      throw new TypeError("network down");
    };
    const outcome = await reauthenticateWithPassword("hunter2", 7);
    expect(outcome.ok).toBe(false);
    expect(storage.getItem("lific_token")).toBe("lific_sess_stale");
  });
});
describe("passwordless re-authentication", () => {
  test("saves the fresh session on a passwordless instance", async () => {
    const { reauthenticateWithoutPassword } = await mod();
    storage.setItem("lific_token", "lific_sess_stale");
    reply("/auth/me/refresh", session(7, "lific_sess_fresh"));
    const outcome = await reauthenticateWithoutPassword(7);
    expect(outcome.ok).toBe(true);
    expect(storage.getItem("lific_token")).toBe("lific_sess_fresh");
  });
  test("refuses a session belonging to a different admin", async () => {
    const { reauthenticateWithoutPassword } = await mod();
    storage.setItem("lific_token", "lific_sess_stale");
    reply("/auth/me/refresh", session(99, "lific_sess_other_admin"));
    const outcome = await reauthenticateWithoutPassword(7);
    expect(outcome.ok).toBe(false);
    expect(storage.getItem("lific_token")).toBe("lific_sess_stale");
  });
  test("the same identity check applies to the password path", async () => {
    const { reauthenticateWithPassword } = await mod();
    storage.setItem("lific_token", "lific_sess_stale");
    reply("/auth/me/refresh", session(99, "lific_sess_other"));
    const outcome = await reauthenticateWithPassword("hunter2", 7);
    expect(outcome.ok).toBe(false);
    expect(storage.getItem("lific_token")).toBe("lific_sess_stale");
  });
});
describe("retryOnceAfterReauth", () => {
  test("passes a successful attempt straight through without re-authenticating", async () => {
    const { retryOnceAfterReauth } = await mod();
    let attempts = 0;
    let reauths = 0;
    const result = await retryOnceAfterReauth(async () => {
      attempts += 1;
      return { ok: true, data: "minted" };
    }, async () => {
      reauths += 1;
      return { ok: true };
    });
    expect(result).toEqual({ ok: true, data: "minted" });
    expect(attempts).toBe(1);
    expect(reauths).toBe(0);
  });
  test("re-authenticates once and retries once", async () => {
    const { retryOnceAfterReauth } = await mod();
    let attempts = 0;
    let reauths = 0;
    const result = await retryOnceAfterReauth(async () => {
      attempts += 1;
      if (attempts === 1) {
        return { ok: false, error: "recent authentication required", status: 403 };
      }
      return { ok: true, data: "minted" };
    }, async () => {
      reauths += 1;
      return { ok: true };
    });
    expect(result).toEqual({ ok: true, data: "minted" });
    expect(attempts).toBe(2);
    expect(reauths).toBe(1);
  });
  test("never retries more than once, even if it is refused again", async () => {
    const { retryOnceAfterReauth } = await mod();
    let attempts = 0;
    let reauths = 0;
    const result = await retryOnceAfterReauth(async () => {
      attempts += 1;
      return { ok: false, error: "recent authentication required", status: 403 };
    }, async () => {
      reauths += 1;
      return { ok: true };
    });
    expect(result.ok).toBe(false);
    expect(attempts).toBe(2);
    expect(reauths).toBe(1);
  });
  test("does not retry when re-authentication itself fails", async () => {
    const { retryOnceAfterReauth } = await mod();
    let attempts = 0;
    const result = await retryOnceAfterReauth(async () => {
      attempts += 1;
      return { ok: false, error: "recent authentication required", status: 403 };
    }, async () => ({ ok: false, error: "invalid credentials" }));
    expect(result).toEqual({ ok: false, error: "invalid credentials", status: 403 });
    expect(attempts).toBe(1);
  });
  test("a non-staleness failure is returned untouched", async () => {
    const { retryOnceAfterReauth } = await mod();
    let reauths = 0;
    const result = await retryOnceAfterReauth(async () => ({ ok: false, error: "only an admin can do this", status: 403 }), async () => {
      reauths += 1;
      return { ok: true };
    });
    expect(result).toEqual({ ok: false, error: "only an admin can do this", status: 403 });
    expect(reauths).toBe(0);
  });
});
describe("passwordless failure falls back to the password prompt", () => {
  test("a refused passwordless refresh is recoverable so callers can prompt", async () => {
    const { reauthenticateWithoutPassword } = await mod();
    storage.setItem("lific_token", "lific_sess_stale");
    reply("/auth/me/refresh", { status: 400, body: { error: "your password is required to confirm this" } });
    const outcome = await reauthenticateWithoutPassword(7);
    expect(outcome).toEqual({
      ok: false,
      error: "your password is required to confirm this",
      recoverable: true
    });
    expect(storage.getItem("lific_token")).toBe("lific_sess_stale");
  });
  test("a refresh naming another account is recoverable, and the token is untouched", async () => {
    const { reauthenticateWithoutPassword } = await mod();
    storage.setItem("lific_token", "lific_sess_stale");
    reply("/auth/me/refresh", session(99, "lific_sess_other_admin"));
    const outcome = await reauthenticateWithoutPassword(7);
    expect(outcome.ok).toBe(false);
    expect(outcome.ok === false && outcome.recoverable).toBe(true);
    expect(storage.getItem("lific_token")).toBe("lific_sess_stale");
  });
  test("a recoverable failure surfaces as staleness so the caller prompts", async () => {
    const { retryOnceAfterReauth, needsReauth } = await mod();
    const result = await retryOnceAfterReauth(async () => ({ ok: false, error: "recent authentication required", status: 403 }), async () => ({
      ok: false,
      error: "your password is required to confirm this",
      recoverable: true
    }));
    expect(result.ok).toBe(false);
    expect(result.ok === false && needsReauth(result)).toBe(true);
  });
  test("an unrecoverable failure surfaces its own message instead", async () => {
    const { retryOnceAfterReauth, needsReauth } = await mod();
    const result = await retryOnceAfterReauth(async () => ({ ok: false, error: "recent authentication required", status: 403 }), async () => ({ ok: false, error: "invalid credentials", recoverable: false }));
    expect(result).toEqual({ ok: false, error: "invalid credentials", status: 403 });
    expect(result.ok === false && needsReauth(result)).toBe(false);
  });
  test("a refused passwordless refresh then a successful password path retries once", async () => {
    const { reauthenticateWithoutPassword, reauthenticateWithPassword, retryOnceAfterReauth } = await mod();
    storage.setItem("lific_token", "lific_sess_stale");
    reply("/auth/me/refresh", { status: 400, body: { error: "your password is required to confirm this" } }, session(7, "lific_sess_mine"));
    let attempts = 0;
    const pending = async () => {
      attempts += 1;
      return storage.getItem("lific_token") === "lific_sess_mine" ? { ok: true, data: "granted" } : { ok: false, error: "recent authentication required", status: 403 };
    };
    const auto = await retryOnceAfterReauth(pending, () => reauthenticateWithoutPassword(7));
    expect(auto.ok).toBe(false);
    expect(storage.getItem("lific_token")).toBe("lific_sess_stale");
    expect(attempts).toBe(1);
    const verified = await reauthenticateWithPassword("hunter2", 7);
    expect(verified.ok).toBe(true);
    expect(storage.getItem("lific_token")).toBe("lific_sess_mine");
    const retried = await pending();
    expect(retried).toEqual({ ok: true, data: "granted" });
    expect(attempts).toBe(2);
  });
  test("both routes call the same-user refresh endpoint", async () => {
    const { reauthenticateWithoutPassword, reauthenticateWithPassword } = await mod();
    reply("/auth/me/refresh", session(7, "lific_sess_fresh"));
    await reauthenticateWithoutPassword(7);
    await reauthenticateWithPassword("hunter2", 7);
    // Controller replay now reloads account sections as well; retain the original
    // assertion on the two refresh operations themselves.
    const refreshCalls=calls.flatMap((url,index)=>url.endsWith("/auth/me/refresh")?[{url,body:bodies[index]??JSON.stringify({})}]:[]);
    expect(refreshCalls.map(call=>call.url)).toEqual(["/api/auth/me/refresh", "/api/auth/me/refresh"]);
    expect(refreshCalls.map(call=>call.body)).toEqual([JSON.stringify({}), JSON.stringify({ password: "hunter2" })]);
  });
});
