import { afterEach, beforeEach, describe, expect, test } from "bun:test";

class MemoryStorage {
  private store = new Map<string, string>();

  getItem(key: string) {
    return this.store.get(key) ?? null;
  }

  setItem(key: string, value: string) {
    this.store.set(key, value);
  }
}

let fetchIssueCached: typeof import("../src/lib/references").fetchIssueCached;
let invalidateReferenceCache: typeof import("../src/lib/references").invalidateReferenceCache;

let storage: MemoryStorage;
let status = "todo";
let calls = 0;
let active = 0;
let peakActive = 0;
let fail = false;
let failureStatus = 503;
let gate: Promise<void> | null = null;
let releaseGate: (() => void) | null = null;
const originalFetch = globalThis.fetch;

beforeEach(async () => {
  (globalThis as { window?: unknown }).window = {
    location: { origin: "http://localhost" },
  };
  ({ fetchIssueCached, invalidateReferenceCache } = await import("../src/lib/references"));
  storage = new MemoryStorage();
  (globalThis as { localStorage: unknown }).localStorage = storage;
  calls = 0;
  active = 0;
  peakActive = 0;
  fail = false;
  failureStatus = 503;
  gate = null;
  releaseGate = null;
  globalThis.fetch = (async () => {
    calls += 1;
    active += 1;
    peakActive = Math.max(peakActive, active);
    if (gate) await gate;
    active -= 1;
    if (fail) return { ok: false, status: failureStatus, json: async () => ({ error: "busy" }) };
    return {
      ok: true,
      status: 200,
      json: async () => ({ identifier: "LIF-1", status }),
    };
  }) as unknown as typeof fetch;
});

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe("fetchIssueCached", () => {
  test("does not reuse an account's issue snapshot after the session changes", async () => {
    storage.setItem("lific_token", "account-a");
    status = "done";
    const first = await fetchIssueCached("LIF-1");
    expect(first.status).toBe("ok");
    if (first.status === "ok") expect(first.issue.status).toBe("done");

    storage.setItem("lific_token", "account-b");
    status = "todo";
    const second = await fetchIssueCached("LIF-1");
    expect(second.status).toBe("ok");
    if (second.status === "ok") expect(second.issue.status).toBe("todo");
    expect(calls).toBe(2);
  });

  test("retries a transient failure", async () => {
    storage.setItem("lific_token", "retry");
    fail = true;
    expect((await fetchIssueCached("LIF-2")).status).toBe("unavailable");

    fail = false;
    status = "active";
    const retry = await fetchIssueCached("LIF-2");
    expect(retry.status).toBe("ok");
    if (retry.status === "ok") expect(retry.issue.status).toBe("active");
    expect(calls).toBe(2);
  });

  test("caches stable not-found responses", async () => {
    storage.setItem("lific_token", "not-found");
    fail = true;
    failureStatus = 404;
    expect((await fetchIssueCached("LIF-404")).status).toBe("unavailable");
    expect((await fetchIssueCached("LIF-404")).status).toBe("unavailable");
    expect(calls).toBe(1);
  });

  test("re-fetches a status after realtime invalidation", async () => {
    storage.setItem("lific_token", "realtime");
    status = "todo";
    await fetchIssueCached("LIF-3");

    status = "done";
    invalidateReferenceCache();
    const refreshed = await fetchIssueCached("LIF-3");
    expect(refreshed.status).toBe("ok");
    if (refreshed.status === "ok") expect(refreshed.issue.status).toBe("done");
    expect(calls).toBe(2);
  });

  test("caps concurrent issue resolution", async () => {
    storage.setItem("lific_token", "bounded");
    gate = new Promise<void>((resolve) => { releaseGate = resolve; });
    const pending = Array.from({ length: 12 }, (_, i) => fetchIssueCached(`LIF-${i + 10}`));
    await Promise.resolve();
    expect(peakActive).toBeLessThanOrEqual(6);
    releaseGate!();
    await Promise.all(pending);
  });
});
