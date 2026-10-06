// Ported from e2e/project-archives.ts on master 9683d38a.
import fixtureModule from "../../../acceptance/server.js";
const {startFixture}=fixtureModule;
const fixtures=[];
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync as Database } from "node:sqlite";
import assert from "node:assert/strict";
const PASSWORD = "archive-smoke-password-123";
const scratch = mkdtempSync(join(tmpdir(), "lific-archives-"));
let browser = null;
const errors = [];
let finishing = false;
async function finish() {
  if(finishing)return;finishing=true;
  for(const fixture of fixtures)await fixture.close();
  rmSync(scratch,{recursive:true,force:true});
}
async function instance(name,seed) {
 const f=await startFixture({seed:false});fixtures.push(f);
 const cli=f.cli;
 cli(["user","create","--username","regular","--email",`regular-${name}@example.test`,"--password",PASSWORD,"--json"]);
 cli(["user","create","--username","second-admin","--email",`second-${name}@example.test`,"--password",PASSWORD,"--admin","--json"]);
 if(seed) {
  cli(["project","create","--name","Archive specimen","--identifier","ARC","--json"]);
  cli(["issue","create","--project","ARC","--title","Archive graph root","--description","Missing source file: /api/attachments/999999","--json"]);
  cli(["issue","create","--project","ARC","--title","Archive graph child","--json"]);
  cli(["comment","add","ARC-1","--content","A comment carried across instances","--json"]);
  cli(["page","create","--project","ARC","--title","Archive notebook","--content","A page carried across instances","--json"]);
 }
 const sql=new Database(f.database);sql.exec("UPDATE instance_settings SET web_auto_login=0, authz_enforced=0");
 if(seed)sql.exec("UPDATE projects SET is_public=1 WHERE identifier='ARC'");sql.close();
 const base=f.origin+f.prefix;
 const login=async(identity)=>{const r=await f.api('/auth/login',{method:'POST',token:null,body:{identity,password:PASSWORD}});assert.equal(r.status,200,await r.clone().text());return (await r.json()).token;};
 const api=async(path,method='GET',body)=>{const r=await f.api(path,{method,body});assert(r.ok,`${path}: ${r.status} ${await r.clone().text()}`);return r.json();};
 return {...f,base,api,login};
}

