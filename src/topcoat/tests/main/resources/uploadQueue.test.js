// Ported from web/tests/uploadQueue.test.ts on master 9683d38a.
const { expect, test }=require('./assertions.js');
const { createConcurrencyQueue, settle }=require('./queue-adapter.js');
function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}
test("runs at most `limit` tasks at once", async () => {
  const queue = createConcurrencyQueue(3);
  const gates = Array.from({ length: 6 }, () => deferred());
  const started = [];
  const results = gates.map((gate, i) => queue.add(() => {
    started.push(i);
    return gate.promise;
  }));
  await settle();
  expect(started).toEqual([0, 1, 2]);
  expect(queue.active).toBe(3);
  expect(queue.waiting).toBe(3);
  gates[1].resolve("b");
  await results[1];
  expect(started).toEqual([0, 1, 2, 3]);
  expect(queue.active).toBe(3);
  for (const gate of gates)
    gate.resolve("done");
  await Promise.all(results);
  expect(started).toEqual([0, 1, 2, 3, 4, 5]);
  expect(queue.active).toBe(0);
  expect(queue.waiting).toBe(0);
});
test("a rejected task frees its slot and does not stall the queue", async () => {
  const queue = createConcurrencyQueue(2);
  const first = deferred();
  const second = deferred();
  let thirdStarted = false;
  const a = queue.add(() => first.promise);
  const b = queue.add(() => second.promise);
  const c = queue.add(async () => {
    thirdStarted = true;
    return "c";
  });
  await settle();
  expect(thirdStarted).toBe(false);
  first.reject(new Error("boom"));
  await expect(a).rejects.toThrow("boom");
  await c;
  expect(thirdStarted).toBe(true);
  second.resolve("b");
  await b;
  expect(queue.active).toBe(0);
});
test("a task that throws synchronously rejects rather than wedging a slot", async () => {
  const queue = createConcurrencyQueue(1);
  const boom = queue.add(() => {
    throw new Error("sync boom");
  });
  await expect(boom).rejects.toThrow("sync boom");
  const after = await queue.add(async () => "ok");
  expect(after).toBe("ok");
  expect(queue.active).toBe(0);
});
test("resolves with each task's own value, in completion order", async () => {
  const queue = createConcurrencyQueue(3);
  const values = await Promise.all([
    queue.add(async () => 1),
    queue.add(async () => 2),
    queue.add(async () => 3),
    queue.add(async () => 4)
  ]);
  expect(values).toEqual([1, 2, 3, 4]);
});
test("a limit below one is clamped to serial execution", async (t) => {
  for (const [limit,expected] of [[0,1],[-5,1],[3.7,3]]) {
    await t.test(`limit ${limit}`,async()=>{
      const queue=createConcurrencyQueue(limit);let started=0;
      try {
        for(let index=0;index<6;index++) void queue.add(()=>{started++;return new Promise(()=>{});});
        await settle();expect(started).toBe(expected);expect(queue.active).toBe(expected);
      } finally {queue.dispose();}
    });
  }
});
