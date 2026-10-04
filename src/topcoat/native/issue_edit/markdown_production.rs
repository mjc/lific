//! Normal production issue reads and actions through the actual server and browser.

use std::{process::Stdio, time::Duration};

use rusqlite::OptionalExtension;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use super::super::home_fixture;
use crate::{
    actor::{ActorCtx, Transport},
    db::{
        models::{CreateIssue, Priority, Role, Status, UpdateIssue},
        queries,
    },
};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Control {
    Prepare { id: usize, source: String },
    Snapshot { id: usize },
    RemoteDescription { id: usize, source: String },
    Demote { id: usize },
    Relations { id: usize, stage: String },
}

// Each stage has distinct targets in all five displayed relation directions.
fn relation_graph(db: &crate::db::DbPool, issue_id: i64, project_id: i64, actor: i64, stage: &str) {
    let conn = db.write().unwrap();
    let hidden = queries::resolve_identifier(&conn, "HIDE-1").unwrap();
    let hidden_project = queries::get_issue(&conn, hidden).unwrap().project_id;
    if stage == "initial" {
        conn.execute(
            "DELETE FROM issue_relations WHERE source_id = ?1 OR target_id = ?1",
            [issue_id],
        )
        .unwrap();
        queries::members::upsert_member(&conn, hidden_project, actor, Role::Viewer).unwrap();
    } else {
        // Keep stored hidden edges; only their current authorized projection changes.
        if queries::members::get_member_role(&conn, hidden_project, actor)
            .unwrap()
            .is_some()
        {
            queries::members::remove_member(&conn, hidden_project, actor).unwrap();
        }
        conn.execute(
            "DELETE FROM issue_relations WHERE (source_id = ?1 AND target_id IN (SELECT id FROM issues WHERE project_id = ?2)) OR (target_id = ?1 AND source_id IN (SELECT id FROM issues WHERE project_id = ?2))",
            rusqlite::params![issue_id, project_id],
        ).unwrap();
    }
    for (index, (kind, reverse)) in [
        ("blocks", false),
        ("blocks", true),
        ("relates_to", false),
        ("duplicate", false),
        ("duplicate", true),
    ]
    .into_iter()
    .enumerate()
    {
        let title = format!("Relation {stage} {index}");
        let target = conn
            .query_row(
                "SELECT id FROM issues WHERE project_id = ?1 AND title = ?2",
                rusqlite::params![project_id, title],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .unwrap()
            .unwrap_or_else(|| {
                queries::create_issue(
                    &conn,
                    &CreateIssue {
                        project_id,
                        title,
                        ..Default::default()
                    },
                )
                .unwrap()
                .id
            });
        let (source, target) = if reverse {
            (target, issue_id)
        } else {
            (issue_id, target)
        };
        queries::link_issues(&conn, source, target, kind).unwrap();
        if stage == "initial" {
            let title = format!("Hidden relation {index}");
            let hidden_target = conn
                .query_row(
                    "SELECT id FROM issues WHERE project_id = ?1 AND title = ?2",
                    rusqlite::params![hidden_project, title],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .unwrap()
                .unwrap_or_else(|| {
                    queries::create_issue(
                        &conn,
                        &CreateIssue {
                            project_id: hidden_project,
                            title,
                            ..Default::default()
                        },
                    )
                    .unwrap()
                    .id
                });
            let (source, target) = if reverse {
                (hidden_target, issue_id)
            } else {
                (issue_id, hidden_target)
            };
            queries::link_issues(&conn, source, target, kind).unwrap();
        }
    }
}

async fn browser(scenario: &str) {
    let fixture = home_fixture::fixture();
    let (actor, admin, issue_id, project_id, first_audit) = {
        let conn = fixture.db.read().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let admin = queries::users::get_user_by_username(&conn, "admin").unwrap();
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, issue_id).unwrap();
        let first_audit: i64 = conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM audit_log", [], |row| {
                row.get(0)
            })
            .unwrap();
        (actor, admin, issue_id, issue.project_id, first_audit)
    };
    let identity = Some(crate::auth::fresh_identity(&admin, Transport::Web));
    let script_dir = tempfile::tempdir().unwrap();
    let script = script_dir.path().join("markdown.browser.test.cjs");
    std::fs::write(&script, DRIVER).unwrap();
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/topcoat/native/browser_fixture.cjs");
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command =
        home_fixture::browser_command(script.to_str().unwrap(), &origin, &fixture.token);
    command
        .arg(helper)
        .arg(scenario)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    });
    let mut transcript = String::new();
    let result = tokio::time::timeout(Duration::from_secs(150), async {
        while let Some(line) = stdout.next_line().await.unwrap() {
            let Some(message) = line.strip_prefix("@lific-fixture:issue-markdown:") else {
                transcript.push_str(&line);
                transcript.push('\n');
                continue;
            };
            let control = serde_json::from_str::<Control>(message).unwrap();
            let id = match control {
                Control::Prepare { id, source } => {
                    queries::members::upsert_member(
                        &fixture.db.write().unwrap(), project_id, actor.id, Role::Maintainer,
                    ).unwrap();
                    crate::actor::scope(
                        ActorCtx { user_id: Some(admin.id), transport: Transport::Web },
                        async {
                            let issue = queries::get_issue(&fixture.db.read().unwrap(), issue_id).unwrap();
                            crate::services::issues::commit_issue_update(
                                &fixture.db, &fixture.realtime, &identity, issue_id,
                                UpdateIssue {
                                    description: Some(source), status: Some(Status::Active),
                                    priority: Some(Priority::Medium), expected_seq: Some(issue.seq),
                                    ..Default::default()
                                },
                            ).unwrap();
                        },
                    ).await;
                    id
                }
                Control::RemoteDescription { id, source } => {
                    crate::actor::scope(
                        ActorCtx { user_id: Some(admin.id), transport: Transport::Web },
                        async {
                            let issue = queries::get_issue(&fixture.db.read().unwrap(), issue_id).unwrap();
                            crate::services::issues::commit_issue_update(
                                &fixture.db, &fixture.realtime, &identity, issue_id,
                                UpdateIssue { description: Some(source), expected_seq: Some(issue.seq),
                                    ..Default::default() },
                            ).unwrap();
                        },
                    ).await;
                    id
                }
                Control::Demote { id } => {
                    // No browser storage/cookie change and no fabricated render:
                    // the next real native procedure must see current membership.
                    queries::members::upsert_member(
                        &fixture.db.write().unwrap(), project_id, actor.id, Role::Viewer,
                    ).unwrap();
                    assert_eq!(queries::members::get_member_role(
                        &fixture.db.read().unwrap(), project_id, actor.id,
                    ).unwrap(), Some(Role::Viewer));
                    id
                }
                Control::Relations { id, stage } => {
                    relation_graph(&fixture.db, issue_id, project_id, actor.id, &stage);
                    id
                }
                Control::Snapshot { id } => id,
            };
            let answer = {
            let reader = Some(crate::auth::fresh_identity(&actor, Transport::Web));
            let scoped = crate::services::issues::resolve_issue(&fixture.db, &reader, "ACC-1").unwrap();
            let relations = |issue: &crate::db::models::Issue| serde_json::json!({
                "Blocked by": issue.blocked_by, "Blocks": issue.blocks, "Related": issue.relates_to,
                "Duplicate of": issue.duplicates, "Duplicated by": issue.duplicated_by,
            });
            let conn = fixture.db.read().unwrap();
            let issue = queries::get_issue(&conn, issue_id).unwrap();
            let count = |field: &str| -> i64 {
                conn.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE id > ?1 AND entity_type = 'issue'
                     AND entity_id = ?2 AND field = ?3 AND actor_user_id = ?4 AND transport = 'web'",
                    rusqlite::params![first_audit, issue_id, field, actor.id], |row| row.get(0),
                ).unwrap()
            };
            serde_json::json!({
                "id": id, "seq": issue.seq, "title": issue.title, "description": issue.description,
                "priority": issue.priority.as_str(), "titleWrites": count("title"),
                "descriptionWrites": count("description"), "priorityWrites": count("priority"),
                "relations": relations(&scoped), "rawRelations": relations(&issue),
            })
            };
            stdin.write_all(format!("{answer}\n").as_bytes()).await.unwrap();
        }
        child.wait().await.unwrap()
    }).await;
    server.abort();
    match result {
        Ok(status) => assert!(status.success(), "{transcript}\n{}", errors.await.unwrap()),
        Err(timeout) => {
            let cleanup = child.kill().await;
            let errors = errors.await.unwrap();
            panic!(
                "normal issue {scenario} timed out: {timeout}; cleanup={cleanup:?}\n{transcript}\n{errors}"
            );
        }
    }
}

