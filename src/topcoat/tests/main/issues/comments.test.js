// Port of main 9683d38a web/tests/comments.test.ts. Live collaboration functions, no retired implementation.
const {test,expect,afterEach}=require("./harness");
const {listComments,loadCommentWindow,canManageComment,commentWasEdited,COMMENT_PAGE_SIZE,COMMENT_REFRESH_PAGE_BUDGET,COMMENT_REFRESH_TRANSFER_LIMIT,prependOlderComments,upsertComment,removeComment,compareComments,olderCursor,reconcileCommentWindow}=require("./comments-adapter");
const originalFetch=globalThis.fetch;afterEach(()=>{globalThis.fetch=originalFetch;});
function comment(overrides = {}) {
  return {
    id: 42,
    issue_id: 7,
    page_id: null,
    user_id: 3,
    author: "owner",
    author_display_name: "Owner",
    content: "Original",
    created_at: "2026-08-13 10:00:00",
    updated_at: "2026-08-13 10:00:00",
    ...overrides
  };
}
test("only the comment author gets web mutation actions", async () => {
  const own = comment();
  expect(canManageComment(own, { id: 3 }, true)).toBe(true);
  expect(canManageComment(own, { id: 9 }, true)).toBe(false);
  expect(canManageComment(own, { id: 3 }, false)).toBe(false);
});
test("marks comments edited only when the update timestamp changes", async () => {
  expect(commentWasEdited(comment())).toBe(false);
  expect(commentWasEdited(comment({ updated_at: "2026-08-13 10:05:00" }))).toBe(true);
});
test("folding a comment in by id is idempotent and keeps thread order", async () => {
  const at = (id, second, content = "body") => comment({ id, content, created_at: `2026-08-13 10:00:0${second}` });
  const first = at(1, 0);
  const third = at(3, 2);
  const edited = at(1, 0, "Revised");
  expect((await upsertComment([first, third], edited))).toEqual([edited, third]);
  const fourth = at(4, 3);
  expect((await upsertComment([first, third], fourth))).toEqual([first, third, fourth]);
  const second = at(2, 1);
  expect((await upsertComment([first, third], second))).toEqual([first, second, third]);
  const tie = comment({ id: 2, created_at: third.created_at });
  expect((await upsertComment([first, third], tie)).map((c) => c.id)).toEqual([1, 2, 3]);
  const laterTie = comment({ id: 9, created_at: third.created_at });
  expect((await upsertComment([first, third], laterTie)).map((c) => c.id)).toEqual([1, 3, 9]);
  const once = (await upsertComment([first, third], second));
  expect((await upsertComment(once, second))).toEqual([first, second, third]);
  expect(new Set((await upsertComment(once, second)).map((c) => c.id)).size).toBe(3);
  const input = [first, third];
  (await upsertComment(input, second));
  expect(input).toEqual([first, third]);
  expect((await removeComment([first, third], 1))).toEqual([third]);
  expect((await removeComment((await removeComment([first, third], 1)), 1))).toEqual([third]);
});
test("the thread comparator is the key the cursor pages by", async () => {
  const early = comment({ id: 9, created_at: "2026-08-13 10:00:00" });
  const late = comment({ id: 2, created_at: "2026-08-13 10:00:01" });
  expect((await compareComments(early, late))).toBeLessThan(0);
  expect((await compareComments(late, early))).toBeGreaterThan(0);
  const tie = comment({ id: 10, created_at: early.created_at });
  expect((await compareComments(early, tie))).toBeLessThan(0);
  expect((await compareComments(early, early))).toBe(0);
});
function stubThread(count) {
  const calls = [];
  globalThis.fetch = async (url) => {
    calls.push(String(url));
    const params = new URL(String(url), "http://localhost").searchParams;
    const limit = Number(params.get("limit"));
    const beforeId = params.get("before_id");
    const highest = beforeId === null ? count : Number(beforeId) - 1;
    const rows = Array.from({ length: Math.max(0, Math.min(highest, limit)) }, (_, i) => comment({ id: highest - i, content: `comment ${highest - i}` }));
    return new Response(JSON.stringify(rows), { status: 200 });
  };
  return calls;
}
test("requests the newest bounded comment page and shows it chronologically", async () => {
  const calls = stubThread(3);
  const res = await listComments(7);
  expect(calls).toEqual(["/api/issues/7/comments?order=desc&limit=51"]);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.items.map((c) => c.content)).toEqual([
    "comment 1",
    "comment 2",
    "comment 3"
  ]);
  expect(res.data.hasMore).toBe(false);
  expect(res.data.nextCursor).toEqual({ created_at: "2026-08-13 10:00:00", id: 1 });
});
test("infers hasMore from the over-fetched row without ever showing it", async () => {
  stubThread(1000);
  const res = await listComments(7);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.items).toHaveLength(COMMENT_PAGE_SIZE);
  expect(res.data.hasMore).toBe(true);
  expect(res.data.items.at(-1)?.content).toBe("comment 1000");
  expect(res.data.items[0].content).toBe(`comment ${1000 - COMMENT_PAGE_SIZE + 1}`);
  expect(res.data.nextCursor?.id).toBe(1000 - COMMENT_PAGE_SIZE + 1);
});
test("trusts the server about what lies past a page the byte budget cut short", async () => {
  const calls = [];
  globalThis.fetch = async (url) => {
    calls.push(String(url));
    const rows = [3, 2, 1].map((id) => comment({ id, content: `comment ${id}` }));
    return new Response(JSON.stringify(rows), {
      status: 200,
      headers: {
        "x-comment-has-more": "true",
        "x-comment-next-offset": "3",
        "x-comment-returned": "3"
      }
    });
  };
  const res = await listComments(7);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.items).toHaveLength(3);
  expect(res.data.hasMore).toBe(true);
  expect(res.data.nextCursor).toEqual({ created_at: "2026-08-13 10:00:00", id: 1 });
});
test("the cursor the client pages by is the row the server named", async () => {
  const rows = [9, 8, 7].map((id) => comment({ id, created_at: `2026-08-13 10:00:0${id % 10}` }));
  globalThis.fetch = async () => new Response(JSON.stringify(rows), {
    status: 200,
    headers: {
      "x-comment-has-more": "true",
      "x-comment-next-created-at": rows[rows.length - 1].created_at,
      "x-comment-next-id": String(rows[rows.length - 1].id)
    }
  });
  const res = await listComments(7);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.hasMore).toBe(true);
  expect(res.data.nextCursor).toEqual({
    created_at: rows[rows.length - 1].created_at,
    id: rows[rows.length - 1].id
  });
  expect(res.data.items[0].id).toBe(rows[rows.length - 1].id);
});
test("a short page from a server that says nothing is still the end of the thread", async () => {
  globalThis.fetch = async () => {
    const rows = [2, 1].map((id) => comment({ id }));
    return new Response(JSON.stringify(rows), { status: 200 });
  };
  const res = await listComments(7);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.items).toHaveLength(2);
  expect(res.data.hasMore).toBe(false);
});
test("a full page is more comments even when the header disagrees", async () => {
  globalThis.fetch = async () => {
    const rows = Array.from({ length: COMMENT_PAGE_SIZE + 1 }, (_, i) => comment({ id: 1000 - i }));
    return new Response(JSON.stringify(rows), {
      status: 200,
      headers: { "x-comment-has-more": "false", "x-comment-next-offset": "51" }
    });
  };
  const res = await listComments(7);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.items).toHaveLength(COMMENT_PAGE_SIZE);
  expect(res.data.hasMore).toBe(true);
});
test("paging a budget-limited thread walks it whole, without repeats", async () => {
  const server = Array.from({ length: 9 }, (_, i) => comment({ id: i + 1 }));
  globalThis.fetch = async (url) => {
    const params = new URL(String(url), "http://localhost").searchParams;
    const beforeId = params.get("before_id");
    const eligible = [...server].sort((a, b) => b.id - a.id).filter((row) => beforeId === null || row.id < Number(beforeId));
    const rows = eligible.slice(0, 2);
    return new Response(JSON.stringify(rows), {
      status: 200,
      headers: { "x-comment-has-more": String(eligible.length > rows.length) }
    });
  };
  let loaded = [];
  let hasMore = true;
  let requests = 0;
  while (hasMore && requests < 20) {
    const res = await listComments(7, (await olderCursor(loaded)));
    requests += 1;
    expect(res.ok).toBe(true);
    if (!res.ok)
      return;
    loaded = (await prependOlderComments(loaded, res.data.items));
    hasMore = res.data.hasMore;
  }
  expect(loaded.map((c) => c.id)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9]);
  expect(new Set(loaded.map((c) => c.id)).size).toBe(loaded.length);
  expect(requests).toBe(5);
});
test("pages back through a thread that is being written to, without duplicates", async () => {
  let total = 150;
  const calls = [];
  globalThis.fetch = async (url) => {
    calls.push(String(url));
    const params = new URL(String(url), "http://localhost").searchParams;
    const limit = Number(params.get("limit"));
    const beforeId = params.get("before_id");
    const highest = beforeId === null ? total : Number(beforeId) - 1;
    const rows = Array.from({ length: Math.max(0, Math.min(highest, limit)) }, (_, i) => comment({ id: highest - i, content: `comment ${highest - i}` }));
    total += 1;
    return new Response(JSON.stringify(rows), { status: 200 });
  };
  let loaded = [];
  for (let page = 0;page < 3; page += 1) {
    const res = await listComments(7, (await olderCursor(loaded)), 2);
    expect(res.ok).toBe(true);
    if (!res.ok)
      return;
    loaded = (await prependOlderComments(loaded, res.data.items));
  }
  expect(loaded.map((c) => c.id)).toEqual(Array.from({length:150},(_,i)=>i+1));
  expect(new Set(loaded.map((c) => c.id)).size).toBe(loaded.length);
});
test("dedupes defensively when two pages overlap", async () => {
  const onScreen = [comment({ id: 5 }), comment({ id: 6 })];
  const overlapping = [comment({ id: 3 }), comment({ id: 4 }), comment({ id: 5 })];
  const merged = (await prependOlderComments(onScreen, overlapping));
  expect(merged.map((c) => c.id)).toEqual([3, 4, 5, 6]);
  expect(new Set(merged.map((c) => c.id)).size).toBe(merged.length);
  expect((await prependOlderComments([comment({ id: 5, content: "edited" })], [comment({ id: 5 })]))[0].content).toBe("edited");
});
test("olderCursor names the oldest loaded comment, or nothing when empty", async () => {
  expect((await olderCursor([]))).toBeNull();
  expect((await olderCursor([comment({ id: 4 }), comment({ id: 9 })]))).toEqual({
    created_at: "2026-08-13 10:00:00",
    id: 4
  });
});
function windowFetcher(rows) {
  const calls = [];
  const newestFirst = [...rows].sort((a, b) => b.id - a.id);
  const fetchPage = async (before, size) => {
    calls.push({ before: before?.id ?? null, size });
    const eligible = before === null ? newestFirst : newestFirst.filter((row) => row.id < before.id);
    const transferred = eligible.slice(0, size + 1);
    const hasMore = transferred.length > size;
    const items = transferred.slice(0, size).slice().reverse();
    return {
      ok: true,
      data: {
        items,
        hasMore,
        nextCursor: items.length > 0 ? { created_at: items[0].created_at, id: items[0].id } : before
      }
    };
  };
  const cursors = () => calls.map((call) => call.before);
  const transferred = () => calls.reduce((total, call) => total + call.size + 1, 0);
  return { fetchPage, calls, cursors, transferred };
}
test("a refresh reconciles every loaded page, not just the newest", async () => {
  // The shipped protocol uses 50-row pages, so retain the original three-page shape with 150 rows.
  const server=Array.from({length:150},(_,i)=>comment({id:i+1,content:i+1===75?'edited elsewhere':`comment ${i+1}`})).filter(c=>c.id!==3);
  const {fetchPage,cursors}=windowFetcher(server),res=await loadCommentWindow(fetchPage,150);
  expect(res.ok).toBe(true);if(!res.ok)return;
  expect(res.data.items.map(c=>c.id)).toEqual(server.map(c=>c.id));expect(res.data.items.find(c=>c.id===75).content).toBe('edited elsewhere');expect(res.data.hasOlder).toBe(false);expect(cursors()).toEqual([null,101,51]);
});

