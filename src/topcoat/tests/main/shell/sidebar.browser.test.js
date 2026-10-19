const { test: nodeTest } = require("node:test");
const assert = require("node:assert/strict");
const { resolve } = require("node:path");
const { tmpdir } = require("node:os");
const { startFixture } = require("../../../acceptance/server.js");
const { accents, appearance, sidebarContrast, checkSidebarContrast, desktopScreenshots, mobileVisualChecks } = require("./sidebar-visual.js");
const { withNativeLinkDiagnostics } = require("./native-links.js");
const stamp = "2026-09-10T12:00:00Z";
const longName = "The extraordinarily long project name that must not hide its actions";
const longTitle = "Investigate the extraordinarily long sidebar regression title with enough detail to wrap across several lines";
const destinations = ["Overview", "Issues", "Board", "Graph", "Modules", "Pages", "Files", "Plans", "Activity", "Insights"];

class API {
  user = { id: 1, username: "sidebar", display_name: "Sidebar Operator", email: "sidebar@example.test", is_admin: false };
  projects = ["One", "Two", longName, "Four"].map((name, i) => ({
    id: i + 1,
    name,
    identifier: ["ONE", "TWO", "LONG", "FOUR"][i],
    emoji: "",
    description: "",
    sort_order: i,
    created_at: stamp,
    updated_at: stamp,
    is_public: false
  }));
  groups = [
    { id: 11, user_id: 1, name: "Work", project_ids: [1, 2], sort_order: 0, created_at: stamp, updated_at: stamp },
    { id: 12, user_id: 1, name: "Personal", project_ids: [3], sort_order: 1, created_at: stamp, updated_at: stamp }
  ];
  calls = [];
  unexpected = [];
  expectedConsole = new Set;
  failingGets = new Set;
  gates = [];
  pending = 0;
  hold(method, path, status = 200) {
    let release;
    const promise = new Promise((r) => release = r);
    const gate = { method, path, status, release, promise };
    this.gates.push(gate);
    return gate;
  }
  fail(method, path) {
    const gate = this.hold(method, path, 409);
    gate.release();
  }
  async attach(context) {
    await context.route("**/api/**", async (route) => {
      const req = route.request();
      const url = new URL(req.url());
      const path = url.pathname.slice(url.pathname.indexOf("/api/") + 4);
      const method = req.method();
      const body = req.postData() ? req.postDataJSON() : null;
      this.calls.push({ method, path, query: url.search, body });
      this.pending++;
      try {
        if (this.calls.length > 250) {
          if (!this.unexpected.includes("Runaway API requests"))
            this.unexpected.push("Runaway API requests");
          return await route.abort();
        }
        const index = this.gates.findIndex((g) => g.method === method && g.path === path);
        if (index >= 0) {
          const gate = this.gates.splice(index, 1)[0];
          await gate.promise;
          if (gate.status !== 200) {
            this.expectedConsole.add(req.url());
            return await route.fulfill({ status: gate.status, json: { error: "Fixture rejected this save." } });
          }
        }
        if (method === "GET" && this.failingGets.has(path)) {
          this.expectedConsole.add(req.url());
          return await route.fulfill({ status: 409, json: { error: "Fixture read temporarily unavailable." } });
        }
        let data;
        if (path === "/auth/me" && ["GET", "PATCH"].includes(method)) {
          if (method === "PATCH")
            Object.assign(this.user, body);
          data = this.user;
        } else if (path === "/projects" && method === "GET")
          data = this.projects;
        else if (path === "/project-groups" && method === "GET")
          data = this.groups;
        else if (path === "/projects/reorder" && method === "PUT") {
          assert.deepEqual([...body.ids].sort(), this.projects.map((p) => p.id).sort());
          this.projects = body.ids.map((id, sort_order) => ({ ...this.projects.find((p) => p.id === id), sort_order }));
          data = this.projects;
        } else if (path === "/project-groups/reorder" && method === "PUT") {
          assert.deepEqual([...body.ids].sort(), this.groups.map((g) => g.id).sort());
          this.groups = body.ids.map((id, sort_order) => ({ ...this.groups.find((g) => g.id === id), sort_order }));
          data = this.groups;
        } else if (path === "/project-groups" && method === "POST") {
          const group = { id: 13, user_id: this.user.id, name: body.name, project_ids: [], sort_order: this.groups.length, created_at: stamp, updated_at: stamp };
          this.groups.push(group);
          data = group;
        } else if (/^\/project-groups\/\d+$/.test(path) && method === "PATCH") {
          const group = this.groups.find((g) => g.id === Number(path.split("/").at(-1)));
          group.name = body.name;
          data = group;
        } else if (/^\/projects\/\d+\/my-role$/.test(path) && method === "GET") {
          data = { role: "lead", enforced: true, is_admin: false };
        } else if (["/issues", "/modules", "/pages", "/plans"].includes(path) && method === "GET") {
          const projectId = Number(url.searchParams.get("project_id"));
          const project = this.projects.find((p) => p.id === projectId);
          data = !project || path === "/pages" && url.searchParams.get("status") !== "active" ? [] : [{
            id: projectId * 10,
            project_id: projectId,
            identifier: `${project.identifier}-1`,
            title: projectId === 1 ? longTitle : "Other project item",
            name: "Recent module",
            status: "active",
            updated_at: stamp,
            created_at: stamp
          }];
        } else if (path === "/instance" && method === "GET")
          data = { web_auto_login: false, auth_mode: "passwords" };
        else if (["/auth/bots", "/auth/keys", "/folders", "/labels"].includes(path) && method === "GET")
          data = [];
        else if (/^\/projects\/\d+\/(index|changes)$/.test(path)) {
          const id = Number(path.split("/")[2]), project = this.projects.find((p) => p.id === id);
          const issues = project ? [{ id: id * 10, kind: "issue", seq: 1, deleted: false, project_id: id, identifier: `${project.identifier}-1`, title: id === 1 ? longTitle : "Other project item", status: "active", updated_at: stamp, created_at: stamp }] : [];
          data = path.endsWith("/index") ? { cursor: 1, issues, pages: [] } : { cursor: 1, changes: [], has_more: false };
        } else if (/^\/issues\/resolve\//.test(path)) {
          const identifier=path.split('/').at(-1), project=this.projects.find(project=>identifier.startsWith(project.identifier+'-'));
          data={id:project.id*10,project_id:project.id,identifier,title:longTitle,status:'todo',priority:'none',description:'',created_at:stamp,updated_at:stamp};
        } else if (/^\/(issues|modules|pages|plans)\/\d+$/.test(path)) {
          const id=Number(path.split('/').at(-1)),project=this.projects.find(project=>project.id===Math.floor(id/10));
          data={id,project_id:project?.id??1,identifier:`${project?.identifier??'ONE'}-1`,title:longTitle,name:'Recent module',status:'active',priority:'none',description:'',content:'',created_at:stamp,updated_at:stamp};
        } else if (/^\/issues\/\d+\/(comments|links|resources|events)$/.test(path)) data=[];
        else if (/^\/projects\/\d+\/views$/.test(path))
          data = [];
        else if (/^\/projects\/\d+\/(mention-candidates|members|bindings)$/.test(path)||['/users','/project-archives','/attachments'].includes(path)) data=[];
        else if (/^\/projects\/\d+\/issue-counts$/.test(path)) data={todo:0,active:0,done:0,cancelled:0,total:0};
        else if (/^\/issues\/\d+\/activity$/.test(path)) data={items:[],has_more:false};
        else if (/^\/projects\/\d+\/activity$/.test(path))
          data = { items: [], has_more: false };
        else {
          this.unexpected.push(`${method} ${path}${url.search}`);
          return await route.fulfill({ status: 501, json: { error: "Unmocked API request" } });
        }
        await route.fulfill({ json: data });
      } catch (error) {
        if (!context.pages().every((p) => p.isClosed()))
          this.unexpected.push(String(error));
      } finally {
        this.pending--;
      }
    });
  }
}
nodeTest("main sidebar browser cases", { skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH, timeout: 240000 }, async (t) => {
  const fixture = await startFixture({ seed: false });
  const browser = fixture.browser, base = fixture.origin + fixture.prefix;
  const shotDir = fixture.scratch, allGates = [], sessions = [];
  for (const [i, name] of ["One", "Two", longName, "Four"].entries()) {
    const r = await fixture.api("/projects", { method: "POST", body: { name, identifier: ["ONE", "TWO", "LONG", "FOUR"][i] } });
    assert.ok(r.ok);
    const project = await r.json();
    await fixture.api("/issues", { method: "POST", body: { project_id: project.id, title: i === 0 ? longTitle : "Other project item" } });
  }
  for(const identifier of ['FIVE',...Array.from({length:45},(_,i)=>`P${i+1}`)]){const r=await fixture.api('/projects',{method:'POST',body:{name:identifier,identifier}});assert.ok(r.ok);const p=await r.json();await fixture.api('/issues',{method:'POST',body:{project_id:p.id,title:longTitle}});}
  try {
    let held = function(api, method, path, status = 200) {
      const gate = api.hold(method, path, status);
      allGates.push(gate);
      return gate;
    };
    async function test(name, run) {
      await t.test(name, async () => {
        const start = sessions.length;
        try {
          await run();
          for (const s of sessions.slice(start)) {
            await settle(s);
            assert.deepEqual(s.errors, [], "Browser runtime/console errors");
          }
        } finally {
          for (const gate of allGates.splice(0))
            gate.release();
          for (const s of sessions.slice(start))
            await s.context.close();
        }
      });
    }
    async function session(options = {}) {
      const api = options.api ?? new API;
      const context = await browser.newContext({ viewport: options.mobile ? { width: 390, height: 844 } : { width: 1280, height: 1000 }, reducedMotion: "reduce", colorScheme: "light" });
      await api.attach(context);
      await context.addCookies([{ name: "lific_token", value: fixture.token, url: fixture.origin, httpOnly: true }]);
      await context.addInitScript((token) => {
        if(location.protocol!=="http:")return;
        localStorage.setItem("lific_token", token);
        window.fixture = { navigate: (path) => location.assign("/app" + path), refresh: () => window.lificTopcoatShell?.refresh(), mobile: () => document.querySelector("[data-mobile-navigation]")?.dataset.open === "true", palette: () => document.querySelector("[data-topcoat-palette]")?.open === true };
      }, fixture.token);
      if (options.observeReveal)
        await context.addInitScript(() => {
          window.sidebarScrollCalls = [];
          const original = Element.prototype.scrollIntoView;
          Element.prototype.scrollIntoView = function(options) {
            const id = this.getAttribute("data-project-id");
            if (id)
              window.sidebarScrollCalls.push({ id, options });
            return original.call(this, options);
          };
        });
      if (options.collapsed)
        await context.addInitScript((ids) => {
          if (location.protocol !== "http:")
            return;
          if (localStorage.getItem("lific:project-tree:disclosures") === null)
            localStorage.setItem("lific:project-tree:disclosures", JSON.stringify([11,12].filter(id=>!ids.includes(id)).map(String)));
        }, options.collapsed);
      const errors = [];
      context.on("page", (page) => {
        page.setDefaultTimeout(5000);
        page.on("pageerror", (error) => errors.push(error.message));
        page.on("console", (message) => {
          if (message.type() !== "error")
            return;
          if (api.expectedConsole.has(message.location().url) && /^Failed to load resource: the server responded with a status of 409\b/.test(message.text()))
            return;
          errors.push(message.text());
        });
      });
      const page = await context.newPage();
      sessions.push({ page, api, aside: page.locator("aside.tc-shell__desktop"), context, errors });
      await page.goto(base.replace(/\/$/, "") + (options.route ?? "/"));
      await page.locator('aside a[href$="/settings"]').waitFor({ state: "attached" });
      await healthy(page, api);
      return { page, api, aside: page.locator("aside.tc-shell__desktop"), context };
    }
    async function healthy(page, api) {
      assert.equal(await page.locator("#boundary-error").count(), 0, "Svelte boundary rendered");
      assert.deepEqual(api.unexpected, [], "Unexpected or runaway HTTP requests");
    }
    async function settle(s) {
      await s.page.waitForFunction(() => !!window.fixture);
      await s.page.waitForTimeout(150);
      const calls = s.api.calls.length;
      await s.page.waitForTimeout(150);
      assert.equal(s.api.calls.length, calls, "HTTP requests continued after the UI settled");
      assert.equal(s.api.pending, 0, "Requests never completed");
      await healthy(s.page, s.api);
    }
    async function routeTo(page, path) {
      await page.evaluate((path) => window.fixture.navigate(path), path);
      await page.waitForURL((url) => url.pathname === "/app" + path);
    }
    async function attr(el, name, value) {
      await el.evaluate((el, args) => new Promise((resolve, reject) => {
        const end = performance.now() + 4000;
        function check() {
          if (el.getAttribute(args.name) === args.value)
            resolve();
          else if (performance.now() > end)
            reject(new Error(`${args.name}: expected ${args.value}, got ${el.getAttribute(args.name)}`));
          else
            requestAnimationFrame(check);
        }
        check();
      }), { name, value });
    }
    const menuItem = (page, name) => page.getByRole("menuitem", { name, exact: true });
    async function action(s, name, choice) {
      const trigger = s.aside.getByRole("button", { name, exact: true });
      await trigger.focus();
      await trigger.click();
      await menuItem(s.page, choice).click();
    }
    async function called(api, count, method, path) {
      for (let i = 0;i < 100; i++) {
        const calls = api.calls.filter((c) => c.method === method && c.path === path);
        if (calls.length >= count)
          return calls.at(-1);
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      throw new Error(`Missing request ${method} ${path} (#${count})`);
    }
    await test("separate disclosure and Overview links", async () => {
      const s = await session();
      const { page, aside } = s;
      await aside.getByRole("button", { name: "Expand One", exact: true }).click();
      assert.equal(new URL(page.url()).pathname.replace(/^\/app/, ""), "/");
      await aside.getByRole("button", { name: "Expand Two", exact: true }).click();
      await aside.locator('a[aria-label="Open One"]').click();
      await page.waitForURL("**/app/ONE/overview");
      await attr(aside.locator('a[aria-label="Open One"]'), "aria-current", "page");
      await attr(aside.locator(".tc-shell__navigation"), "hidden", null);
      await attr(aside.locator("#project-nav-2"), "hidden", null);
      await aside.getByRole("button", { name: "Collapse One", exact: true }).click();
      assert.equal(new URL(page.url()).pathname.replace(/^\/app/, ""), "/ONE/overview");
      await attr(aside.locator(".tc-shell__navigation"), "hidden", "");
      await attr(aside.locator("#project-nav-2"), "hidden", null);
    });
    await test("sidebar selection and readable accents in both themes", async () => {
      const s = await session({ route: "/ONE/issues/ONE-1" });
      await s.aside.locator("[data-recents-toggle]").click();
      await checkSidebarContrast(s.page, shotDir);
    });
    await test("selected account metadata remains readable across accents", async () => {
      const s = await session({ route: "/settings" });
      const measurements = [];
      for (const theme of ["light", "dark"])
        for (const accent of accents) {
          await appearance(s.page, theme, accent);
          const result = { theme, accent, ...await sidebarContrast(s.page) };
          measurements.push(result);
          assert.ok(result.minimumText >= 4.5, JSON.stringify(result));
        }
      await require("node:fs/promises").writeFile(resolve(shotDir, "sidebar-account-contrast.json"), JSON.stringify(measurements, null, 2));
    });
    await test("desktop visual matrix", async () => {
      const api = new API;
      api.projects[0].emoji = "\uD83E\uDD8E";
      api.projects[1].emoji = "lucide:Terminal";
      const s = await session({ api, route: "/ONE/issues" });
      await desktopScreenshots(s.page, shotDir);
    });
    await test("mobile visual grammar contrast and touch targets", async () => {
      const api = new API;
      api.projects[0].emoji = "\uD83E\uDD8E";
      api.projects[1].emoji = "lucide:Terminal";
      const s = await session({ api, mobile: true, route: "/ONE/issues" });
      await mobileVisualChecks(s.page, shotDir, true);
    });
    await test("direct link reveal and deliberate collapse survives revalidation", async () => {
      const s = await session({ route: "/ONE/issues/ONE-1", collapsed: [11, 12] });
      const { page, aside } = s;
      await attr(aside.locator('[data-group-id="11"] .tc-projects__project-list'), "hidden", null);
      await attr(aside.locator('[data-group-id="12"] .tc-projects__project-list'), "hidden", "");
      await attr(aside.locator(".tc-shell__navigation"), "hidden", null);
      await aside.getByRole("button", { name: "Collapse One", exact: true }).click();
      await aside.locator('[data-group-id="11"] [data-action="disclose"]').click();
      await routeTo(page, "/ONE/pages");
      await page.evaluate(() => window.fixture.refresh());
      await settle(s);
      await attr(aside.locator('[data-group-id="11"] .tc-projects__project-list'), "hidden", "");
      await attr(aside.locator(".tc-shell__navigation"), "hidden", "");
      assert.ok(await aside.getByRole("button", { name: "Current: One", exact: true }).isVisible());
      assert.ok((await page.evaluate(() => [11,12].filter(id=>!JSON.parse(localStorage.getItem("lific:project-tree:disclosures")??'["11","12"]').includes(String(id))))).includes(11));
      await page.reload();
      await attr(aside.locator('[data-group-id="11"] .tc-projects__project-list'), "hidden", null);
      await attr(aside.locator(".tc-shell__navigation"), "hidden", null);
    });
    await test("failed initial groups recover and reveal the active project exactly once", async () => {
      const api = new API;
      api.failingGets.add("/project-groups");
      const s = await session({ api, route: "/ONE/issues/ONE-1", collapsed: [11, 12], observeReveal: true });
      const { page, aside } = s;
      const scrollCalls = () => page.evaluate(() => window.sidebarScrollCalls);
      await settle(s);
      assert.ok(await aside.locator('a[aria-label="Open One"]').isVisible(), "Projects loaded despite failed groups");
      assert.equal(await aside.locator('[data-group-id="11"] .tc-projects__project-list').count(), 0);
      await attr(aside.locator(".tc-shell__navigation"), "hidden", "");
      assert.deepEqual(await scrollCalls(), [], "Failed membership read must not consume route reveal");
      assert.deepEqual(await page.evaluate(() => [11,12].filter(id=>!JSON.parse(localStorage.getItem("lific:project-tree:disclosures")??'["11","12"]').includes(String(id)))), [11, 12]);
      api.failingGets.delete("/project-groups");
      const gate = held(api, "GET", "/project-groups");
      const count = api.calls.filter((c) => c.path === "/project-groups").length;
      const recovering = page.evaluate(() => window.fixture.refresh());
      await called(api, count + 1, "GET", "/project-groups");
      assert.deepEqual(await scrollCalls(), [], "Wait for recovered membership, not just project success");
      gate.release();
      await recovering;
      await attr(aside.locator('[data-group-id="11"] .tc-projects__project-list'), "hidden", null);
      await attr(aside.locator(".tc-shell__navigation"), "hidden", null);
      await attr(aside.locator('[data-group-id="12"] .tc-projects__project-list'), "hidden", "");
      await settle(s);
      assert.deepEqual(await scrollCalls(), [{ id: "1", options: { block: "nearest" } }]);
      assert.deepEqual(await page.evaluate(() => [11,12].filter(id=>!JSON.parse(localStorage.getItem("lific:project-tree:disclosures")??'["11","12"]').includes(String(id)))), [12]);
      await aside.getByRole("button", { name: "Collapse One", exact: true }).click();
      await aside.locator('[data-group-id="11"] [data-action="disclose"]').click();
      for (const path of ["/ONE/pages", "/ONE/board"]) {
        await routeTo(page, path);
        await page.evaluate(() => window.fixture.refresh());
        await settle(s);
        await attr(aside.locator('[data-group-id="11"] .tc-projects__project-list'), "hidden", "");
        await attr(aside.locator(".tc-shell__navigation"), "hidden", "");
        assert.deepEqual(await scrollCalls(), [{ id: "1", options: { block: "nearest" } }], "Revalidation must not reveal twice");
      }
      assert.ok(await aside.getByRole("button", { name: "Current: One", exact: true }).isVisible());
      assert.ok((await page.evaluate(() => [11,12].filter(id=>!JSON.parse(localStorage.getItem("lific:project-tree:disclosures")??'["11","12"]').includes(String(id))))).includes(11));
    });
    await test("later failed group snapshot defers newly visible project reveal until recovery", async () => {
      const s = await session({ observeReveal: true });
      const { page, aside, api } = s;
      const scrollCalls = () => page.evaluate(() => window.sidebarScrollCalls);
      const group = aside.locator('[data-group-id="11"] .tc-projects__project-list');
      const projectNav = aside.locator("#project-nav-5");
      const revealed = [{ id: "5", options: { block: "nearest" } }];
      await settle(s);
      await attr(group, "hidden", null);
      await aside.locator('[data-group-id="11"] [data-action="disclose"]').click();
      await attr(group, "hidden", "");
      assert.ok((await page.evaluate(() => [11,12].filter(id=>!JSON.parse(localStorage.getItem("lific:project-tree:disclosures")??'["11","12"]').includes(String(id))))).includes(11));
      api.projects.push({ ...api.projects[0], id: 5, identifier: "FIVE", name: "Five", sort_order: 4 });
      api.groups.find((g) => g.id === 11).project_ids.push(5);
      api.failingGets.add("/project-groups");
      await routeTo(page, "/FIVE/issues");
      await settle(s);
      assert.ok(await aside.locator('a[aria-label="Open Five"]').isVisible(), "New project arrived in the successful project response");
      assert.equal(await group.locator('a[aria-label="Open Five"]').count(), 0, "Failed group response leaves the older membership snapshot in place");
      await attr(group, "hidden", "");
      await attr(projectNav, "hidden", "");
      assert.deepEqual(await scrollCalls(), [], "An older successful group load must not permit premature reveal");
      api.failingGets.delete("/project-groups");
      const gate = held(api, "GET", "/project-groups");
      const count = api.calls.filter((c) => c.path === "/project-groups").length;
      const recovering = page.evaluate(() => window.fixture.refresh());
      await called(api, count + 1, "GET", "/project-groups");
      assert.deepEqual(await scrollCalls(), [], "Recovery must wait for the current membership response");
      gate.release();
      await recovering;
      await settle(s);
      await attr(group, "hidden", null);
      await attr(projectNav, "hidden", null);
      assert.ok(await group.locator('a[aria-label="Open Five"]').isVisible());
      assert.deepEqual(await scrollCalls(), revealed);
      for (const collapsed of [false, true, false]) {
        if (collapsed) {
          await aside.getByRole("button", { name: "Collapse Five", exact: true }).click();
          await aside.locator('[data-group-id="11"] [data-action="disclose"]').click();
        } else if (await group.getAttribute("hidden") !== null) {
          await aside.locator('[data-group-id="11"] [data-action="disclose"]').click();
          await aside.getByRole("button", { name: "Expand Five", exact: true }).click();
        }
        await routeTo(page, collapsed ? "/FIVE/pages" : "/FIVE/board");
        await page.evaluate(() => window.fixture.refresh());
        await settle(s);
        await attr(group, "hidden", collapsed ? "" : null);
        await attr(projectNav, "hidden", collapsed ? "" : null);
        assert.equal((await page.evaluate(() => [11,12].filter(id=>!JSON.parse(localStorage.getItem("lific:project-tree:disclosures")??'["11","12"]').includes(String(id))))).includes(11), collapsed);
        assert.deepEqual(await scrollCalls(), revealed, "Later snapshots neither re-reveal nor override deliberate disclosure choices");
      }
    });
    await test("long sidebar reveals nearest once and respects subsequent user scrolling", async () => {
      const api = new API;
      const template = api.projects[0];
      api.groups = [];
      api.projects = Array.from({ length: 45 }, (_, i) => ({
        ...template,
        id: i + 1,
        identifier: `P${i + 1}`,
        name: `Project ${i + 1}`,
        sort_order: i
      }));
      const s = await session({ api, observeReveal: true });
      const { page, aside } = s;
      const nav = aside;
      const target = aside.locator('[data-project-id="45"]');
      await settle(s);
      const navBox = await nav.boundingBox();
      assert.ok((await target.boundingBox()).y > navBox.y + navBox.height, "Target begins below the visible sidebar");
      assert.equal(await nav.evaluate((el) => el.scrollTop), 0);
      await routeTo(page, "/P45/issues");
      await settle(s);
      const targetBox = await target.boundingBox();
      assert.ok(targetBox.y >= navBox.y && targetBox.y + targetBox.height <= navBox.y + navBox.height + 1, "Route entry scrolls target into view");
      assert.ok(Math.abs(targetBox.y + targetBox.height - navBox.y - navBox.height) <= 2, "Nearest scroll aligns the offscreen row to the lower edge");
      assert.deepEqual(await page.evaluate(() => window.sidebarScrollCalls), [{ id: "45", options: { block: "nearest" } }]);
      await page.mouse.move(navBox.x + navBox.width / 2, navBox.y + navBox.height / 2);
      await page.mouse.wheel(0, -1e4);
      await page.waitForFunction(() => document.querySelector("aside.tc-shell__desktop").scrollTop === 0);
      for (const path of ["/P45/issues/P45-1", "/P45/pages"]) {
        await routeTo(page, path);
        await page.evaluate(() => window.fixture.refresh());
        await settle(s);
        assert.equal(await nav.evaluate((el) => el.scrollTop), 0, "Within-project navigation must not fight manual scrolling");
        assert.deepEqual(await page.evaluate(() => window.sidebarScrollCalls), [{ id: "45", options: { block: "nearest" } }]);
      }
      await routeTo(page, "/P44/issues");
      await settle(s);
      assert.ok(await nav.evaluate((el) => el.scrollTop > 0), "Entering another project reveals it again");
      assert.deepEqual(await page.evaluate(() => window.sidebarScrollCalls), [
        { id: "45", options: { block: "nearest" } },
        { id: "44", options: { block: "nearest" } }
      ]);
    });
    await test("stable destinations and cached recents across slow failed refreshes", async () => {
      const s = await session({ route: "/ONE/issues" });
      const { page, aside, api } = s;
      const nav = aside.locator(".tc-shell__navigation");
      assert.deepEqual(await nav.locator(':scope > a[href^="/app/ONE/"]').allTextContents().then((rows) => rows.map((s) => s.trim())), destinations);
      const boardY = (await nav.getByRole("link", { name: "Board", exact: true }).boundingBox()).y;
      for (const section of ["issues", "modules", "pages", "plans"]) {
        await routeTo(page, `/ONE/${section}`);
        const toggle = aside.locator("[data-recents-toggle]");
        if (await toggle.getAttribute("aria-expanded") !== "true")
          await toggle.click();
        const recent = aside.locator("[data-recents-content]");
        await attr(recent, "aria-busy", "false");
        assert.equal(await recent.locator("a").count(), 1);
        const before = await recent.locator("a").allTextContents();
        assert.ok((await toggle.boundingBox()).y > (await nav.getByRole("link", { name: "Insights", exact: true }).boundingBox()).y);
        assert.equal((await nav.getByRole("link", { name: "Board", exact: true }).boundingBox()).y, boardY);
        const gate = held(api, "GET", `/${section}`, 409);
        await routeTo(page, `/ONE/${section}/${section === "issues" ? "ONE-1" : "10"}`);
        await attr(recent, "aria-busy", "true");
        assert.deepEqual(await recent.locator("a").allTextContents(), before);
        gate.release();
        await attr(recent, "aria-busy", "false");
        assert.deepEqual(await recent.locator("a").allTextContents(), before);
        assert.equal((await nav.getByRole("link", { name: "Board", exact: true }).boundingBox()).y, boardY);
      }
      const gate = held(api, "GET", "/issues");
      await routeTo(page, "/TWO/issues");
      const recent = aside.locator("#recent-2");
      await attr(recent, "aria-busy", "true");
      assert.equal(await recent.locator("a").count(), 0, "No previous-project cache leakage");
      gate.release();
      await attr(recent, "aria-busy", "false");
      assert.match(await recent.innerText(), /Other project item/);
    });
    await test("long titles focus hover and 230 190 pixel widths", async () => {
      const s = await session({ route: "/ONE/issues" });
      const { page, aside } = s;
      await aside.locator("[data-recents-toggle]").click();
      const recent = aside.locator('.tc-recents__list a[href$="/ONE/issues/ONE-1"]');
      await recent.waitFor();
      for (const width of [230, 190]) {
        const separator = page.getByRole("separator", { name: "Resize sidebar" });
        await separator.focus();
        while (Number(await separator.getAttribute("aria-valuenow")) > width)
          await page.keyboard.press("ArrowLeft");
        assert.equal((await aside.boundingBox()).width, width);
        await recent.hover();
        assert.equal(await recent.getAttribute("title"), `ONE-1: ${longTitle}`);
        await page.keyboard.press("Tab");
        await recent.focus();
        assert.equal(await recent.evaluate((el) => el.matches(":focus-visible")), true);
        const tip = recent.locator(".tc-recents__label");
        assert.equal(await tip.isVisible(), true);
        assert.equal(await tip.textContent(), longTitle);
        const box = await tip.boundingBox();
        assert.ok(box.x >= 0 && box.x + box.width <= width, "Focus title stays inside sidebar");
        assert.ok(await tip.evaluate((el) => el.scrollHeight <= el.clientHeight && el.scrollWidth <= el.clientWidth), "Focus title wraps without clipping");
        await page.screenshot({ path: resolve(shotDir, width === 230 ? "sidebar-desktop-default.png" : "sidebar-desktop-narrow.png") });
        await separator.focus();
        const project = aside.locator("a[data-project-id]").filter({ hasText: longName });
        await project.hover();
        assert.equal(await project.getAttribute("title"), longName);
        const actions = aside.getByRole("button", { name: `Actions for ${longName}`, exact: true });
        await actions.focus();
        assert.equal(await actions.evaluate((el) => getComputedStyle(el).opacity), "1");
        const actionBox = await actions.boundingBox();
        assert.ok(actionBox.x + actionBox.width <= width, "Long project preserves visible actions");
        assert.equal(await aside.locator("nav").evaluate((el) => el.scrollWidth <= el.clientWidth), true, "No horizontal sidebar scroll");
      }
      assert.equal(await page.evaluate(() => localStorage.getItem("lific:sidebar:width")), "190");
      await page.reload();
      await attr(page.getByRole("separator", { name: "Resize sidebar" }), "aria-valuenow", "190");
    });
    await test("focused recent tooltip does not steal another project click", async () => {
      const s = await session({ route: "/ONE/issues" });
      const { page, aside } = s;
      await aside.locator("[data-recents-toggle]").click();
      const recent = aside.locator(".tc-recents__list a").first();
      await page.keyboard.press("Tab");
      await recent.focus();
      await recent.locator(".tc-recents__label").waitFor();
      const project = aside.locator("a[data-project-id]").filter({ hasText: longName });
      await project.click();
      await page.waitForURL("**/app/LONG/overview");
    });
    await test("text-scaled default preserves pixel preference and resize semantics", async () => {
      const { page, aside } = await session({ route: "/settings" });
      const handle = page.getByRole("separator", { name: "Resize sidebar" });
      const stored = () => page.evaluate(() => localStorage.getItem("lific:sidebar:width"));
      const width = async (expected) => {
        await attr(handle, "aria-valuenow", String(expected));
        assert.ok(Math.abs((await aside.boundingBox()).width - expected) < 0.02, "ARIA reports the rendered CSS-pixel width");
      };
      const scale = async (name) => {
        await page.locator("[data-tc-preference=fontScale]").first().selectOption({ S: "small", M: "normal", L: "large" }[name]);
      };
      await width(230);
      await scale("L");
      await width(258.75);
      await attr(handle, "aria-valuemin", "202.5");
      await scale("S");
      await width(215.625);
      await scale("M");
      await width(230);
      assert.equal(await stored(), null, "Text changes do not create a manual preference");
      await handle.click();
      assert.equal(await stored(), null, "Clicking the resize handle is not a resize");
      await scale("L");
      await page.evaluate(() => localStorage.setItem("lific:sidebar:width", "300"));
      await page.reload();
      await width(300);
      await scale("S");
      await width(300);
      await scale("M");
      await width(300);
      assert.equal(await stored(), "300");
      await page.evaluate(() => localStorage.setItem("lific:sidebar:width", "190"));
      await page.reload();
      await width(190);
      await scale("L");
      await width(202.5);
      assert.equal(await stored(), "190", "Temporary minimum does not overwrite the saved width");
      await page.reload();
      await width(202.5);
      await scale("M");
      await width(190);
      await scale("L");
      await width(202.5);
      await handle.focus();
      await page.keyboard.press("ArrowRight");
      await width(212.5);
      assert.equal(await stored(), "212.5", "Keyboard resize is still ten physical pixels");
      const box = await handle.boundingBox();
      await page.mouse.move(box.x + box.width / 2, box.y + 100);
      await page.mouse.down();
      await page.mouse.move(box.x + box.width / 2 + 37, box.y + 100);
      await width(249.5);
      assert.equal(await stored(), "212.5", "Dragging does not persist until release");
      await page.mouse.up();
      assert.equal(await stored(), "249.5");
      await page.reload();
      await width(249.5);
      await handle.dblclick();
      await width(258.75);
      assert.equal(await stored(), null, "Reset resumes the proportional default");
      await aside.getByRole("button", { name: "Collapse sidebar", exact: true }).click();
      await scale("M");
      await page.getByRole("button", { name: "Expand sidebar", exact: true }).click();
      await width(230);
      assert.equal(await stored(), null);
    });
    await test("native modified sidebar links", async () => {
      const s = await session({ route: "/ONE/issues" });
      const { page, aside } = s;
      await aside.locator("[data-recents-toggle]").click();
      const links = [aside.locator('a[aria-label="Open Two"]'), aside.locator('.tc-shell__navigation > a[href$="/ONE/board"]'), aside.locator(".tc-recents__list a").first(), aside.locator('a[href$="/settings"]')];
      await withNativeLinkDiagnostics(page, async (openPopup) => {
        for (const link of links) {
          const href = await link.getAttribute("href");
          for (const gesture of ["ctrl", "middle"]) {
            await openPopup(link, gesture, async (popup) => {
              assert.equal(new URL(popup.url()).pathname, href);
              assert.equal(new URL(page.url()).pathname.replace(/^\/app/, ""), "/ONE/issues");
              await popup.locator('aside a[href$="/settings"]').waitFor();
              await settle({ ...s, page: popup });
            });
          }
        }
      });
    });
    await test("group create rename failure retry Cancel Escape", async () => {
      const s = await session();
      const { page, aside, api } = s;
      const input = aside.getByRole("textbox", { name: "Group name", exact: true });
      for (const mode of ["create", "rename"]) {
        const start = async () => {
          if (mode === "create")
            await action(s, "New project or group", "New group");
          else
            await action(s, "Actions for group Work", "Rename");
          await input.waitFor();
        };
        for (const cancel of ["Cancel", "Escape"]) {
          await start();
          await input.fill("Discard this draft");
          const before = api.calls.filter((c) => c.method !== "GET").length;
          if (cancel === "Escape")
            await input.press("Escape");
          else
            await aside.getByRole("button", { name: "Cancel", exact: true }).click();
          await input.waitFor({ state: "hidden" });
          assert.equal(api.calls.filter((c) => c.method !== "GET").length, before);
          const trigger = aside.getByRole("button", { name: mode === "create" ? "New project or group" : "Actions for group Work", exact: true });
          assert.equal(await trigger.evaluate((el) => document.activeElement === el), true, "Cancel restores the edit trigger");
        }
        await start();
        const name = mode === "create" ? "Research" : "Renamed work";
        const path = mode === "create" ? "/project-groups" : "/project-groups/11";
        const method = mode === "create" ? "POST" : "PATCH";
        await input.fill(name);
        await input.press("Tab");
        assert.equal(await input.inputValue(), name, "Blur must not save or discard");
        const gate = held(api, method, path, 409);
        await aside.getByRole("button", { name: "Save", exact: true }).click();
        await attr(input, "disabled", "");
        assert.equal(await aside.getByRole("button", { name: "Saving…", exact: true }).isDisabled(), true);
        gate.release();
        await aside.getByRole("alert").waitFor();
        assert.match(await aside.getByRole("alert").innerText(), /Fixture rejected/);
        assert.equal(await input.inputValue(), name);
        await attr(input, "aria-invalid", "true");
        assert.equal(await input.evaluate((el) => document.activeElement === el), true);
        await input.press("Enter");
        await input.waitFor({ state: "hidden" });
        await aside.getByRole("button", { name: `Actions for group ${name}`, exact: true }).waitFor();
        const writes = api.calls.filter((c) => c.method === method && c.path === path);
        assert.deepEqual(writes.map((c) => c.body), [{ name }, { name }]);
      }
    });
    await test("personal group and grouped project order payloads rollback", async () => {
      const s = await session();
      const { aside, api } = s;
      const groupOrder = () => aside.locator('button[aria-label^="Actions for group "]').evaluateAll((els) => els.map((el) => el.getAttribute("aria-label")));
      const projectOrder = () => aside.locator('[data-group-id="11"] a[title]').evaluateAll((els) => els.map((el) => el.getAttribute("title")));
      const originalGroups = await groupOrder();
      for (const kind of ["group", "project"]) {
        const path = kind === "group" ? "/project-groups/reorder" : "/projects/reorder";
        const trigger = kind === "group" ? "Actions for group Work" : "Actions for One";
        const original = kind === "group" ? originalGroups : ["One", "Two"];
        const readOrder = kind === "group" ? groupOrder : projectOrder;
        const downIds = kind === "group" ? [12, 11] : [2, 1, 3, 4];
        const upIds = kind === "group" ? [11, 12] : [1, 2, 3, 4];
        await aside.getByRole("button", { name: trigger, exact: true }).click();
        assert.equal(await menuItem(s.page, "Move up").isDisabled(), true);
        await s.page.keyboard.press("Escape");
        await action(s, trigger, "Move down");
        assert.deepEqual((await called(api, 1, "PUT", path)).body, { ids: downIds });
        await settle(s);
        assert.deepEqual(await readOrder(), [...original].reverse());
        await action(s, trigger, "Move up");
        assert.deepEqual((await called(api, 2, "PUT", path)).body, { ids: upIds });
        await settle(s);
        assert.deepEqual(await readOrder(), original);
        const gate = held(api, "PUT", path, 409);
        await action(s, trigger, "Move down");
        await called(api, 3, "PUT", path);
        assert.deepEqual(await readOrder(), [...original].reverse(), "Optimistic order is visible");
        gate.release();
        await aside.getByRole("alert").waitFor();
        assert.match(await aside.getByRole("alert").innerText(), new RegExp(`${kind === "group" ? "Group" : "Project"} order wasn't saved`));
        assert.deepEqual(await readOrder(), original, "Failed write rolls visible order back");
      }
      await action(s, "Actions for One", "Move down");
      await settle(s);
      await s.page.reload();
      await settle(s);
      assert.deepEqual(await projectOrder(), ["Two", "One"], "Canonical order reloads from API");
      const otherAPI = new API;
      otherAPI.user.id = 2;
      otherAPI.user.display_name = "Other Operator";
      otherAPI.groups.forEach((g) => g.user_id = 2);
      const other = await session({ api: otherAPI });
      assert.deepEqual(await other.aside.locator('[data-group-id="11"] a[title]').evaluateAll((els) => els.map((el) => el.getAttribute("title"))), ["One", "Two"]);
      assert.equal(otherAPI.calls.filter((c) => c.method !== "GET").length, 0);
      assert.equal(api.calls.some((c) => c.method === "PUT" && /^\/projects\/\d+$/.test(c.path)), false, "Order must not mutate project records");
    });
    await test("Settings identity and theme share live shell state", async () => {
      const s = await session();
      const { page, aside } = s;
      const identity = aside.locator('a[href$="/settings"]');
      await identity.click();
      await page.waitForURL("**/app/settings");
      await page.getByLabel("Display name", { exact: true }).fill("Updated Operator");
      await page.getByRole("button", { name: "Save profile", exact: true }).click();
      await identity.filter({ hasText: "Updated Operator" }).waitFor();
      await page.getByLabel("Theme",{exact:true}).selectOption("dark");
      await attr(aside.getByRole("button", { name: "Choose theme, current: dark", exact: true }), "title", "Theme: dark");
      assert.equal(await page.evaluate(() => document.documentElement.dataset.theme === "dark"), true);
      await action(s, "Choose theme, current: dark", "Light");
      assert.equal(await page.evaluate(() => localStorage.getItem("lific_theme")), "light");
      assert.equal(await page.getByLabel("Theme",{exact:true}).inputValue(),"light");
      await page.reload();
      await identity.filter({ hasText: "Updated Operator" }).waitFor();
      await aside.getByRole("button", { name: "Choose theme, current: light", exact: true }).waitFor();
    });
    await test("real Layout mobile menus history and palette suppression", async () => {
      const s = await session({ mobile: true });
      const { page } = s;
      const depth = (n) => page.waitForFunction((n) => history.state?.lificMobileNav?.depth === n, n);
      const rootPane = page.locator("[data-mobile-root]");
      const projectPane = page.locator("[data-mobile-project]");
      const open = async () => {
        await page.getByRole("button", { name: "Open navigation", exact: true }).click();
        await depth(1);
      };
      const closed = () => page.waitForFunction(() => !window.fixture.mobile());
      await open();
      assert.equal(await page.locator("main").evaluate((el) => !!el.closest("[inert]")), true);
      for (const key of ["Control+k", "Control+p", "Meta+k", "Meta+p"])
        await page.keyboard.press(key);
      assert.equal(await page.evaluate(() => window.fixture.palette()), false);
      await rootPane.getByRole("button", { name: "Open One navigation", exact: true }).click();
      await depth(2);
      await page.goBack();
      await depth(1);
      await page.goForward();
      await depth(2);
      await projectPane.getByRole("link", { name: "Board", exact: true }).click();
      await page.waitForURL("**/app/ONE/board");
      await closed();
      await page.goBack();
      await depth(0);
      assert.equal(new URL(page.url()).pathname.replace(/^\/app/, ""), "/");
      await open();
      await rootPane.getByRole("button", { name: "Actions for One", exact: true }).click();
      await menuItem(page, "Move down").click();
      assert.deepEqual((await called(s.api, 1, "PUT", "/projects/reorder")).body, { ids: [2, 1, 3, 4] });
      await rootPane.getByRole("button", { name: "Actions for Work", exact: true }).click();
      await menuItem(page, "Rename").waitFor();
      await page.keyboard.press("Escape");
      await depth(1);
      await rootPane.getByRole("button", { name: "New project or group", exact: true }).click();
      await menuItem(page, "New project").click();
      await page.waitForURL("**/app/projects/new");
      await closed();
      await page.goBack();
      await depth(0);
      await open();
      await page.waitForFunction(() => new DOMMatrix(getComputedStyle(document.querySelector("[data-mobile-navigation]")).transform).m41 === 0);
      await page.screenshot({ path: resolve(shotDir, "sidebar-mobile.png") });
      await page.keyboard.press("Escape");
      await depth(0);
      await closed();
      await page.keyboard.press("Control+k");
      await page.waitForFunction(() => window.fixture.palette());
      await page.keyboard.press("Escape");
    });
  } finally {
    for (const gate of allGates)
      gate.release();
    await fixture.close();
  }
});
