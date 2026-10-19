const { test } = require("node:test");
const assert = require("node:assert/strict");
const { startFixture } = require("../../../acceptance/server.js");
const HOSTILE_BODY = [
  "# Hostile body",
  "",
  "## script",
  '<img src=x onerror="window.__pwned = true">',
  "<script>window.__pwned = true;</script>",
  '<svg onload="window.__pwned = true"></svg>',
  "[click me](javascript:window.__pwned=true)",
  "<a href='javascript:window.__pwned=true'>single quoted js</a>",
  "<a href=javascript:window.__pwned=true>unquoted js</a>",
  "",
  "## media and embeds the regex never saw",
  '<video src="https://tracker.invalid/v.mp4" poster="https://tracker.invalid/p.png"></video>',
  '<audio><source src="https://tracker.invalid/a.mp3"></audio>',
  '<picture><source srcset="https://tracker.invalid/s.png"><img src="https://tracker.invalid/f.png"></picture>',
  '<input type="image" src="https://tracker.invalid/i.png">',
  '<object data="https://tracker.invalid/o.swf"></object>',
  '<embed src="https://tracker.invalid/e.swf">',
  '<iframe src="https://tracker.invalid/frame"></iframe>',
  '<svg><image href="https://tracker.invalid/svg.png"/></svg>',
  '<link rel="stylesheet" href="https://tracker.invalid/s.css">',
  "",
  "## css-shaped fetches",
  "<style>body { background: url(https://tracker.invalid/css.png); }</style>",
  '<p style="background-image:url(https://tracker.invalid/inline.png)">styled</p>',
  '<table><tr><td background="https://tracker.invalid/td.png">cell</td></tr></table>',
  '<img src="https://tracker.invalid/a.png" srcset="https://tracker.invalid/b.png 2x">',
  "",
  "## attribute syntax the regex could not parse",
  "<img src='https://tracker.invalid/single.png' alt='single quoted'>",
  "<img src=https://tracker.invalid/unquoted.png alt=unquoted>",
  "<a href='https://example.com/single-quoted'>single quoted link</a>",
  "",
  "## attachments we are not entitled to",
  "[a file we are not allowed to see](/api/attachments/424242)",
  "![also not allowed](/api/attachments/424243)",
  "",
  "## things that must survive",
  "[a normal link](https://example.com/docs)",
  "[another project's issue](/PRIV/issues/PRIV-1)",
  "[a prefix-shaped project](/DEMOX/issues/DEMOX-1)"
].join(`
`);
const FRIENDLY_BODY = [
  "# Friendly body",
  "",
  "Some **bold** and `inline code` and a [link](https://example.com/ok).",
  "",
  "- [x] a done task",
  "- [ ] a pending task",
  "",
  "| a | b |",
  "| - | - |",
  "| 1 | 2 |",
  "",
  "```",
  "a code block",
  "```",
  "",
  "> a quote",
  "",
  "A sibling issue: [see DEMO-1](/DEMO/issues/DEMO-1).",
  "",
  "And bare in prose, DEMO-1 should auto-link."
].join(`
`);
function isPrivateRequest(url, base) {
  if (!url.startsWith(base))
    return true;
  const path = url.slice(base.length);
  if (path.startsWith("/public/"))
    return false;
  if (path.startsWith("/assets/"))
    return false;
  if (path === "/" || path === "/favicon.ico" || path.endsWith(".png"))
    return false;
  return path.startsWith("/api") || path.startsWith("/oauth") || path.startsWith("/mcp");
}
async function watchedPage(context, base) {
  const page = await context.newPage();
  const consoleErrors = [];
  const pageErrors = [];
  const privateRequests = [];
  const offOrigin = [];
  page.on("console", (msg) => {
    if (msg.type() === "error")
      consoleErrors.push(msg.text());
  });
  page.on("pageerror", (err) => pageErrors.push(String(err)));
  const BODY_DRIVEN = new Set(["image", "xhr", "fetch", "script", "media"]);
  const requests = [];
  page.on("request", (req) => {
    const url = req.url();
    requests.push(url);
    if (!url.startsWith(base)) {
      if (url.startsWith("http") && BODY_DRIVEN.has(req.resourceType())) {
        offOrigin.push(url);
      }
      return;
    }
    if (isPrivateRequest(url, base))
      privateRequests.push(url);
  });
  return { page, consoleErrors, pageErrors, privateRequests, offOrigin, requests };
}
const BODY_SELECTOR = "article.tc-public__markdown";
test("main public-project browser acceptance", { skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH, timeout: 240000 }, async (t) => {
  const fixture = await startFixture();
  const browser = fixture.browser, base = fixture.origin + fixture.prefix;
  const failures = [], fail = (scenario, message) => failures.push(`${scenario}: ${message}`);
  const create = async (path, body) => {
    const r = await fixture.api(path, { method: "POST", body });
    assert.ok(r.ok, await r.clone().text());
    return r.json();
  };
  try {
    await fixture.api("/auth/me", { method: "PATCH", body: { display_name: "Public Operator" } });
    const demo = await create("/projects", { name: "Demo", identifier: "DEMO" });
    const priv = await create("/projects", { name: "Secret", identifier: "PRIV" });
    const issue = await create("/issues", { project_id: demo.id, title: "Public issue one", description: "A body **anyone** may read." });
    await create("/issues", { project_id: demo.id, title: "Public issue two" });
    await create("/issues", { project_id: demo.id, title: "Hostile issue", description: HOSTILE_BODY });
    await create("/issues", { project_id: demo.id, title: "Friendly issue", description: FRIENDLY_BODY });
    await create(`/issues/${issue.id}/comments`, { content: "A public comment body" });
    await create("/pages", { project_id: demo.id, title: "Public page", content: "Page body text" });
    await create("/issues", { project_id: priv.id, title: "Classified issue", description: "classified-marker-string" });
    assert.ok((await fixture.api(`/projects/${demo.id}`, { method: "PUT", body: { is_public: true } })).ok);
    await t.test("public list (desktop)", async () => {
      const scenario = "public list (desktop)";
      const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
      const w = await watchedPage(context, base);
      try {
        await w.page.goto(`${base}/public/DEMO`, { waitUntil: "load", timeout: 15000 });
        await w.page.waitForLoadState("networkidle", { timeout: 1e4 }).catch(() => {});
        const body = await w.page.locator("body").innerText() ?? "";
        if (w.page.url().includes("/login")) {
          fail(scenario, `redirected to the login page (${w.page.url()})`);
        }
        if (!w.page.url().includes("/public/DEMO/issues")) {
          fail(scenario, `/public/DEMO did not redirect to its issue list (${w.page.url()})`);
        }
        for (const expected of [
          "Public issue one",
          "Public issue two",
          "Issues",
          "Pages",
          "Read only",
          "Sign in"
        ]) {
          if (!body.includes(expected)) {
            fail(scenario, `expected visible text ${JSON.stringify(expected)} not found`);
          }
        }
        if (body.includes("Public read-only view")) {
          fail(scenario, "the retired 'Public read-only view' banner is still rendered");
        }
        if (body.includes("Something went wrong")) {
          fail(scenario, "rendered the error boundary fallback");
        }
        for (const url of w.privateRequests) {
          fail(scenario, `made a private-API request: ${url}`);
        }
        const token = await w.page.evaluate(() => localStorage.getItem("lific_token"));
        if (token) {
          fail(scenario, "a session token was minted for an anonymous visitor");
        }
        for (const err of w.consoleErrors)
          fail(scenario, `console error: ${err}`);
        for (const err of w.pageErrors)
          fail(scenario, `uncaught page error: ${err}`);
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
    await t.test("public issue detail (desktop)", async () => {
      const scenario = "public issue detail (desktop)";
      const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
      const w = await watchedPage(context, base);
      try {
        await w.page.goto(`${base}/public/DEMO/issues`, { waitUntil: "load", timeout: 15000 });
        await w.page.getByText("Public issue one").first().click();
        await w.page.waitForFunction(() => document.body.innerText.includes("A public comment body"), undefined, {
          timeout: 1e4
        }).catch(() => {});
        if (!w.page.url().includes("/public/DEMO/issues/DEMO-1")) {
          fail(scenario, `clicking the row landed on ${w.page.url()}`);
        }
        const body = await w.page.locator("body").innerText() ?? "";
        for (const expected of [
          "Public issue one",
          "A public comment body",
          "DEMO-1",
          "Public Operator",
          "Read only"
        ]) {
          if (!body.includes(expected)) {
            fail(scenario, `expected visible text ${JSON.stringify(expected)} not found`);
          }
        }
        if (await w.page.locator("textarea").count() > 0) {
          fail(scenario, "a text input was offered on a read-only public page");
        }
        if (await w.page.getByRole("button", { name: "Export" }).count() > 0) {
          fail(scenario, "the export button was offered on a public page");
        }
        for (const url of w.privateRequests) {
          fail(scenario, `made a private-API request: ${url}`);
        }
        for (const err of w.consoleErrors)
          fail(scenario, `console error: ${err}`);
        for (const err of w.pageErrors)
          fail(scenario, `uncaught page error: ${err}`);
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
    await t.test("public view (mobile)", async () => {
      const scenario = "public view (mobile)";
      const context = await browser.newContext({
        viewport: { width: 390, height: 844 },
        isMobile: true,
        hasTouch: true,
        deviceScaleFactor: 3
      });
      const w = await watchedPage(context, base);
      try {
        await w.page.goto(`${base}/public/DEMO/issues/DEMO-1`, {
          waitUntil: "load",
          timeout: 15000
        });
        await w.page.waitForLoadState("networkidle", { timeout: 1e4 }).catch(() => {});
        const body = await w.page.locator("body").innerText() ?? "";
        if (!body.includes("Public issue one")) {
          fail(scenario, "the issue title did not render at phone width");
        }
        const overflow = await w.page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
        if (overflow > 1) {
          fail(scenario, `the page scrolls horizontally by ${overflow}px at 390px wide`);
        }
        for (const err of w.consoleErrors)
          fail(scenario, `console error: ${err}`);
        for (const err of w.pageErrors)
          fail(scenario, `uncaught page error: ${err}`);
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
    await t.test("hostile markdown", async () => {
      const scenario = "hostile markdown";
      const context = await browser.newContext();
      const w = await watchedPage(context, base);
      try {
        await w.page.goto(`${base}/public/DEMO/issues/DEMO-3`, { waitUntil: "load", timeout: 15000 });
        await w.page.locator(BODY_SELECTOR).first().waitFor({ timeout: 15000 });
        await w.page.waitForLoadState("networkidle", { timeout: 1e4 }).catch(() => {});
        const pwned = await w.page.evaluate(() => window.__pwned === true);
        if (pwned)
          fail(scenario, "injected script executed");
        const markup = await w.page.evaluate((sel) => {
          const el = document.querySelector(sel);
          return el ? el.innerHTML : "";
        }, BODY_SELECTOR);
        if (markup === "")
          fail(scenario, "the issue body did not render at all");
        const dom = await w.page.evaluate((sel) => {
          const root = document.querySelector(sel);
          const tags = [];
          const attrs = [];
          for (const el of Array.from(root?.querySelectorAll("*") ?? [])) {
            if (el.closest(".attachment-view-host"))
              continue;
            const tag = el.tagName.toLowerCase();
            tags.push(tag);
            const appStyled = tag === "img" && el.hasAttribute("data-attachment-decorated");
            for (const a of Array.from(el.attributes)) {
              if (appStyled && a.name.toLowerCase() === "style")
                continue;
              attrs.push({ name: a.name.toLowerCase(), value: a.value, tag });
            }
          }
          return { tags, attrs };
        }, BODY_SELECTOR);
        for (const [needle, what] of [
          [/onerror/i, "an onerror handler"],
          [/onload/i, "an onload handler"],
          [/<script/i, "a <script> tag"]
        ]) {
          if (needle.test(markup))
            fail(scenario, `${what} survived sanitization`);
        }
        for (const attr of dom.attrs) {
          if (/^\s*javascript:/i.test(attr.value)) {
            fail(scenario, `a javascript: URL survived in <${attr.tag} ${attr.name}>`);
          }
          if (/^on/i.test(attr.name)) {
            fail(scenario, `an event handler survived: <${attr.tag} ${attr.name}>`);
          }
        }
        for (const tag of [
          "video",
          "audio",
          "source",
          "track",
          "picture",
          "iframe",
          "object",
          "embed",
          "style",
          "link",
          "form",
          "svg",
          "image",
          "use",
          "math"
        ]) {
          if (dom.tags.includes(tag)) {
            fail(scenario, `a <${tag}> element survived into the public body`);
          }
        }
        for (const attr of dom.attrs) {
          if (["style", "srcset", "poster", "background", "ping"].includes(attr.name)) {
            fail(scenario, `a ${attr.name}= attribute survived on <${attr.tag}>`);
          }
        }
        for (const attr of dom.attrs) {
          if (/tracker\.invalid/i.test(attr.value)) {
            fail(scenario, `a remote resource URL survived in <${attr.tag} ${attr.name}>`);
          }
        }
        if (!/example\.com\/docs/.test(markup)) {
          fail(scenario, "an ordinary outbound link was stripped; only unsafe ones should be");
        }
        if (!/example\.com\/single-quoted/.test(markup)) {
          fail(scenario, "a single-quoted outbound link was stripped");
        }
        if (!/Hostile body/.test(markup)) {
          fail(scenario, "the body's own prose was lost");
        }
        for (const url of w.offOrigin)
          fail(scenario, `fetched an off-origin resource: ${url}`);
        for (const url of w.privateRequests) {
          fail(scenario, `made a private-API request: ${url}`);
        }
        for (const url of w.requests) {
          if (url.startsWith(`${base}/api/attachments/`)) {
            fail(scenario, `probed a private attachment route: ${url}`);
          }
        }
        for (const err of w.pageErrors)
          fail(scenario, `uncaught page error: ${err}`);
        for (const err of w.consoleErrors) {
          if (err.includes("Failed to load resource"))
            continue;
          fail(scenario, `console error: ${err}`);
        }
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
    await t.test("safe markdown survives", async () => {
      const scenario = "safe markdown survives";
      const context = await browser.newContext();
      const w = await watchedPage(context, base);
      try {
        await w.page.goto(`${base}/public/DEMO/issues/DEMO-4`, { waitUntil: "load", timeout: 15000 });
        await w.page.locator(BODY_SELECTOR).first().waitFor({ timeout: 15000 });
        await w.page.waitForLoadState("networkidle", { timeout: 1e4 }).catch(() => {});
        const markup = await w.page.evaluate((sel) => {
          const el = document.querySelector(sel);
          return el ? el.innerHTML : "";
        }, BODY_SELECTOR);
        for (const [needle, what] of [
          [/<strong>/i, "bold"],
          [/<code>/i, "inline code"],
          [/<pre>/i, "a code block"],
          [/<table>/i, "a table"],
          [/<blockquote>/i, "a quote"],
          [/<ul>/i, "a list"],
          [/type="checkbox"/i, "a task-list checkbox"],
          [/example\.com\/ok/, "an outbound link"]
        ]) {
          if (!needle.test(markup))
            fail(scenario, `${what} was stripped`);
        }
        const autoLink = w.page.locator(`${BODY_SELECTOR} a[href*="/public/DEMO/issues/"]`).first();
        if (await autoLink.count() === 0) {
          fail(scenario, "a bare identifier in prose was not auto-linked");
        } else {
          const href = await autoLink.getAttribute("href") ?? "";
          if (!href.startsWith(`${fixture.prefix}/public/DEMO/issues/`)) {
            fail(scenario, `an auto-linked identifier escaped the public view: ${href}`);
          }
          await autoLink.click();
          await w.page.waitForFunction(() => document.body.innerText.includes("A public comment body"), undefined, {
            timeout: 1e4
          }).catch(() => fail(scenario, "the auto-linked identifier did not navigate"));
          if (!w.page.url().includes("/public/DEMO/issues/DEMO-1")) {
            fail(scenario, `navigation left the public view: ${w.page.url()}`);
          }
        }
        for (const url of w.privateRequests)
          fail(scenario, `private-API request: ${url}`);
        for (const err of w.pageErrors)
          fail(scenario, `uncaught page error: ${err}`);
        for (const err of w.consoleErrors)
          fail(scenario, `console error: ${err}`);
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
    await t.test("public pages", async () => {
      const scenario = "public pages";
      const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
      const w = await watchedPage(context, base);
      try {
        await w.page.goto(`${base}/public/DEMO/pages`, { waitUntil: "load", timeout: 15000 });
        await w.page.waitForFunction(() => document.body.innerText.includes("Public page"), undefined, {
          timeout: 15000
        }).catch(() => fail(scenario, "the page tree did not list the published page"));
        await w.page.getByText("Public page").first().click();
        await w.page.waitForFunction(() => document.body.innerText.includes("Page body text"), undefined, {
          timeout: 1e4
        }).catch(() => fail(scenario, "the page body never rendered"));
        if (!/\/public\/DEMO\/pages\/\d+/.test(w.page.url())) {
          fail(scenario, `opening a page landed on ${w.page.url()}`);
        }
        const body = await w.page.locator("body").innerText() ?? "";
        for (const expected of ["Public page", "Page body text", "Read only"]) {
          if (!body.includes(expected)) {
            fail(scenario, `expected visible text ${JSON.stringify(expected)} not found`);
          }
        }
        for (const url of w.privateRequests)
          fail(scenario, `private-API request: ${url}`);
        for (const err of w.consoleErrors)
          fail(scenario, `console error: ${err}`);
        for (const err of w.pageErrors)
          fail(scenario, `uncaught page error: ${err}`);
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
    await t.test("whole issue list arrives", async () => {
      const scenario = "whole issue list arrives";
      const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
      const w = await watchedPage(context, base);
      try {
        for (let i = 1;i <= 120; i++)
          await create("/issues", { project_id: demo.id, title: `Bulk issue ${i}`, description: "", status: "backlog" });
        await w.page.goto(`${base}/public/DEMO/issues`, { waitUntil: "load", timeout: 15000 });
        for (const title of ["Bulk issue 1", "Bulk issue 120"]) {
          const found = await w.page.locator("[data-public-issue]").filter({ hasText: new RegExp(`${title}$`) }).first().waitFor({ timeout: 15000 }).then(() => true).catch(() => false);
          if (!found)
            fail(scenario, `${JSON.stringify(title)} never rendered`);
        }
        const rows = await w.page.evaluate(() => Array.from(document.querySelectorAll("[data-public-issue]")).filter((el) => /^DEMO-\d+$/.test(el.getAttribute("data-public-issue") ?? "")).length);
        if (rows < 100) {
          fail(scenario, `only ${rows} issue rows rendered; expected at least 100`);
        }
        const body = await w.page.locator("body").innerText() ?? "";
        if (!body.includes("Public issue one")) {
          fail(scenario, "the originally seeded issues are missing from the list");
        }
        for (const url of w.privateRequests)
          fail(scenario, `private-API request: ${url}`);
        for (const err of w.consoleErrors)
          fail(scenario, `console error: ${err}`);
        for (const err of w.pageErrors)
          fail(scenario, `uncaught page error: ${err}`);
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
    await t.test("route race", async () => {
      const scenario = "route race";
      const context = await browser.newContext();
      const w = await watchedPage(context, base);
      try {
        await w.page.goto(`${base}/public/DEMO/issues`, { waitUntil: "load", timeout: 15000 });
        await w.page.waitForLoadState("networkidle", { timeout: 1e4 }).catch(() => {});
        const traversals = [];
        for (let i = 0;i < 6; i++) {
          for (const route of ["/public/DEMO/issues/DEMO-3", "/public/DEMO/issues"])
            traversals.push(w.page.goto(`${base}${route}`).catch((error) => {
              if (!/ERR_ABORTED|interrupted by another navigation/.test(String(error)))
                throw error;
            }));
        }
        await Promise.all(traversals);
        await w.page.goto(`${base}/public/DEMO/issues/DEMO-1`);
        const settled = await w.page.waitForFunction(() => document.body.innerText.includes("A public comment body"), undefined, {
          timeout: 1e4
        }).then(() => true).catch(() => false);
        if (!settled)
          fail(scenario, "the final route never rendered");
        const body = await w.page.locator("body").innerText() ?? "";
        if (body.includes("Hostile body")) {
          fail(scenario, "a stale route's content overwrote the current one");
        }
        for (const err of w.pageErrors)
          fail(scenario, `uncaught page error: ${err}`);
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
    await t.test("private project stays private", async () => {
      const scenario = "private project stays private";
      const context = await browser.newContext();
      const w = await watchedPage(context, base);
      try {
        await w.page.goto(`${base}/public/PRIV/issues`, { waitUntil: "load", timeout: 15000 });
        await w.page.waitForLoadState("networkidle", { timeout: 1e4 }).catch(() => {});
        const body = await w.page.locator("body").innerText() ?? "";
        if (body.includes("classified-marker-string") || body.includes("Classified issue")) {
          fail(scenario, "an unpublished project's content rendered");
        }
        if (!body.includes("This project isn't public")) {
          fail(scenario, `expected the "isn't public" message, got: ${body.slice(0, 200)}`);
        }
        for (const url of w.privateRequests)
          fail(scenario, `private-API request: ${url}`);
        const [priv, nope] = await w.page.evaluate(async (b) => {
          const read = async (p) => {
            const r = await fetch(`${b}/public/api/projects/${p}/index`, { credentials: "omit" });
            return { status: r.status, body: await r.text() };
          };
          return [await read("PRIV"), await read("NOPE")];
        }, base);
        if (priv.status !== 404) {
          fail(scenario, `an unpublished project's index answered ${priv.status}, not 404`);
        }
        if (priv.status !== nope.status || priv.body !== nope.body) {
          fail(scenario, `a private project is distinguishable from a nonexistent one: ` + `${priv.status} ${JSON.stringify(priv.body)} vs ${nope.status} ${JSON.stringify(nope.body)}`);
        }
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
    await t.test("unpublish closes the view", async () => {
      const scenario = "unpublish closes the view";
      const context = await browser.newContext();
      const w = await watchedPage(context, base);
      try {
        await w.page.goto(`${base}/public/DEMO/issues`, { waitUntil: "load", timeout: 15000 });
        await w.page.waitForLoadState("networkidle", { timeout: 1e4 }).catch(() => {});
        if (!(await w.page.locator("body").innerText()).includes("Public issue one")) {
          fail(scenario, "the list did not render before unpublishing");
        }
        assert.ok((await fixture.api(`/projects/${demo.id}`, { method: "PUT", body: { is_public: false } })).ok);
        const status = await w.page.evaluate(async (b) => {
          const r = await fetch(`${b}/public/api/projects/DEMO/index`, {
            credentials: "omit"
          });
          return r.status;
        }, base);
        if (status !== 404) {
          fail(scenario, `the index endpoint answered ${status} after unpublishing, not 404`);
        }
        await w.page.reload({ waitUntil: "load", timeout: 15000 });
        await w.page.waitForLoadState("networkidle", { timeout: 1e4 }).catch(() => {});
        const body = await w.page.locator("body").innerText() ?? "";
        if (body.includes("Public issue one")) {
          fail(scenario, "the issue still rendered after publication was turned off");
        }
        if (!body.includes("This project isn't public")) {
          fail(scenario, `expected the "isn't public" message, got: ${body.slice(0, 200)}`);
        }
        assert.ok((await fixture.api(`/projects/${demo.id}`, { method: "PUT", body: { is_public: true } })).ok);
        await w.page.reload({ waitUntil: "load", timeout: 15000 });
        await w.page.waitForFunction(() => document.body.innerText.includes("Public issue one"), undefined, {
          timeout: 1e4
        }).catch(() => fail(scenario, "republishing did not restore the original address"));
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
    await t.test("signed-in app is unaffected", async () => {
      const scenario = "signed-in app is unaffected";
      const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
      const page = await context.newPage();
      try {
        await page.goto(`${base}/DEMO/issues`, { waitUntil: "load", timeout: 15000 });
        await page.waitForLoadState("networkidle", { timeout: 1e4 }).catch(() => {});
        if (!page.url().includes("/login")) {
          fail(scenario, `an anonymous visitor reached ${page.url()} instead of the login page`);
        }
        await page.goto(`${base}/login`, { waitUntil: "load", timeout: 15000 });
        await page.fill("input[name=identity]", fixture.credentials.identity);
        await page.fill("input[name=password]", fixture.credentials.password);
        await page.click("button[type=submit]");
        await page.waitForURL(`${base}/`, { timeout: 15000 }).catch(() => {});
        if (page.url().includes("/login")) {
          const text = await page.locator("body").innerText().catch(() => "");
          fail(scenario, `login did not land on the app: ${text.slice(0, 200)}`);
        }
        await page.evaluate(() => {
          window.location.hash = "#/DEMO/issues";
        });
        await page.waitForFunction(() => document.body.innerText.includes("Public issue one"), undefined, {
          timeout: 15000
        }).catch(() => fail(scenario, "the signed-in issue list did not render"));
        if (await page.getByRole("button", { name: "Create issue", exact: true }).count() === 0) {
          fail(scenario, "the signed-in list lost its create control");
        }
        const publicPage = await context.newPage();
        let watching = true;
        const seen = [];
        publicPage.on("request", (req) => {
          if (watching)
            seen.push({ url: req.url(), auth: req.headers()["authorization"] });
        });
        await publicPage.goto(`${base}/public/DEMO/issues`, {
          waitUntil: "load",
          timeout: 15000
        });
        await publicPage.waitForFunction(() => document.body.innerText.includes("Public issue one"), undefined, {
          timeout: 15000
        }).catch(() => fail(scenario, "the public route did not render for a signed-in reader"));
        await publicPage.waitForLoadState("networkidle", { timeout: 1e4 }).catch(() => {});
        watching = false;
        if (await publicPage.evaluate(() => localStorage.getItem("lific_token")) === null) {
          fail(scenario, "the context under test was not actually signed in");
        }
        for (const req of seen) {
          if (req.url.startsWith(`${base}/public/api`) && req.auth !== undefined) {
            fail(scenario, `a public request carried a credential: ${req.url}`);
          }
          if (req.url.startsWith(base) && isPrivateRequest(req.url, base)) {
            fail(scenario, `the public view reached the private API: ${req.url}`);
          }
        }
        if (await publicPage.getByRole("button", { name: "Create issue", exact: true }).count() > 0) {
          fail(scenario, "the public view offered a create control to a signed-in reader");
        }
        await publicPage.evaluate(() => {
          window.location.hash = "#/DEMO/issues";
        });
        const restored = await publicPage.getByRole("button", { name: "Create issue", exact: true }).first().waitFor({ timeout: 15000 }).then(() => true).catch(() => false);
        if (!restored) {
          fail(scenario, "the create control did not come back after leaving the public view");
        }
      } catch (e) {
        fail(scenario, String(e));
      } finally {
        await context.close();
      }
      assert.deepEqual(failures.filter((f) => f.startsWith(scenario + ":")), []);
    });
  } finally {
    await fixture.close();
  }
});