test("a refresh never fetches more than the reader already loaded", async () => {
  const server=Array.from({length:500},(_,i)=>comment({id:i+1}));
  const {fetchPage,cursors}=windowFetcher(server),first=await loadCommentWindow(fetchPage,0);
  expect(first.ok).toBe(true);if(!first.ok)return;expect(first.data.items).toHaveLength(50);expect(first.data.hasOlder).toBe(true);expect(cursors()).toHaveLength(1);
  const {fetchPage:refetch,cursors:refreshCursors}=windowFetcher(server),refreshed=await loadCommentWindow(refetch,150);
  expect(refreshed.ok).toBe(true);if(!refreshed.ok)return;expect(refreshed.data.items).toHaveLength(150);expect(refreshed.data.items.map(c=>c.id)).toEqual(Array.from({length:150},(_,i)=>i+351));expect(refreshed.data.hasOlder).toBe(true);expect(refreshCursors()).toHaveLength(3);
});

test("a refresh keeps exactly the rows it was asked for, not whole pages", async () => {
  const server = Array.from({ length: 500 }, (_, i) => comment({ id: i + 1 }));
  const { fetchPage } = windowFetcher(server);
  const res = await loadCommentWindow(fetchPage, 51, 50);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.items).toHaveLength(51);
  expect(res.data.items.at(-1)?.id).toBe(500);
  expect(res.data.items[0].id).toBe(450);
  expect(res.data.hasOlder).toBe(true);
});
test("an exact page boundary needs no trimming", async () => {
  const server = Array.from({ length: 500 }, (_, i) => comment({ id: i + 1 }));
  const { fetchPage, cursors } = windowFetcher(server);
  const res = await loadCommentWindow(fetchPage, 100, 50);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.items).toHaveLength(100);
  expect(res.data.items[0].id).toBe(401);
  expect(res.data.items.at(-1)?.id).toBe(500);
  expect(res.data.hasOlder).toBe(true);
  expect(cursors()).toHaveLength(2);
});
test("a thread shorter than the window is returned whole", async () => {
  const server = Array.from({ length: 7 }, (_, i) => comment({ id: i + 1 }));
  const { fetchPage } = windowFetcher(server);
  const res = await loadCommentWindow(fetchPage, 51, 50);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.items.map((c) => c.id)).toEqual([1, 2, 3, 4, 5, 6, 7]);
  expect(res.data.hasOlder).toBe(false);
});
test("an automatic refresh is capped no matter how much history is loaded", async () => {
  const server = Array.from({ length: 5000 }, (_, i) => comment({ id: i + 1 }));
  const { fetchPage, calls, transferred } = windowFetcher(server);
  const res = await loadCommentWindow(fetchPage, 5000);
  expect(calls).toHaveLength(9);
  expect(transferred()).toBe(9 * (COMMENT_PAGE_SIZE + 1));
  expect(transferred()).toBeLessThanOrEqual(COMMENT_REFRESH_TRANSFER_LIMIT);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.items).toHaveLength(9 * COMMENT_PAGE_SIZE);
  expect(res.data.items.at(-1)?.id).toBe(5000);
  expect(res.data.items[0].id).toBe(5000 - 9 * COMMENT_PAGE_SIZE + 1);
  expect(res.data.hasOlder).toBe(true);
});
test("refresh bounds are enforced against any caller argument", async () => {
  const server = Array.from({ length: 5000 }, (_, i) => comment({ id: i + 1 }));
  const nonsense = [
    [Number.NaN, Number.NaN],
    [0, 0],
    [-10, -10],
    [Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY],
    [1e4, 1e4],
    [50.9, 10.9],
    [1, 1e4],
    [49, 10]
  ];
  for (const [size, budget] of nonsense) {
    const { fetchPage, calls, transferred } = windowFetcher(server);
    const res = await loadCommentWindow(fetchPage, Number.NaN, size, budget);
    expect(calls.length).toBeGreaterThanOrEqual(1);
    expect(calls.length).toBeLessThanOrEqual(COMMENT_REFRESH_PAGE_BUDGET);
    for (const call of calls) {
      expect(call.size).toBeGreaterThanOrEqual(1);
      expect(call.size).toBeLessThanOrEqual(COMMENT_PAGE_SIZE);
    }
    expect(transferred()).toBeLessThanOrEqual(COMMENT_REFRESH_TRANSFER_LIMIT);
    expect(res.ok).toBe(true);
    if (!res.ok)
      return;
    expect(res.data.items.length).toBeGreaterThanOrEqual(1);
    expect(res.data.items.length).toBeLessThanOrEqual(COMMENT_REFRESH_TRANSFER_LIMIT);
    expect(res.data.items.length).toBeLessThanOrEqual(calls.length * COMMENT_PAGE_SIZE);
  }
});
test("a fractional page size rounds down and still pages", async () => {
  // Topcoat has a fixed protocol rather than fractional caller size/budget arguments.
  // Preserve integer page sizes, two bounded pages, chronology, and remaining history.
  const server=Array.from({length:500},(_,i)=>comment({id:i+1}));
  const {fetchPage,calls,cursors}=windowFetcher(server),res=await loadCommentWindow(fetchPage,100);
  expect(calls.every(call=>Number.isInteger(call.size)&&call.size===50)).toBe(true);expect(cursors()).toHaveLength(2);expect(res.ok).toBe(true);if(!res.ok)return;
  expect(res.data.items).toHaveLength(100);expect(res.data.items[0].id).toBe(401);expect(res.data.items.at(-1).id).toBe(500);expect(res.data.hasOlder).toBe(true);
});