#[tokio::test]
async fn native_issue_production_browser_markdown_save_refreshes_dom_and_exact_web_authored_source()
{
    browser("save").await;
}

#[tokio::test]
async fn native_issue_production_browser_metadata_refresh_keeps_dirty_body_and_owning_focus() {
    browser("dirty").await;
}

#[tokio::test]
async fn native_issue_production_browser_current_viewer_role_rejects_stale_editor_and_reloads_readonly()
 {
    browser("role").await;
}

#[tokio::test]
async fn native_issue_production_browser_empty_description_autofocuses_and_can_reenter_after_clear()
{
    browser("empty").await;
}

#[tokio::test]
async fn native_issue_production_browser_metadata_menus_dismiss_on_outside_click() {
    browser("menus").await;
}

#[tokio::test]
async fn native_issue_production_browser_save_replaces_all_current_authorized_relation_links() {
    browser("relations_saved").await;
}

#[tokio::test]
async fn native_issue_production_browser_conflict_relations_match_captured_winner_and_keep_dirty_body()
 {
    browser("relations_conflict").await;
}

const DRIVER: &str = r###"
// Normal production document; fixture controls never use application HTTP.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const readline = require('node:readline');
const {createHash} = require('node:crypto');
const {mountedProxy, launchBrowser} = require(process.argv[4]);
const upstream = new URL(process.argv[2]), token = process.argv[3], scenario = process.argv[5];
const pending = new Map();
let next = 0;
const input = readline.createInterface({input: process.stdin});
input.on('line', line => {
  const answer = JSON.parse(line), request = pending.get(answer.id);
  if (request) {clearTimeout(request.timer); pending.delete(answer.id); request.resolve(answer);}
});
function control(action, fields = {}) {
  const id = ++next;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {pending.delete(id); reject(new Error(`fixture ${action} acknowledgement`));}, 10000);
    pending.set(id, {resolve, reject, timer});
    process.stdout.write(`@lific-fixture:issue-markdown:${JSON.stringify({id, action, ...fields})}\n`);
  });
}
const editor = '[data-native-issue-editor="ACC-1"]';
const body = '#native-issue-body-input-ACC-1', edit = '#native-issue-body-edit-ACC-1';
const title = '#native-issue-title-ACC-1', titleInput = '#native-issue-title-input-ACC-1';
const preview = '.native-issue-editor__preview';
const seq = async page => Number(await page.locator('[data-native-issue-seq]').textContent());
function sequence(outcome) {
  const option = outcome[1];
  assert.equal(option.t,'Option');
  assert.equal(option.v.t,'i64');
  assert.equal(option.v.bits,64);
  assert.match(option.v.v,/^[0-9]+$/);
  const value = Number(option.v.v);
  assert.ok(Number.isSafeInteger(value),'Fixture cursor must remain exact when compared with JSON DB control replies.');
  return value;
}
async function waitForBodyFocus(page) {
  await page.waitForFunction(() => document.activeElement === document.querySelector('#native-issue-body-input-ACC-1'));
}
async function startBody(page) {
  await page.locator(edit).click();
  await page.locator(body).waitFor({state:'visible'});
  await waitForBodyFocus(page);
}
async function procedure(page, field, action, expected) {
  const responsePromise = page.waitForResponse(response => response.request().method() === 'POST' &&
    new URL(response.url()).pathname.endsWith(`/__native_issue_edit/save/${field}`));
  await action();
  const response = await responsePromise;
  assert.equal(response.status(), 200);
  const outcome = await response.json();
  assert.equal(outcome[0][expected === 'saved' ? 'ok' : 'err'], expected);
  return outcome;
}
async function idle(page) {
  await page.waitForFunction(() => ![...document.querySelectorAll('[role="status"]')]
    .some(element => !element.hidden && element.textContent === 'Saving…'));
}
// Hold a genuine procedure until focus is returned to the body. No response is
// mocked; releasing the request exercises its real cookie/role/transaction.
async function metadataWithBodyFocus(page, proxy, prefix, expected) {
  const url = `${proxy.origin}${prefix}/__native_issue_edit/save/priority`;
  let release;
  const held = new Promise(resolve => {release = resolve;});
  let seen;
  const entered = new Promise(resolve => {seen = resolve;});
  const intercept = async route => {seen(); await held; await route.continue();};
  await page.route(url, intercept);
  try {
    await page.getByRole('button',{name:'Change issue priority',exact:true}).click();
    const outcome = procedure(page, 'priority', () => page.locator('[data-native-issue-priority-option="low"]').click(), expected);
    await Promise.race([entered, outcome.then(() => assert.fail('Procedure completed without entering its actual hold.'))]);
    await page.locator(body).focus();
    release();
    const result = await outcome;
    await idle(page);
    return result;
  } finally {release(); await page.unroute(url, intercept);}
}
test(`normal production issue Markdown ${scenario}`, async t => {
  const browser = await launchBrowser();
  try {
    for (const prefix of ['', '/app', '/ACC']) await t.test(prefix || 'root', async () => {
      const initial = scenario==='empty' ? '' : '# Production heading\n\nInitial **safe** paragraph.\n\n<script>window.nativeHostile=1</script>\n<a href="javascript:window.nativeHostile=1">Unsafe link</a>';
      const baseline = await control('prepare', {source: initial});
      const initialRelations = scenario.startsWith('relations_') ? await control('relations', {stage:'initial'}) : null;
      const proxy = await mountedProxy(upstream, prefix);
      let context;
      try {
        context = await browser.newContext({viewport:{width:1440,height:900}, reducedMotion:'reduce'});
        await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
        await context.addInitScript(() => {
          window.issueViolations = [];
          document.addEventListener('securitypolicyviolation', event => window.issueViolations.push(event.effectiveDirective));
        });
        const requests = [], errors = [], dialogs = [], failures = [];
        context.on('request', request => requests.push({url:request.url(), authorization:!!request.headers().authorization}));
        const page = await context.newPage();
        page.setDefaultTimeout(15000);
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => {if(message.type()==='error') errors.push(message.text());});
        page.on('requestfailed', request => failures.push({url:request.url(),failure:request.failure()}));
        page.on('dialog', dialog => {dialogs.push(dialog.type()); dialog.dismiss();});
        const cssPromise = page.waitForResponse(response => response.request().resourceType()==='stylesheet' &&
          new URL(response.url()).pathname===`${prefix}/__topcoat-app.css`);
        const response = await page.goto(`${proxy.origin}${prefix}/ACC/issues/ACC-1`);
        assert.equal(response.status(), 200);
        await page.locator(editor).waitFor();
        if (scenario==='empty') {
          await page.locator(body).waitFor({state:'visible'});
          await waitForBodyFocus(page);
        } else {
          await page.locator(`${preview} h1`).waitFor();
          assert.equal(await page.locator(`${preview} h1`).textContent(), 'Production heading');
          assert.equal(await page.locator(`${preview} p strong`).textContent(), 'safe');
        }
        assert.equal(await page.locator(`${preview} script, ${preview} [onerror], ${preview} a[href^="javascript:"]`).count(), 0);
        assert.equal(await page.evaluate(() => window.nativeHostile), undefined);
        const css = await cssPromise;
        assert.equal(css.status(), 200);
        assert.equal(css.headers()['content-type'],'text/css; charset=utf-8');
        assert.equal(css.headers()['cache-control'],'no-cache');
        assert.equal(new URL(css.url()).searchParams.get('v'), createHash('sha256').update(await css.body()).digest('hex'));
        assert.equal(new URL(css.url()).pathname, `${prefix}/__topcoat-app.css`);
        const styles = await page.locator(editor).evaluate(element => ({
          display:getComputedStyle(element).display, padding:getComputedStyle(element).paddingTop,
          proseWhitespace:getComputedStyle(element.querySelector('.tc-markdown')).whiteSpace,
          contentPadding:getComputedStyle(element.querySelector('.native-issue-editor__content')).paddingTop,
          fieldsWidth:getComputedStyle(element.querySelector('.native-issue-editor__fields')).width,
          rules:document.querySelector('link[rel="stylesheet"]').sheet.cssRules.length,
        }));
        // Pinned DocumentDetail uses a flex document and a docked 220px aside.
        // The separate paired screenshot test compares the exact master boxes.
        assert.equal(styles.display,'flex'); assert.equal(styles.padding,'0px');
        assert.equal(styles.contentPadding,'24px'); assert.equal(styles.fieldsWidth,'220px');
        assert.ok(styles.rules>0);
        assert.equal(styles.proseWhitespace,'normal','Markdown prose must not inherit the source preview whitespace.');
        assert.equal(await page.locator('script[src]').count(),1);
        assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')),null);
        if (scenario!=='empty') await startBody(page);
        assert.equal(await page.locator(body).inputValue(),initial,'Actual textarea retains exact unsanitized author source.');

        if (scenario.startsWith('relations_')) {
          const groups = async () => page.locator('.native-issue-editor__relations').evaluateAll(sections =>
            Object.fromEntries(sections.map(section => [section.querySelector('h2').textContent,
              [...section.querySelectorAll('a')].map(link => ({identifier:link.textContent,href:link.getAttribute('href')}))])));
          const expected = snapshot => Object.fromEntries(Object.entries(snapshot.relations)
            .filter(([,values])=>values.length).map(([label,values])=>[label,values.map(identifier=>({identifier,
              href:`${prefix}/${identifier.split('-')[0]}/issues/${identifier}`}))]));
          // Field state changes synchronously; its authorized relation shard
          // completes in a separate real framework render. Await that exact DOM.
          const waitForRelations = wanted => page.waitForFunction(expected => {
            const sections=[...document.querySelectorAll('.native-issue-editor__relations')];
            if(sections.length!==Object.keys(expected).length)return false;
            return sections.every(section=>{
              const wanted=expected[section.querySelector('h2').textContent];
              const links=[...section.querySelectorAll('a')];
              return wanted && links.length===wanted.length && links.every((link,index)=>
                link.textContent===wanted[index].identifier && link.getAttribute('href')===wanted[index].href);
            });
          },wanted);
          assert.deepEqual(await groups(),expected(initialRelations),'Initial normal-route DOM contains all five authorized relation directions.');
          const draft=`  Dirty relations ${prefix || 'root'}\n\n  exact author whitespace  \n`;
          await page.locator(body).fill(draft);
          const observed=await seq(page);
          const changed=await control('relations',{stage:'updated'});
          assert.equal(changed.seq,observed,'Relation-only fixture mutation permits a genuine successful field Save.');
          for (const label of Object.keys(changed.relations)) {
            assert.equal(changed.rawRelations[label].some(identifier=>identifier.startsWith('HIDE-')),true,'Hidden edge remains stored.');
            assert.equal(changed.relations[label].some(identifier=>identifier.startsWith('HIDE-')),false,'Current reader loses hidden-project visibility.');
          }
          if (scenario==='relations_saved') {
            const saved=await procedure(page,'description',()=>page.locator('[data-native-issue-body-save]').click(),'saved');
            await idle(page);await page.locator(body).waitFor({state:'hidden'});
            const stored=await control('snapshot');
            assert.equal(await seq(page),sequence(saved));assert.equal(stored.seq,sequence(saved));
            assert.equal(stored.description,draft);assert.equal(stored.descriptionWrites,baseline.descriptionWrites+1);
            await waitForRelations(expected(stored));
            assert.deepEqual(await groups(),expected(stored),'Successful Save replaces every relation direction with its authorized snapshot.');
          } else {
            const winner=await control('remote_description',{source:`# Captured winner ${prefix || 'root'}\n\nWinner source.`});
            assert.ok(winner.seq>observed);
            const url=`${proxy.origin}${prefix}/__native_issue_edit/save/description`;
            let captured, later;
            const intercept=async route=>{
              const response=await route.fetch();
              assert.equal(response.status(),200);
              captured=await response.json();
              assert.equal(captured[0].err,'conflict');assert.equal(sequence(captured),winner.seq);
              await control('relations',{stage:'later'});
              later=await control('remote_description',{source:`# Later winner ${prefix || 'root'}\n\nLater source.`});
              assert.ok(later.seq>winner.seq);
              // Deliver the actual production response after a newer DB winner exists.
              await route.fulfill({response});
            };
            await page.route(url,intercept);
            try {
              const conflict=await procedure(page,'description',()=>page.locator('[data-native-issue-body-save]').click(),'conflict');
              assert.deepEqual(conflict,captured,'Browser receives the original complete production response.');
              await idle(page);
              assert.equal(await seq(page),winner.seq,'Conflict sequence is the captured winner, even after a later commit.');
              await page.waitForFunction(heading=>document.querySelector('.native-issue-editor__preview h1')?.textContent===heading,`Captured winner ${prefix || 'root'}`);
              assert.equal(await page.locator(title).textContent(),winner.title);
              assert.equal(await page.locator(body).inputValue(),draft);
              assert.equal(await page.locator(body).isVisible(),true);await waitForBodyFocus(page);
              await page.locator('[data-native-issue-save-error]').filter({hasText:'This issue changed. Your draft is still here.'}).waitFor();
              await waitForRelations(expected(winner));
              assert.deepEqual(await groups(),expected(winner),'Conflict relations belong to the captured winner, not a later DB read.');
              const stored=await control('snapshot');
              assert.equal(stored.seq,later.seq);assert.equal(stored.description,later.description);
              assert.equal(stored.descriptionWrites,baseline.descriptionWrites,'Losing native Save never writes its dirty body.');
            } finally {await page.unroute(url,intercept);}
          }
          assert.equal(await page.locator('.native-issue-editor__relations a').filter({hasText:'HIDE-'}).count(),0,
            'Revoked relation disappears from active DOM without changing the verified session cookie.');
        } else if (scenario==='menus') {
          await page.locator('[data-native-issue-body-cancel]').click();
          await page.locator(body).waitFor({state:'hidden'});
          const status=page.getByRole('button',{name:'Change issue status',exact:true});
          const priority=page.getByRole('button',{name:'Change issue priority',exact:true});
          const headerStatus=page.getByTitle('Change status',{exact:true});
          await headerStatus.click();
          assert.equal(await headerStatus.getAttribute('aria-expanded'),'true');
          await priority.click();
          assert.equal(await headerStatus.getAttribute('aria-expanded'),'false','Opening a peer menu closes the previous picker.');
          assert.equal(await priority.getAttribute('aria-expanded'),'true');
          await page.locator(`${preview} p`).first().click();
          assert.equal(await priority.getAttribute('aria-expanded'),'false','Actual outside clicks dismiss the priority menu.');
          await status.click();
          await page.locator(title).click();
          assert.equal(await status.getAttribute('aria-expanded'),'false','Actual outside clicks dismiss the status menu.');
          const stored=await control('snapshot');
          assert.equal(stored.seq,baseline.seq);assert.equal(stored.priorityWrites,baseline.priorityWrites);
        } else if (scenario==='empty') {
          const added='# Added description\n\nActual initial author text.';
          await page.locator(body).fill(added);
          const saved=await procedure(page,'description',()=>page.locator(body).press('Control+s'),'saved');
          await idle(page);
          await page.locator(`${preview} h1`).waitFor();
          assert.equal(await page.locator(`${preview} h1`).textContent(),'Added description');
          let stored=await control('snapshot');
          assert.equal(stored.description,added);assert.equal(stored.seq,sequence(saved));
          assert.equal(stored.descriptionWrites,baseline.descriptionWrites+1);
          await startBody(page);
          await page.locator(body).fill('');
          const cleared=await procedure(page,'description',()=>page.locator(body).press('Control+s'),'saved');
          await idle(page);
          await page.locator(body).waitFor({state:'hidden'});
          await page.getByText('Click to add a description...', {exact:true}).click();
          await page.locator(body).waitFor({state:'visible'});
          await waitForBodyFocus(page);
          assert.equal(await page.locator(body).inputValue(),'');
          assert.equal(await page.locator(body).evaluate(element=>element===document.activeElement),true);
          stored=await control('snapshot');
          assert.equal(stored.description,'');assert.equal(stored.seq,sequence(cleared));
          assert.equal(stored.descriptionWrites,baseline.descriptionWrites+2);
        } else if (scenario==='save') {
          const source = `# Saved ${prefix || 'root'}\n\nUpdated **paragraph**.\n\n  exact trailing spaces  \n`;
          await page.locator(body).fill(source);
          const before = await seq(page);
          const saved = await procedure(page,'description',()=>page.locator(body).press('Control+s'),'saved');
          await idle(page);
          await page.locator(body).waitFor({state:'hidden'});
          await page.waitForFunction(heading => document.querySelector('.native-issue-editor__preview h1')?.textContent===heading,
            `Saved ${prefix || 'root'}`);
          assert.equal(await page.locator(`${preview} p strong`).textContent(),'paragraph');
          assert.ok(sequence(saved)>before,'A committed field advances the global sequence.');
          assert.equal(await seq(page),sequence(saved));
          const stored = await control('snapshot');
          assert.equal(stored.description,source); assert.equal(stored.seq,sequence(saved));
          assert.equal(stored.descriptionWrites,baseline.descriptionWrites+1,'Exactly one committed description has verified Web attribution.');
          await page.reload();
          await page.locator(`${preview} h1`).waitFor();
          assert.equal(await page.locator(`${preview} h1`).textContent(),`Saved ${prefix || 'root'}`);
          await startBody(page);
          assert.equal(await page.locator(body).inputValue(),source,'Reload reopens exact persisted Markdown source.');
        } else if (scenario==='dirty') {
          const draft = `  # Unsaved ${prefix || 'root'}\n\n  whitespace stays exact  \n`;
          await page.locator(body).fill(draft);
          const winner = `# Remote ${prefix || 'root'}\n\nAuthoritative **winner** paragraph.`;
          const committed = await control('remote_description',{source:winner});
          const conflict = await metadataWithBodyFocus(page,proxy,prefix,'conflict');
          assert.equal(sequence(conflict),committed.seq);
          await page.waitForFunction(heading => document.querySelector('.native-issue-editor__preview h1')?.textContent===heading,
            `Remote ${prefix || 'root'}`);
          assert.equal(await seq(page),committed.seq);
          assert.equal(await page.locator(body).inputValue(),draft);
          assert.equal(await page.locator(body).isVisible(),true);
          assert.equal(await page.locator(body).evaluate(element=>element===document.activeElement),true);
          assert.equal(await page.locator(`${preview} p strong`).textContent(),'winner');
          const saved = await metadataWithBodyFocus(page,proxy,prefix,'saved');
          assert.ok(sequence(saved)>committed.seq);
          assert.equal(await seq(page),sequence(saved));
          assert.equal(await page.locator(body).inputValue(),draft);
          assert.equal(await page.locator(body).evaluate(element=>element===document.activeElement),true);
          const stored = await control('snapshot');
          assert.equal(stored.description,winner,'Metadata saves cannot commit the dirty body.');
          assert.equal(stored.descriptionWrites,baseline.descriptionWrites);
          assert.equal(stored.priorityWrites,baseline.priorityWrites+1);
          assert.equal(stored.priority,'low');
          assert.equal(stored.seq,sequence(saved));
        } else if (scenario==='role') {
          await page.locator('[data-native-issue-body-cancel]').click();
          await control('demote');
          await page.locator(title).click();
          await page.locator(titleInput).waitFor({state:'visible'});
          const attempted='Rejected stale editor title';
          await page.locator(titleInput).fill(attempted);
          await procedure(page,'title',()=>page.locator(titleInput).press('Enter'),'forbidden');
          await idle(page);
          await page.locator('[data-native-issue-save-error]').waitFor({state:'visible'});
          assert.equal(await page.locator(titleInput).inputValue(),attempted);
          const stored=await control('snapshot');
          assert.equal(stored.seq,baseline.seq); assert.equal(stored.titleWrites,baseline.titleWrites);
          await page.reload();
          await page.locator(`${preview} h1`).waitFor();
          assert.equal(await page.locator(`${preview} h1`).textContent(),'Production heading');
          for (const selector of ['input[aria-label="Issue title"]','textarea[aria-label="Issue description"]',
            '[data-native-issue-body-save]','[data-native-issue-status-option]','[data-native-issue-priority-option]'])
            assert.equal(await page.locator(selector).count(),0,'Fresh Viewer document is read-only.');
        } else assert.fail(`Unknown scenario ${scenario}`);
        assert.equal(requests.some(request=>new URL(request.url).pathname.split('/').includes('api')),false,
          'Capture every context request, including absolute and unmounted REST attempts.');
        assert.equal(requests.some(request=>request.authorization),false);
        assert.deepEqual(await page.evaluate(()=>window.issueViolations),[]);
        assert.deepEqual(errors,[]); assert.deepEqual(dialogs,[]); assert.deepEqual(failures,[]);
      } finally {
        try {if(context) await context.close();} finally {await proxy.close();}
      }
    });
  } finally {
    try {await browser.close();} finally {
      input.close();
      for(const request of pending.values()) {clearTimeout(request.timer); request.reject(new Error('Browser fixture disposed.'));}
      pending.clear();
    }
  }
});
"###;
