// Port of main 9683d38a web/tests/saveQueue.test.ts; original assertions retained.
const { describe, expect, test } = require("./harness");
const { createSaveQueue } = require("./save-adapter");
function deferredSender() {
  const sent = [];
  const gates = [];
  const send = (patch) => {
    sent.push(patch);
    return new Promise((resolve) => gates.push(resolve));
  };
  return {
    send,
    sent,
    finish(index, landed = true) {
      gates[index](landed);
    }
  };
}
describe("save queue", () => {
  test("sends one patch straight through", async () => {
    const io = deferredSender();
    const queue = createSaveQueue({ send: io.send });
    const settled = queue.push({ allow_signup: true });
    expect(io.sent).toEqual([{ allow_signup: true }]);
    io.finish(0);
    expect(await settled).toBe(true);
    expect(queue.busy).toBe(false);
  });
  test("an edit made during an in-flight save is not dropped", async () => {
    const io = deferredSender();
    const queue = createSaveQueue({ send: io.send });
    const first = queue.push({ allow_signup: true });
    const second = queue.push({ instance_name: "Lific" });
    expect(io.sent).toHaveLength(1);
    io.finish(0);
    expect(await first).toBe(true);
    expect(io.sent).toEqual([{ allow_signup: true }, { instance_name: "Lific" }]);
    io.finish(1);
    expect(await second).toBe(true);
  });
  test("queued patches coalesce, last write winning per field", async () => {
    const io = deferredSender();
    const queue = createSaveQueue({ send: io.send });
    const first = queue.push({ allow_signup: true });
    queue.push({ web_auto_login: true });
    queue.push({ web_auto_login: false });
    const last = queue.push({ instance_name: "Lific" });
    expect(queue.pending).toEqual({ web_auto_login: false, instance_name: "Lific" });
    io.finish(0);
    await first;
    expect(io.sent[1]).toEqual({ web_auto_login: false, instance_name: "Lific" });
    io.finish(1);
    await last;
    expect(io.sent).toHaveLength(2);
  });
  test("everyone who pushed into a coalesced batch learns how it went", async () => {
    const io = deferredSender();
    const queue = createSaveQueue({ send: io.send });
    const first = queue.push({ a: 1 });
    const second = queue.push({ b: 2 });
    const third = queue.push({ c: 3 });
    io.finish(0, true);
    expect(await first).toBe(true);
    io.finish(1, false);
    expect(await second).toBe(false);
    expect(await third).toBe(false);
  });
  test("a failure does not stall the queue", async () => {
    const io = deferredSender();
    const queue = createSaveQueue({ send: io.send });
    const failing = queue.push({ a: 1 });
    io.finish(0, false);
    expect(await failing).toBe(false);
    expect(queue.busy).toBe(false);
    const next = queue.push({ b: 2 });
    expect(io.sent).toHaveLength(2);
    io.finish(1, true);
    expect(await next).toBe(true);
  });
  test("reports busy while a save is in flight and idle again afterwards", async () => {
    const io = deferredSender();
    const states = [];
    const queue = createSaveQueue({
      send: io.send,
      onStateChange: (state) => states.push(state)
    });
    const settled = queue.push({ a: 1 });
    expect(queue.busy).toBe(true);
    io.finish(0);
    await settled;
    expect(queue.busy).toBe(false);
    expect(states).toEqual(["sending", "idle"]);
  });
  test("an edit arriving after dispatch is not merged into the sent patch", async () => {
    const io = deferredSender();
    const queue = createSaveQueue({ send: io.send });
    const first = queue.push({ a: 1 });
    expect(io.sent[0]).toEqual({ a: 1 });
    queue.push({ a: 2 });
    expect(io.sent[0]).toEqual({ a: 1 });
    io.finish(0);
    await first;
    expect(io.sent[1]).toEqual({ a: 2 });
  });
});
