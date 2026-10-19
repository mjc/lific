// Ported from e2e/discord-feedback.ts on master 9683d38a.
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import fixtureModule from "../../../acceptance/server.js";
const { startFixture } = fixtureModule;
async function checkSelection(context, base) {
  const modifier = process.platform === "darwin" ? "Meta" : "Control";
  for (const width of [1440, 390]) {
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    try {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(`${base}/ACC/issues`);
      const rows = page.locator("[data-issue-id]");
      await rows.nth(2).waitFor();
      const selected = page.locator('[data-issue-id] input[type="checkbox"]:checked');
      const all = page.getByRole("button", { name: /^(Select all|All selected)( visible)?$/ });
      const clear = page.getByRole("button", { name: "Clear selection", exact: true });
      const selectionCount = page.getByText(/^\d+ selected$/, { exact: true });
      async function assertSelection(identifiers, visibleCount) {
        await page.waitForFunction((count) => document.querySelectorAll('[data-issue-id] input[type="checkbox"]:checked').length === count, identifiers.length);
        assert.deepEqual(await selected.evaluateAll((els) => els.map((el) => el.getAttribute("aria-label")).sort()), identifiers.map((id) => `Select ${id}`).sort());
        if (identifiers.length) {
          await selectionCount.waitFor();
          assert.equal(await selectionCount.innerText(), `${identifiers.length} selected`);
          assert(await clear.isVisible());
        } else {
          await selectionCount.waitFor({ state: "hidden" });
          await clear.waitFor({ state: "hidden" });
        }
        if (width >= 640) {
          const complete = identifiers.length === visibleCount;
          assert.equal(await all.getAttribute("aria-pressed"), String(complete));
          const count = identifiers.length && !complete ? `${identifiers.length}/${visibleCount}` : visibleCount;
          assert.equal((await all.innerText()).replace(/\s+/g, " ").trim(), `${complete ? "All selected" : "Select all"} ${count}`);
        } else {
          assert.equal(await all.count(), 0, "desktop select-all must not be exposed on mobile");
        }
      }
      async function selectVisible() {
        if (width >= 640)
          await all.click();
        else {
          await rows.first().getByRole("checkbox").focus();
          await page.keyboard.press(`${modifier}+a`);
        }
      }
      async function clearSelection() {
        if (width >= 640)
          await all.click();
        else
          await clear.click();
      }
      const checkbox = page.getByRole("checkbox", { name: "Select ACC-1", exact: true });
      await assertSelection([], 3);
      assert.equal(await checkbox.evaluate((el) => getComputedStyle(el).opacity), "1");
      const initialUrl = page.url();
      await rows.first().click({ position: { x: 2, y: 2 } });
      assert.equal(page.url(), initialUrl, "row whitespace must not navigate");
      await page.keyboard.press(`${modifier}+a`);
      await assertSelection(["ACC-1", "ACC-2", "ACC-3"], 3);
      await clearSelection();
      await assertSelection([], 3);
      await checkbox.click();
      await page.getByRole("checkbox", { name: "Select ACC-2", exact: true }).click();
      await assertSelection(["ACC-1", "ACC-2"], 3);
      await selectVisible();
      await assertSelection(["ACC-1", "ACC-2", "ACC-3"], 3);
      await page.getByRole("checkbox", { name: "Select ACC-3", exact: true }).click();
      await assertSelection(["ACC-1", "ACC-2"], 3);
      const downloadEvent = page.waitForEvent("download");
      await page.getByRole("button", { name: "Export selected", exact: true }).click();
      const download = await downloadEvent;
      assert.equal(download.suggestedFilename(), "ACC-selected-issues.md");
      const text = readFileSync(await download.path(), "utf8");
      assert(text.includes("Smoke issue") && text.includes("Second smoke issue") && text.includes("First smoke comment"));
      assert(!text.includes("Excluded smoke issue"));
      await page.getByRole("button", { name: "Export selected", exact: true }).waitFor({ state: "visible" });
      await page.route("**/api/export/issues/*", (route) => route.fulfill({ status: 503, body: "unavailable" }));
      let partialDownload = false;
      page.on("download", () => {
        partialDownload = true;
      });
      await page.getByRole("button", { name: "Export selected", exact: true }).click();
      await page.getByRole("alert").filter({ hasText: "Could not export" }).waitFor();
      assert.equal(partialDownload, false);
      await page.unroute("**/api/export/issues/*");
      await clear.click();
      await assertSelection([], 3);
      const backlog = page.getByRole("button", { name: /^Backlog\s+1$/i });
      await backlog.click();
      await rows.nth(2).waitFor({ state: "detached" });
      await selectVisible();
      await assertSelection(["ACC-1", "ACC-2"], 2);
      await clearSelection();
      await assertSelection([], 2);
      await backlog.click();
      await rows.nth(2).waitFor();
      await rows.first().click({ position: { x: 2, y: 2 } });
      await page.keyboard.press("/");
      const search = page.getByPlaceholder("Search issues");
      await search.fill("Second smoke");
      await page.waitForFunction(() => document.querySelectorAll("[data-issue-id]").length === 1);
      await search.press(`${modifier}+a`);
      assert.equal(await search.evaluate((el) => el.selectionEnd - el.selectionStart), "Second smoke".length);
      assert.equal(await selected.count(), 0, "Ctrl+A in search must select text, not rows");
      await assertSelection([], 1);
      await selectVisible();
      await assertSelection(["ACC-2"], 1);
      await clearSelection();
      await assertSelection([], 1);
      await search.fill("");
      await search.press("Escape");
      await rows.nth(2).waitFor();
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth);
      assert.equal(overflow, false, `list overflow at ${width}px`);
      assert(!(await page.locator("body").innerText()).includes("Lucide:Terminal"), "malformed icon leaked text");
      await page.getByRole("link", { name: "Smoke issue", exact: true }).click();
      await page.waitForURL(/ACC-1$/);
      assert.equal(errors.length, 0, errors.join(`
`));
      console.log(`ok   Discord issue selection, export, icons (${width}px)`);
    } finally {
      await page.close();
    }
  }
}
async function checkConnections(context, base) {
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  page.on("dialog",dialog=>dialog.accept());
  try {
    await page.setViewportSize({ width: 390, height: 900 });
    await page.goto(`${base}/settings`);
    for (const id of ["codex-laptop", "codex-desktop", "custom-agent"]) {
      await page.getByLabel("Client template").selectOption('custom');
      await page.getByLabel("Custom tool ID",{exact:true}).fill(id);
      await page.getByLabel("Display name (optional)").fill(id);
      await page.getByRole("button",{name:'Connect tool',exact:true}).click();
      const dialog=page.locator('[data-secret]');
      await dialog.getByRole('button',{name:'Copy key',exact:true}).waitFor();
      if(id==='custom-agent') {
        assert((await dialog.innerText()).includes('HTTP MCP'));
        assert(!(await dialog.innerText()).includes('same everywhere'));
        const config=JSON.parse(await dialog.locator('[data-client-config]').innerText());
        assert(config.url.endsWith('/app/mcp'));
        assert(config.headers.Authorization.startsWith('Bearer '));
      }
      await page.reload();
    }
    await page.reload();
    const laptop = page.locator("section").filter({has:page.getByRole("heading",{name:"Connected tools",exact:true})}).locator("p").filter({hasText:"codex-laptop"});
    const desktop = page.locator("section").filter({has:page.getByRole("heading",{name:"Connected tools",exact:true})}).locator("p").filter({hasText:"codex-desktop"});
    await laptop.getByRole("button", { name: "Disconnect", exact: true }).click();
    await laptop.getByRole("button", { name: "Reconnect", exact: true }).waitFor();
    assert(await desktop.getByRole("button", { name: "Disconnect", exact: true }).isVisible());
    await laptop.getByRole("button", { name: "Reconnect", exact: true }).click();
    await page.locator("[data-secret]").getByRole("button", { name: "Copy key", exact: true }).waitFor();
    await page.reload();
    assert.equal(await laptop.count(), 1);
    await laptop.getByRole("button", { name: "Disconnect", exact: true }).click();
    await laptop.getByRole("button", { name: "Delete", exact: true }).click();
    await laptop.waitFor({ state: "detached" });
    assert(await desktop.getByRole("button", { name: "Disconnect", exact: true }).isVisible());
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    assert.equal(errors.length, 0, errors.join(`
`));
    console.log("ok   Discord custom/named connection lifecycle (390px)");
  } finally {
    await page.close();
  }
}
async function seededFixture() {
  const f = await startFixture();
  try {
  async function create(route, body) {
    const r = await f.api(route, { method: "POST", body });
    assert(r.ok, await r.clone().text());
    return r.json();
  }
  const update = await f.api("/issues/" + f.issue.id, { method: "PUT", body: { title: "Smoke issue", status: "active", expected_seq: f.issue.seq } });
  assert(update.ok, await update.clone().text());
  await create("/issues", { project_id: f.project.id, title: "Second smoke issue", status: "todo" });
  await create("/issues", { project_id: f.project.id, title: "Excluded smoke issue", status: "backlog" });
  f.cli(["comment", "add", "ACC-1", "--content", "First smoke comment", "--json"]);
  return f;
  } catch(error) {await f.close();throw error;}
}
for (const [title, check] of [["master discord feedback: issue selection and real selected exports", checkSelection], ["master discord feedback: independent named MCP connections lifecycle", checkConnections]]) {
  test(title, { timeout: 120000 }, async () => {
    const f = await seededFixture();
    try {
      const page = await f.newPage();
      await check(page.context(), f.origin + f.prefix);
    } finally {
      await f.close();
    }
  });
}

