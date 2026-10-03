const { test, before, after, assert, root } = require("./harness");
const { startFixture } = require("../../../acceptance/server");
let fixture;
before(async () => {
  fixture = await startFixture();
});
after(async () => {
  await fixture?.close();
});
const cases = [
  { name: "viewer", role: "viewer", enforced: true, is_admin: false, editable: false },
  { name: "maintainer", role: "maintainer", enforced: true, is_admin: false, editable: true },
  { name: "admin", role: null, enforced: true, is_admin: true, editable: true },
  { name: "legacy", role: "viewer", enforced: false, is_admin: false, editable: true },
  { name: "unavailable", role: null, enforced: true, is_admin: false, editable: false }
];
for (const policy of cases)
  test("peek permissions: " + policy.name, async () => {
    const context = await fixture.browser.newContext({ viewport: { width: 1280, height: 900 }, reducedMotion: "reduce" });
    const page = await context.newPage();
    try {
      page.setDefaultTimeout(1500);page.setDefaultNavigationTimeout(15000);
      const errors = [];
      const writes = [];
      const roleProjects = [];
      let saveGate = Promise.resolve();
      let release;
      const gate = new Promise((resolve) => release = resolve);
      const issue = { id: 22, project_id: 2, identifier: "VIEW-1", title: "Viewer preview fixture", description: "Synthetic issue", status: "todo", priority: "high", module_id: 3, labels: [], blocks: [], blocked_by: [], relates_to: [], duplicates: [], created_at: "2026-09-18 10:00:00", updated_at: "2026-09-18 10:00:00" };
      page.on("pageerror", (error) => errors.push(error.message));
      await page.route("**/api/**", async (route) => {
        const request = route.request();
        const path = new URL(request.url()).pathname.replace(/^\/app/, "");
        if (request.method() !== "GET") {
          writes.push(request.postDataJSON());
          await saveGate;
          Object.assign(issue, request.postDataJSON());
          return route.fulfill({ json: issue });
        }
        if (path.endsWith("/my-role")) {
          roleProjects.push(path);
          await gate;
          return route.fulfill(policy.name === "unavailable" ? { status: 503, json: { error: "Unavailable" } } : { json: policy });
        }
        if (path.includes("/issues/resolve/"))
          return route.fulfill({ json: issue });
        if (path === "/api/modules")
          return route.fulfill({ json: [{ id: 3, project_id: 2, name: "Tracking", emoji: null }] });
        return route.fulfill({ json: [] });
      });
      await page.goto(fixture.url("/__component_fixture"));
      await page.setContent('<button id="preview">Preview issue</button>');
      await page.addScriptTag({ path: require("node:path").join(root, "src/topcoat/plans/assets/picker.js") });
      await page.evaluate(() => document.querySelector("#preview").onclick = () => LificTopcoatIssuePicker.peek(document.body, { identifier: "VIEW-1", request: async (path) => {
        const r = await fetch("/app/api" + path);
        if (!r.ok)
          throw new Error("Unavailable");
        return r.json();
      } }));
      await page.getByRole("button", { name: "Preview issue", exact: true }).click();
      const panel = page.locator("dialog.tc-issue-picker");
      await panel.waitFor();
      assert.equal(await panel.getByRole("button", { name: issue.title, exact: true }).count(), 0, `${policy.name}: title editable before issue-project permissions resolve`);
      for (const name of ["Todo", "High", "Tracking"])
        assert.equal(await panel.getByRole("button", { name, exact: true }).count(), 0);
      release();
      if (policy.editable) {
        await panel.getByRole("button", { name: issue.title, exact: true }).waitFor();
        for (const name of ["Todo", "High", "Tracking"])
          await panel.getByRole("button", { name, exact: true }).waitFor();
        await panel.getByRole("button", { name: issue.title, exact: true }).click();
        await panel.getByRole("textbox").fill("Changed by an editor");
        await panel.getByRole("textbox").press("Tab");
        await panel.getByRole("button", { name: "Changed by an editor", exact: true }).waitFor();
        assert.deepEqual(writes, [{ title: "Changed by an editor" }]);
        if (policy.name === "maintainer") {
          let finishSave;
          saveGate = new Promise((resolve) => finishSave = resolve);
          await panel.getByRole("button", { name: issue.title, exact: true }).click();
          await panel.getByRole("textbox").fill("Saved with Enter");
          const requested = page.waitForRequest((request) => request.method() === "PUT");
          await panel.getByRole("textbox").press("Enter");
          await requested;
          await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
          const pendingWrites = writes.length;
          finishSave();
          assert.equal(pendingWrites, 2, "Enter and blur must share one delayed save");
          await panel.getByRole("button", { name: "Saved with Enter", exact: true }).waitFor();
          await panel.getByRole("button", { name: "Saved with Enter", exact: true }).click();
          await panel.getByRole("textbox").fill("Cancelled draft");
          await panel.getByRole("textbox").press("Escape");
          await panel.waitFor({ state: "hidden" });
          await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
          assert.equal(writes.length, 2, "Escape must not commit through blur");
          await page.getByRole("button", { name: "Preview issue", exact: true }).click();
          await panel.getByRole("button", { name: "Saved with Enter", exact: true }).click();
          await panel.getByRole("textbox").fill("Saved with shortcut");
          await panel.getByRole("textbox").press("Control+s");
          await panel.getByRole("button", { name: "Saved with shortcut", exact: true }).waitFor();
          assert.deepEqual(writes, [{ title: "Changed by an editor" }, { title: "Saved with Enter" }, { title: "Saved with shortcut" }]);
        }
      } else {
        await panel.getByText(policy.name === "unavailable" ? "Editing permissions could not be checked." : "Read-only access", { exact: true }).waitFor();
        await panel.getByRole("heading", { name: issue.title, exact: true }).waitFor();
        for (const name of ["Todo", "High", "Tracking"])
          assert.equal(await panel.getByRole("button", { name, exact: true }).count(), 0);
        assert.equal(await panel.getByRole("textbox").count(), 0);
        for (const text of ["Todo", "High", "Tracking"])
          await panel.getByText(text, { exact: true }).waitFor();
        assert.deepEqual(writes, []);
      }
      assert.deepEqual(roleProjects, ["/api/projects/2/my-role"]);
      assert.deepEqual(errors, []);
    } finally {
      await context.close();
    }
  });