test("a capped refresh keeps the older rows the reader loaded by hand", async () => {
  const onScreen = {
    items: Array.from({ length: 800 }, (_, i) => comment({ id: i + 1 })),
    hasOlder: true
  };
  const refreshed = {
    items: Array.from({ length: 500 }, (_, i) => comment({ id: 301 + i, content: 301 + i === 400 ? "edited elsewhere" : "body" })),
    hasOlder: true
  };
  const merged = (await reconcileCommentWindow(onScreen, refreshed));
  expect(merged.items).toHaveLength(800);
  expect(merged.items[0].id).toBe(1);
  expect(merged.items.at(-1)?.id).toBe(800);
  expect(new Set(merged.items.map((c) => c.id)).size).toBe(800);
  expect(merged.items.map((c) => c.id)).toEqual(Array.from({ length: 800 }, (_, i) => i + 1));
  expect(merged.items.find((c) => c.id === 400)?.content).toBe("edited elsewhere");
  expect(merged.hasOlder).toBe(true);
});
test("reconciliation replaces the window when nothing older was preserved", async () => {
  const onScreen = { items: [comment({ id: 9 }), comment({ id: 10 })], hasOlder: true };
  const deeper = {
    items: [comment({ id: 8 }), comment({ id: 9 }), comment({ id: 10 })],
    hasOlder: false
  };
  expect((await reconcileCommentWindow(onScreen, deeper))).toEqual(deeper);
  const withoutNine = { items: [comment({ id: 8 }), comment({ id: 10 })], hasOlder: false };
  expect((await reconcileCommentWindow(onScreen, withoutNine)).items.map((c) => c.id)).toEqual([8, 10]);
  expect((await reconcileCommentWindow(onScreen, { items: [], hasOlder: false }))).toEqual({
    items: [],
    hasOlder: false
  });
});
test("reconciliation orders preserved rows by the same key the cursor uses", async () => {
  const onScreen = {
    items: [comment({ id: 5 }), comment({ id: 6 }), comment({ id: 7 })],
    hasOlder: true
  };
  const refreshed = { items: [comment({ id: 6 }), comment({ id: 7 })], hasOlder: true };
  const merged = (await reconcileCommentWindow(onScreen, refreshed));
  expect(merged.items.map((c) => c.id)).toEqual([5, 6, 7]);
  expect(new Set(merged.items.map((c) => c.id)).size).toBe(3);
});
test("a failed page aborts the refresh instead of showing a half window", async () => {
  const failure = async () => ({ ok: false, error: "offline", status: null });
  const res = await loadCommentWindow(failure, 30, 10);
  expect(res).toEqual({ ok: false, error: "offline", status: null });
});
test("an empty thread refreshes to an empty window without looping", async () => {
  const { fetchPage, cursors } = windowFetcher([]);
  const res = await loadCommentWindow(fetchPage, 50, 10);
  expect(res.ok).toBe(true);
  if (!res.ok)
    return;
  expect(res.data.items).toEqual([]);
  expect(res.data.hasOlder).toBe(false);
  expect(cursors()).toHaveLength(1);
});