function watch(page) {
  page.on("pageerror", (error) => errors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() !== "error")
      return;
    if (/^Failed to load resource:.*(400|403|409|422|net::ERR_FAILED)/.test(message.text()))
      return;
    errors.push(message.text());
  });
}
async function signedPage(base, token, width = 1280) {
  const context = await browser.newContext({ viewport: { width, height: 900 } });
  await context.addCookies([{ name: "lific_token", value: token, url: base, httpOnly: true, sameSite: "Lax" }]);
  await context.route("https://fonts.googleapis.com/**", (route) => route.fulfill({ status: 200, contentType: "text/css", body: "" }));
  await context.addInitScript((token) => {
    if (!localStorage.getItem("lific_token"))
      localStorage.setItem("lific_token", token);
  }, token);
  const page = await context.newPage();
  watch(page);page.setDefaultTimeout(30000);
  await page.goto(`${base}/projects/new`);
  return page;
}
async function visible(page, text) {
  try {
    await page.getByText(text, { exact: false }).filter({ visible: true }).first().waitFor({ state: "visible", timeout: 15000 });
  } catch (error) {
    throw new Error(`${error}
Page: ${await page.locator("body").innerText()}`);
  }
}
async function changeSession(page, base, token) {
  await page.context().addCookies([{ name: "lific_token", value: token, url: base, httpOnly: true, sameSite: "Lax" }]);
  await page.evaluate((token) => {
    const oldValue = localStorage.getItem("lific_token");
    localStorage.setItem("lific_token", token);
    window.dispatchEvent(new StorageEvent("storage", { key: "lific_token", oldValue, newValue: token }));
  }, token);
  const response=await fetch(`${base}/api/auth/me`,{headers:{Authorization:`Bearer ${token}`}});
  assert.equal(response.status,200);const user=await response.json();
  await page.waitForFunction(id=>window.lificSession.state.user?.id===id&&!window.lificSession.state.loading,user.id);
}
async function staleDownload(page, base, leave) {
  await page.goto(`${base}/ARC/overview`);
  const panel = page.locator("section").filter({ has: page.getByRole("heading", { name: "Project archive", exact: true }) });
  await panel.getByRole("checkbox").check();
  let release, arrived, finished;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const started = new Promise((resolve) => {
    arrived = resolve;
  });
  const served = new Promise((resolve) => {
    finished = resolve;
  });
  const downloads = [];
  const download = (event) => downloads.push(event.suggestedFilename());
  page.on("download", download);
  const aborted = page.waitForEvent("requestfailed", { predicate: (request) => request.url().endsWith("/api/project-archives/ARC"), timeout: 1e4 });
  await page.route("**/api/project-archives/ARC", async (route) => {
    const response = await route.fetch();
    arrived();
    await held;
    await route.fulfill({ response }).catch(() => {});
    finished();
  });
  await panel.getByRole("button", { name: "Download project archive" }).click();
  await started;
  await leave();
  release();
  await served;
  await aborted;
  await page.waitForTimeout(250);
  assert.deepEqual(downloads, [], "stale archive never triggers a browser download");
  page.off("download", download);
  await page.unroute("**/api/project-archives/ARC");
}
async function main(t) {
  const a = await instance("source", true);
  const b = await instance("destination", false);
  const caps = await b.api("/project-archives");
  assert.equal(caps.can_import, true, "fresh backend must expose archive capabilities");
  assert.deepEqual(await b.api("/projects"), [], "destination starts with no projects");
  await a.api("/issues/link", "POST", { source: "ARC-1", target: "ARC-2", relation_type: "blocks" });
  const issue = await a.api("/issues/resolve/ARC-1");
  const form = new FormData;
  form.append("file", new File(["Archive attachment bytes"], "archive-note.txt", { type: "text/plain" }));
  form.append("entity_type", "issue");
  form.append("entity_id", String(issue.id));
  const uploaded = await fetch(`${a.base}/api/attachments`, { method: "POST", headers: { Authorization: `Bearer ${a.token}` }, body: form });
  assert(uploaded.ok, await uploaded.text());
  browser = a.browser;
  const source = await signedPage(a.base, a.token);
  await source.goto(`${a.base}/ARC/overview`);
  const exportPanel = source.locator("section").filter({ has: source.getByRole("heading", { name: "Project archive", exact: true }) });
  await exportPanel.getByRole("checkbox").check();
  const downloaded = source.waitForEvent("download");
  await exportPanel.getByRole("button", { name: "Download project archive" }).click();
  const archive = await downloaded;
  assert.equal(archive.suggestedFilename(), "ARC.lific.tar.gz");
  const archivePath = join(scratch, archive.suggestedFilename());
  await archive.saveAs(archivePath);
  assert.equal(await archive.failure(), null);
  assert(gunzipSync(readFileSync(archivePath)).length > 0, "download is valid gzip");
  console.log("ok  source UI archive download");
  const destination = await signedPage(b.base, b.token);
  const observer = await signedPage(b.base, b.token);
  await t.test("real two-instance import transfers graph, comments, pages, files and privacy",async(round)=> {
  await destination.getByRole("link", { name: "Import project archive", exact: true }).click();
  await destination.getByLabel("Project archive (.tar.gz)", { exact: true }).setInputFiles(archivePath);
  await destination.getByRole("checkbox", { name: /I understand this imports/ }).check();
  let posts = 0;
  destination.on("request", (request) => {
    if (request.method() === "POST" && request.url().endsWith("/api/project-archives"))
      posts++;
  });
  const importResponse = destination.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/api/project-archives"));
  await destination.getByRole("button", { name: "Import as private project" }).click();
  const imported = await importResponse;
  assert.equal(imported.status(), 201, await imported.text());
  const result = await imported.json();
  await visible(destination, "ARC imported");
  const totalRows = Object.values(result.report.rows).reduce((sum, count) => sum + count, 0);
  const shownCounts = await destination.locator('dl dd').allTextContents();
  assert.equal(shownCounts.map(Number).reduce((sum,count)=>sum+count,0),totalRows);
  for (const [table,count] of Object.entries(result.report.rows)) {
    const entry=destination.locator('dl dt').filter({hasText:table.replaceAll('_',' ')}).first();
    assert.equal(await entry.locator('xpath=following-sibling::dd[1]').innerText(),String(count));
  }
  await visible(destination, '1 files imported.');
  await round.test("observer tab discovers the imported project",async()=>{await visible(observer,"Archive specimen");});
  assert.equal(posts, 1, "import is submitted once");
  await visible(destination, "unresolved reference");
  await visible(destination, "999999");
  assert(destination.url().endsWith("/projects/import"), "warnings stay on screen");
  await destination.setViewportSize({ width: 390, height: 844 });
  assert(await destination.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "mobile report overflows");
  const projects = await b.api("/projects");
  assert.equal(projects.length, 1);
  assert.equal(projects[0].is_public, false);
  assert.equal((await a.api("/projects"))[0].is_public, true, "source remains published and untouched");
  const relations = await b.api(`/projects/${projects[0].id}/relations`);
  assert.equal(relations.length, 1, "dependency edge transfers");
  await destination.getByRole("link", { name: "Open imported project" }).click();
  await visible(destination, "Archive specimen");
  await destination.goto(`${b.base}/ARC/issues/ARC-1`);
  await visible(destination, "Archive graph root");
  await visible(destination, "A comment carried across instances");
  await visible(destination, "archive-note.txt");
  await destination.locator("[data-attachment-id]").filter({hasText:"archive-note.txt"}).locator("[data-attachment-preview]").click();
  await visible(destination, "Archive attachment bytes");
  const attachmentDownload = destination.waitForEvent("download");
  await destination.locator("[data-attachment-id]").filter({hasText:"archive-note.txt"}).getByRole("link",{name:"archive-note.txt",exact:true}).click();
  const attachment = await attachmentDownload;
  assert.equal(readFileSync(await attachment.path(), "utf8"), "Archive attachment bytes");
  await destination.goto(`${b.base}/ARC/pages`);
  await destination.getByText("Archive notebook", { exact: true }).filter({ visible: true }).first().click();
  await destination.waitForURL(/\/ARC\/pages\/\d+$/);
  await destination.getByRole("textbox",{name:"Page title",exact:true}).waitFor();
  await destination.waitForFunction(()=>document.querySelector("[data-page-title]")?.value==="Archive notebook");
  await visible(destination, "A page carried across instances");
  console.log("ok  empty-instance import, counts, other-tab discovery, graph, comment, page, attachment, privacy, mobile report");
  });
  await t.test("duplicate identifiers, extension and server-sourced upload cap",async()=>{
  await destination.goto(`${b.base}/projects/import`);
  await destination.getByLabel("Project archive (.tar.gz)", { exact: true }).waitFor();
  await destination.getByLabel("Project archive (.tar.gz)", { exact: true }).setInputFiles(archivePath);
  await destination.getByRole("checkbox", { name: /I understand this imports/ }).check();
  await destination.getByRole("button", { name: "Import as private project" }).click();
  await visible(destination, "already exists");
  assert.equal((await b.api("/projects")).length, 1);
  await destination.getByLabel("Project archive (.tar.gz)", { exact: true }).setInputFiles({ name: "not-an-archive.txt", mimeType: "text/plain", buffer: Buffer.from("bad") });
  await visible(destination, "ending in .tar.gz");
  assert(await destination.getByRole("button", { name: "Import as private project" }).isDisabled());
  let invalidPosts = 0;
  destination.on("request", (request) => {
    if (request.method() === "POST" && request.url().endsWith("/api/project-archives"))
      invalidPosts++;
  });
  await destination.evaluate((max) => {
    const file = new File(["size-probe"], "too-large.tar.gz", { type: "application/gzip" });
    Object.defineProperty(file, "size", { value: max + 1 });
    const transfer = new DataTransfer;
    transfer.items.add(file);
    const input = document.querySelector('input[name="archive"]');
    input.files = transfer.files;
    input.dispatchEvent(new Event("change", { bubbles: true }));
  }, caps.max_upload_bytes);
  await visible(destination, "web upload limit");
  assert(await destination.getByRole("button", { name: "Import as private project" }).isDisabled());
  assert.equal(invalidPosts, 0);
  assert(await destination.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "mobile form overflows");
  console.log("ok  duplicate ID, invalid extension, server-sourced browser size cap");
  });
  await t.test("in-flight navigation, duplicate submit and lost response survive reload",async()=>{
  await destination.goto(`${b.base}/projects/import`);
  await destination.getByLabel("Project archive (.tar.gz)", { exact: true }).setInputFiles(archivePath);
  await destination.getByRole("checkbox", { name: /I understand this imports/ }).check();
  let lostPosts = 0;
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  await destination.route("**/api/project-archives", async (route) => {
    if (route.request().method() !== "POST")
      return route.continue();
    lostPosts++;
    await held;
    return route.abort("failed");
  });
  await destination.getByRole("button", { name: "Import as private project" }).click();
  await destination.getByRole("button",{name:"Import as private project",exact:true}).waitFor();
  assert(await destination.getByRole("button",{name:"Import as private project",exact:true}).isDisabled());
  assert(await destination.getByRole("button", { name: "Import as private project" }).isDisabled());
  await destination.goto(`${b.base}/projects/new`);
  await destination.getByRole("link", { name: "Import project archive", exact: true }).click();
  await visible(destination, "Import in progress");
  release();
  await visible(destination, "Check the project list before importing again.");
  assert.equal(await destination.getByRole("button", { name: "Import as private project" }).count(), 0);
  await destination.reload();
  await visible(destination, "Check the project list before importing again.");
  assert.equal(lostPosts, 1);
  await destination.unroute("**/api/project-archives");
  console.log("ok  in-flight navigation and duplicate-submit guard, lost response remains unknown after reload, no automatic retry");
  });
  await t.test("regular user controls, server denial and mobile export",async()=>{
  const regularToken = await b.login("regular");
  const regular = await signedPage(b.base, regularToken, 390);
  await visible(regular, "Create project");
  assert.equal(await regular.getByRole("link", { name: "Import project archive", exact: true }).count(), 0);
  await regular.goto(`${b.base}/projects/import`);
  await visible(regular, "Only a signed-in instance admin");
  assert.equal(await regular.locator('input[name="archive"]').count(), 0);
  const deniedForm = new FormData;
  deniedForm.append("archive", new File([readFileSync(archivePath)], "ARC.lific.tar.gz", { type: "application/gzip" }));
  const denied = await fetch(`${b.base}/api/project-archives`, { method: "POST", headers: { Authorization: `Bearer ${regularToken}` }, body: deniedForm });
  assert.equal(denied.status, 403, "non-admin archive POST is refused in legacy mode");
  await regular.goto(`${b.base}/ARC/overview`);
  await visible(regular, "Archive specimen");
  assert.equal(await regular.getByRole("button", { name: "Download project archive" }).count(), 0);
  assert(await regular.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "mobile overview overflows");
  await source.setViewportSize({ width: 390, height: 844 });
  await exportPanel.getByRole("button", { name: "Download project archive" }).waitFor();
  assert(await source.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "mobile export overflows");
  });
  await t.test("stale exports abort on project navigation and session replacement",async()=>{
  await source.setViewportSize({ width: 1280, height: 900 });
  await a.api("/projects", "POST", { name: "Other project", identifier: "OTHER" });
  await staleDownload(source, a.base, async () => {
    await source.evaluate(() => {
      window.location.assign(window.LificTopcoatRouting.href("/OTHER/overview"));
    });
    await visible(source, "Other project");
  });
  const sourceSecondToken = await a.login("second-admin");
  await staleDownload(source, a.base, () => changeSession(source, a.base, sourceSecondToken));
  const renewedSourceToken = await a.login("second-admin");
  await staleDownload(source, a.base, () => changeSession(source, a.base, renewedSourceToken));
  console.log("ok  delayed export aborts on project navigation, account change and same-user session replacement, no stale download event");
  });
  await t.test("pending imports remain isolated by user and survive refresh and logout",async()=>{
  await observer.context().close();
  const firstUser = await b.api("/auth/me");
  const secondToken = await b.login("second-admin");
  const secondUser = await (await fetch(`${b.base}/api/auth/me`, { headers: { Authorization: `Bearer ${secondToken}` } })).json();
  const firstKey = `lific:topcoat:archive-pending:private:${firstUser.id}`;
  const secondKey = `lific:topcoat:archive-pending:private:${secondUser.id}`;
  await visible(destination, "Check the project list before importing again.");
  await changeSession(destination, b.base, secondToken);
  await destination.locator('input[name="archive"]').waitFor();
  assert.equal(await destination.getByText("Check the project list before importing again.", { exact: false }).count(), 0);
  assert.equal(await destination.evaluate((key) => sessionStorage.getItem(key), firstKey), "1");
  await destination.getByLabel("Project archive (.tar.gz)", { exact: true }).setInputFiles(archivePath);
  await destination.getByRole("checkbox", { name: /I understand this imports/ }).check();
  await a.api('/issues','POST',{project_id:(await a.api('/projects')).find(project=>project.identifier==='OTHER').id,title:'Old account private issue',description:'Missing source /api/attachments/888888'});
  const oldArchive=await fetch(`${a.base}/api/project-archives/OTHER`,{headers:{Authorization:`Bearer ${a.token}`}});
  assert.equal(oldArchive.status,200);
  const otherArchivePath=join(scratch,'OTHER.lific.tar.gz');
  const {writeFileSync}=await import('node:fs');writeFileSync(otherArchivePath,Buffer.from(await oldArchive.arrayBuffer()));
  await destination.getByLabel('Project archive (.tar.gz)',{exact:true}).setInputFiles(otherArchivePath);
  await destination.getByRole('checkbox',{name:/I understand this imports/}).check();
  let releaseOld, reachedOld;
  const oldHeld = new Promise((resolve) => {
    releaseOld = resolve;
  });
  const oldStarted = new Promise((resolve) => {
    reachedOld = resolve;
  });
  await destination.route("**/api/project-archives", async (route) => {
    if (route.request().method() !== "POST")
      return route.continue();
    const response=await route.fetch();assert.equal(response.status(),201);
    reachedOld();
    await oldHeld;
    await route.fulfill({response});
  });
  await destination.getByRole("button", { name: "Import as private project" }).click();
  await oldStarted;
  await visible(destination, "Import in progress");
  await changeSession(destination, b.base, b.token);
  await visible(destination, "Check the project list before importing again.");
  releaseOld();
  await destination.waitForTimeout(250);
  assert.equal(await destination.getByText("OTHER", { exact: false }).count(), 0);
  assert.equal(await destination.getByText("source attachment ID 888888", { exact: false }).count(), 0);
  assert.equal(await destination.getByText("Import in progress", { exact: true }).count(), 0);
  await destination.unroute("**/api/project-archives");
  await destination.getByRole("button", { name: "I've checked the project list" }).click();
  assert.equal(await destination.evaluate((key) => sessionStorage.getItem(key), firstKey), null);
  assert.equal(await destination.evaluate((key) => sessionStorage.getItem(key), secondKey), "1");
  await changeSession(destination, b.base, secondToken);
  await visible(destination, "Check the project list before importing again.");
  const refreshed = await fetch(`${b.base}/api/auth/me/refresh`, { method: "POST", headers: { Authorization: `Bearer ${secondToken}`, "Content-Type": "application/json" }, body: JSON.stringify({ password: PASSWORD }) });
  assert.equal(refreshed.status, 200);
  const freshToken = (await refreshed.json()).token;
  assert.notEqual(freshToken, secondToken);
  await changeSession(destination, b.base, freshToken);
  await visible(destination, "Check the project list before importing again.");
  await destination.reload();
  await visible(destination, "Check the project list before importing again.");
  assert.equal(await destination.evaluate((key) => sessionStorage.getItem(key), secondKey), "1");
  await destination.evaluate(() => {
    localStorage.removeItem("lific_token");
    window.dispatchEvent(new Event("lific:session-change"));
  });
  await destination.getByText("Sign in", { exact: true }).waitFor();
  assert.equal(await destination.getByText("Check the project list before importing again.", { exact: false }).count(), 0);
  assert.equal(await destination.evaluate((key) => sessionStorage.getItem(key), secondKey), "1");
  console.log("ok  pending warnings isolated by user ID, old-account progress/report hidden, own-only reset, session-refresh and logout retention");
  });
  assert.equal(errors.length, 0, errors.join(`
`));
  console.log("ok  regular-user controls hidden and POST denied in legacy mode, mobile export, no unexpected console errors");
}
test("master project archives: real two-instance roundtrip, validation, permissions, cancellation and account isolation", { timeout: 180000 }, async (t) => {
  try {
    await main(t);
  } finally {
    await finish(0);
  }
});