test('master named Codex connections keep the template separate from connection identity',{timeout:30000},async()=>{
 const f=await startFixture();try {
  const p=await f.newPage();await p.goto(f.url('/settings'));
  for(const id of ['codex-laptop','codex-desktop']) {
    await p.getByLabel('Client template').selectOption('codex');
    await p.getByLabel('Custom tool ID',{exact:true}).fill(id);
    await p.getByLabel('Display name (optional)').fill(id);
    const pending=p.waitForRequest(request=>request.method()==='POST'&&request.url().endsWith('/api/auth/bots'));
    await p.getByRole('button',{name:'Connect tool',exact:true}).click();
    const request=await pending;assert.equal(request.postDataJSON().tool,id);
    await p.locator('[data-secret]').getByRole('button',{name:'Copy key',exact:true}).waitFor();
    await p.reload();
  }
  const response=await f.api('/auth/bots');assert(response.ok);const bots=await response.json();
  assert.deepEqual(bots.map(bot=>bot.tool_id).sort(),['codex-desktop','codex-laptop']);
 }finally{await f.close();}
});

test('master Discord selected exports keep real comments, order, exclusions and reject partial downloads',{timeout:60000},async(t)=>{
 const f=await seededFixture();try {
  for(const width of [1440,390])await t.test(`${width}px`,async()=>{
   const page=await f.newPage();try {
    await page.setViewportSize({width,height:900});await page.goto(f.url('/ACC/issues'));
    await page.getByRole('checkbox',{name:'Select ACC-1',exact:true}).check();
    await page.getByRole('checkbox',{name:'Select ACC-2',exact:true}).check();
    const pending=page.waitForEvent('download');
    await page.getByRole('button',{name:'Export selected',exact:true}).click();
    const download=await pending;assert.equal(download.suggestedFilename(),'ACC-selected-issues.md');
    const text=readFileSync(await download.path(),'utf8');
    assert(text.includes('Smoke issue')&&text.includes('Second smoke issue')&&text.includes('First smoke comment'));
    assert(!text.includes('Excluded smoke issue'));
    await page.getByRole('button',{name:'Export selected',exact:true}).waitFor();
    await page.route('**/api/export/issues/*',route=>route.fulfill({status:503,body:'unavailable'}));
    let partial=false;page.on('download',()=>{partial=true;});
    await page.getByRole('button',{name:'Export selected',exact:true}).click();
    await page.getByRole('alert').filter({hasText:'Could not export'}).waitFor();
    assert.equal(partial,false);
    await page.unroute('**/api/export/issues/*');
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    assert(!(await page.locator('body').innerText()).includes('Lucide:Terminal'));
   }finally{await page.context().close();}
  });
 }finally{await f.close();}
});