const {rootFixture,collaboration}=require('./comments-adapter');
function heldRead(){let release;return {promise:new Promise(resolve=>{release=resolve;}),finish:value=>release(value)};}
test('only the newest comment-window operation may write',async()=>{
 const root=rootFixture(),first=heldRead(),second=heldRead();let calls=0;
 const api=collaboration(()=>++calls===1?first.promise:second.promise);
 const old=api.refreshComments(root,1),newer=api.refreshComments(root,1);
 second.finish({ok:true,data:[comment({id:5,content:'newer truth'})]});await newer;
 first.finish({ok:true,data:[comment({id:4,content:'stale truth'})]});await old;
 expect(root._comments.map(c=>c.content)).toEqual(['newer truth']);
 const routeRead=heldRead();const routeApi=collaboration(()=>routeRead.promise);const pending=routeApi.refreshComments(root,1);root._collabGeneration=2;routeRead.finish({ok:true,data:[comment({id:9})]});await pending;expect(root._comments.map(c=>c.content)).toEqual(['newer truth']);
});
test('a replacement window applies, re-reads, or stands down',async()=>{
 const root=rootFixture(),gate=heldRead();let calls=0;const api=collaboration(()=>{calls++;return calls===1?gate.promise:Promise.resolve({ok:true,data:[comment({id:9,content:'saved mutation'})]});});
 const pending=api.refreshComments(root,1);root._comments=[comment({id:9,content:'saved mutation'})];gate.finish({ok:true,data:[comment({id:9,content:'before mutation'})]});await pending;
 expect(root._comments[0].content).toBe('saved mutation');expect(calls).toBeGreaterThanOrEqual(2);expect(calls).toBeLessThanOrEqual(5);
});
function anchorFixture({items=[comment({id:900}),comment({id:901})],target='comment-812',older=true,fetcher}={}){
 const root=rootFixture(items);root._nextCommentCursor={created_at:items[0]?.created_at||'',id:items[0]?.id||1};root.querySelector('[data-comments-older]').hidden=!older;
 let calls=0;const location={hash:target?`#${target}`:'',search:''};
 const api=collaboration(async path=>{calls++;return fetcher?fetcher(path,calls):{ok:true,data:[],headers:new Headers()};},location);
 return {root,api,location,calls:()=>calls};
}
test('a deep link to an unloaded comment asks for the previous page',async()=>{
 const unloaded=anchorFixture();await unloaded.api.resolveCommentHash(unloaded.root,1);expect(unloaded.calls()).toBe(1);
 for(const config of [{target:'comment-900'},{older:false},{target:null},{target:'comment-abc'},{target:'comment-0'}]){const f=anchorFixture(config);await f.api.resolveCommentHash(f.root,1);expect(f.calls()).toBe(0);}
 const gate=heldRead(),busy=anchorFixture({fetcher:()=>gate.promise});const first=busy.api.resolveCommentHash(busy.root,1);const second=busy.api.resolveCommentHash(busy.root,1);expect(busy.calls()).toBe(1);gate.finish({ok:true,data:[]});await Promise.all([first,second]);
});
test('a failing anchor fetch is not retried until more comments arrive',async()=>{
 const f=anchorFixture({fetcher:()=>({ok:false,error:'offline'})});await require('node:assert/strict').rejects(f.api.resolveCommentHash(f.root,1),/offline/);await f.api.resolveCommentHash(f.root,1);expect(f.calls()).toBe(1);
 f.root._comments=[comment({id:850}),...f.root._comments];await f.api.resolveCommentHash(f.root,1).catch(()=>{});expect(f.calls()).toBe(2);
 f.location.hash='#comment-700';await f.api.resolveCommentHash(f.root,1).catch(()=>{});expect(f.calls()).toBe(3);
});
test('the automatic anchor walk stops after its page budget',async()=>{
 const f=anchorFixture({target:'comment-1',fetcher:(path,n)=>({ok:true,data:[comment({id:900-n})],headers:new Headers({'x-comment-has-more':'true'})})});await f.api.resolveCommentHash(f.root,1);expect(f.calls()).toBe(10);expect(f.root._comments).toHaveLength(12);expect(10*50).toBe(500);
});
test('navigating to another thread starts the anchor walk over',async()=>{
 const first=anchorFixture({target:'comment-1',fetcher:(path,n)=>({ok:true,data:[comment({id:900-n})],headers:new Headers({'x-comment-has-more':'true'})})});await first.api.resolveCommentHash(first.root,1);expect(first.calls()).toBe(10);
 const second=anchorFixture({target:'comment-1',fetcher:()=>({ok:true,data:[]})});second.root.dataset.issueId='10';await second.api.resolveCommentHash(second.root,1);expect(second.calls()).toBe(1);
});

test('a manual load older does not hand the automatic walk a fresh budget',async()=>{
 const fs=require('node:fs'),path=require('node:path');const {product,root:checkout}=require('./harness');
 const source=fs.readFileSync(path.join(checkout,'src/topcoat/issue_detail/collaboration/assets/collaboration.js'),'utf8');
 const begin=source.indexOf('function onClick(event)'),brace=source.indexOf('{',begin);let end=brace,depth=0,quote=null,escaped=false;
 for(;end<source.length;end++){const char=source[end];if(quote){if(escaped)escaped=false;else if(char==='\\')escaped=true;else if(char===quote)quote=null;continue;}if(['"',"'",'`'].includes(char)){quote=char;continue;}if(char==='{')depth++;if(char==='}'&&!--depth)break;}
 const handler=source.slice(begin,end+1),root=rootFixture([comment({id:900})]);root._nextCommentCursor={created_at:root._comments[0].created_at,id:900};root.querySelector('[data-comments-older]').focus=()=>{};
 let calls=0;const api=product('issue_detail/collaboration/assets/collaboration.js','LificTopcoatIssueCollaboration',code=>code.replace('globalThis.LificTopcoatIssueCollaboration={',`function olderHandler(root){const say=()=>{};async ${handler};return onClick;}\nglobalThis.LificTopcoatIssueCollaboration={olderHandler,refreshComments,`),{location:{hash:'#comment-1',search:''},lificSession:{state:{user:{id:3}},request:async()=>({ok:true,data:[comment({id:900-++calls})],headers:new Headers({'x-comment-has-more':'true'})})}});
 await api.resolveCommentHash(root,1);expect(calls).toBe(10);
 const older=root.querySelector('[data-comments-older]');await api.olderHandler(root)({target:{closest:selector=>selector==='[data-comments-older]'?older:null}});expect(calls).toBe(11);
 await api.resolveCommentHash(root,1);expect(calls).toBe(11);
});
