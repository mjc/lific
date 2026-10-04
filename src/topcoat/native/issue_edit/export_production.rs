//! Real native issue downloads through the production router and browser.

use std::net::SocketAddr;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use scraper::{Html, Selector};
use tower::ServiceExt;

use super::super::home_fixture::{self, Fixture};
use crate::db::{
    models::{Role, UpdateIssue},
    queries,
};

const TITLE: &str = "Native export";
const DESCRIPTION: &str = "Export **body**.\n\nTrailing author spaces  \n";
const FILENAME: &str = "acc-1-native-export.md";
const MOUNTS: [&str; 3] = ["", "/app", "/ACC"];
const DOWNLOAD: &str = "/__native_issue_export/ACC-1";

fn export_fixture(role: Role) -> Fixture {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, id).unwrap();
        queries::members::upsert_member(&conn, issue.project_id, actor.id, role).unwrap();
        queries::update_issue(
            &conn,
            id,
            &UpdateIssue {
                title: Some(TITLE.into()),
                description: Some(DESCRIPTION.into()),
                ..Default::default()
            },
        )
        .unwrap();
        for related in ["ACC-2", "HIDE-1"] {
            let related = queries::resolve_identifier(&conn, related).unwrap();
            queries::link_issues(&conn, id, related, "relates_to").unwrap();
        }
    }
    fixture
}

fn expected(fixture: &Fixture) -> String {
    let conn = fixture.db.read().unwrap();
    let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
    let identity = Some(crate::auth::fresh_identity(
        &actor,
        crate::actor::Transport::Web,
    ));
    drop(conn);
    let visible = crate::authz::visible_project_ids(&fixture.db, &identity).unwrap();
    let conn = fixture.db.read().unwrap();
    let bundle = crate::export::export_issue(&conn, "ACC-1", visible.as_ref()).unwrap();
    assert_eq!(bundle.files.len(), 1);
    let file = bundle.files.into_iter().next().unwrap();
    assert_eq!(file.path, format!("ACC/issues/{FILENAME}"));
    assert!(file.content.contains("ACC-2"));
    assert!(!file.content.contains("HIDE-1"));
    file.content
}

fn cursor(fixture: &Fixture) -> (i64, i64) {
    let conn = fixture.db.read().unwrap();
    let issue =
        queries::get_issue(&conn, queries::resolve_identifier(&conn, "ACC-1").unwrap()).unwrap();
    let audit = conn
        .query_row("SELECT COALESCE(MAX(id), 0) FROM audit_log", [], |row| {
            row.get(0)
        })
        .unwrap();
    (issue.seq, audit)
}

async fn get(
    fixture: &Fixture,
    path: &str,
    cookie: Option<&str>,
    prefix: &str,
) -> axum::response::Response {
    let app = if prefix.is_empty() {
        fixture.app.clone()
    } else {
        Router::new().nest(prefix, fixture.app.clone())
    };
    let mut request = Request::builder().uri(format!("{prefix}{path}"));
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    if !prefix.is_empty() {
        request = request.header("x-forwarded-prefix", prefix);
    }
    let mut request = request.body(Body::empty()).unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<SocketAddr>().unwrap(),
    ));
    app.oneshot(request).await.unwrap()
}

async fn body(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn native_issue_export_viewer_and_maintainer_download_exact_scoped_markdown_at_every_mount() {
    for role in [Role::Viewer, Role::Maintainer] {
        let fixture = export_fixture(role);
        let expected = expected(&fixture);
        let before = cursor(&fixture);
        let cookie = format!("lific_token={}", fixture.token);
        for prefix in MOUNTS {
            let response = get(&fixture, DOWNLOAD, Some(&cookie), prefix).await;
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "native download at {prefix} for {role:?}"
            );
            assert_eq!(
                response.headers()[header::CONTENT_TYPE],
                "text/markdown; charset=utf-8"
            );
            assert_eq!(
                response.headers()[header::CONTENT_DISPOSITION],
                format!("attachment; filename=\"{FILENAME}\"")
            );
            let bytes = body(response).await;
            assert_eq!(
                bytes, expected,
                "native export matches the established scoped Rust exporter"
            );
            assert!(bytes.contains("identifier: ACC-1"));
            assert!(bytes.contains("# Native export\n\n"));
            assert!(bytes.contains("relates_to:\n- ACC-2"));
            assert!(!bytes.contains("HIDE-1"));
            assert!(!bytes.contains("Private hidden"));
        }
        assert_eq!(
            cursor(&fixture),
            before,
            "export creates no mutation, audit or cursor change"
        );
    }
}

#[tokio::test]
async fn native_issue_export_normal_issue_document_exposes_export_to_both_authorized_roles() {
    for role in [Role::Viewer, Role::Maintainer] {
        let fixture = export_fixture(role);
        let cookie = format!("lific_token={}", fixture.token);
        for prefix in MOUNTS {
            let response = get(&fixture, "/ACC/issues/ACC-1", Some(&cookie), prefix).await;
            assert_eq!(response.status(), StatusCode::OK);
            let markup = body(response).await;
            let document = Html::parse_document(&markup);
            let exports = document
                .select(&Selector::parse("button[aria-label='Export']").unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                exports.len(),
                1,
                "the actual normal document has one private Export button for {role:?}"
            );
            assert!(exports[0].value().attr("disabled").is_none());
            assert!(
                !markup.contains("/api/export/issues/"),
                "the native control must not call the legacy API transport"
            );
        }
    }
}

#[tokio::test]
async fn native_issue_export_hidden_missing_and_revoked_membership_are_denied_before_capacity() {
    let fixture = export_fixture(Role::Viewer);
    let cookie = format!("lific_token={}", fixture.token);
    let _first = fixture.db.acquire_export_slot().unwrap();
    let _second = fixture.db.acquire_export_slot().unwrap();
    for prefix in MOUNTS {
        for (path, status) in [
            ("/__native_issue_export/HIDE-1", StatusCode::FORBIDDEN),
            ("/__native_issue_export/ACC-99999", StatusCode::NOT_FOUND),
        ] {
            let response = get(&fixture, path, Some(&cookie), prefix).await;
            assert_eq!(response.status(), status);
            assert!(
                response
                    .headers()
                    .get(header::CONTENT_DISPOSITION)
                    .is_none()
            );
            let content = body(response).await;
            assert!(!content.contains(TITLE));
            assert!(!content.contains(DESCRIPTION));
            assert!(!content.contains("Private hidden initial work"));
        }
    }
    {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, id).unwrap();
        queries::members::remove_member(&conn, issue.project_id, actor.id).unwrap();
    }
    for prefix in MOUNTS {
        let response = get(&fixture, DOWNLOAD, Some(&cookie), prefix).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "current membership outranks export capacity"
        );
        assert!(
            response
                .headers()
                .get(header::CONTENT_DISPOSITION)
                .is_none()
        );
        assert!(!body(response).await.contains(TITLE));
    }
}

#[tokio::test]
async fn native_issue_export_absent_invalid_expired_and_deleted_sessions_redirect_without_bytes() {
    let fixture = export_fixture(Role::Viewer);
    {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        assert_eq!(
            conn.execute(
                "UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE user_id=?1",
                [actor.id]
            )
            .unwrap(),
            1
        );
        assert!(queries::users::validate_session(&conn, &fixture.token).is_err());
    }
    for prefix in MOUNTS {
        for cookie in [
            None,
            Some("lific_token=invalid".to_owned()),
            Some(format!("lific_token={}", fixture.token)),
        ] {
            let response = get(&fixture, DOWNLOAD, cookie.as_deref(), prefix).await;
            assert!(response.status().is_redirection());
            assert_eq!(
                response.headers()[header::LOCATION],
                format!("{prefix}/login")
            );
            assert!(
                response
                    .headers()
                    .get(header::CONTENT_DISPOSITION)
                    .is_none()
            );
            assert!(!body(response).await.contains(TITLE));
        }
    }
    let deleted = export_fixture(Role::Viewer);
    {
        let conn = deleted.db.write().unwrap();
        queries::users::delete_session(&conn, &deleted.token).unwrap();
    }
    let cookie = format!("lific_token={}", deleted.token);
    for prefix in MOUNTS {
        let response = get(&deleted, DOWNLOAD, Some(&cookie), prefix).await;
        assert!(response.status().is_redirection());
        assert_eq!(
            response.headers()[header::LOCATION],
            format!("{prefix}/login")
        );
        assert!(
            response
                .headers()
                .get(header::CONTENT_DISPOSITION)
                .is_none()
        );
        assert!(!body(response).await.contains(TITLE));
    }
}

#[tokio::test]
async fn native_issue_export_capacity_failure_is_retryable_without_bypassing_the_shared_limit() {
    let fixture = export_fixture(Role::Viewer);
    let cookie = format!("lific_token={}", fixture.token);
    let before = cursor(&fixture);
    let first = fixture.db.acquire_export_slot().unwrap();
    let second = fixture.db.acquire_export_slot().unwrap();
    for prefix in MOUNTS {
        let response = get(&fixture, DOWNLOAD, Some(&cookie), prefix).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(
            response
                .headers()
                .get(header::CONTENT_DISPOSITION)
                .is_none()
        );
        assert!(!body(response).await.contains(TITLE));
    }
    drop(first);
    drop(second);
    let response = get(&fixture, DOWNLOAD, Some(&cookie), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await, expected(&fixture));
    assert_eq!(cursor(&fixture), before);
}

async fn browser(role: Role, capacity_blocked: bool) {
    let fixture = export_fixture(role);
    let expected = expected(&fixture);
    let before = cursor(&fixture);
    let _slots = capacity_blocked.then(|| {
        [
            fixture.db.acquire_export_slot().unwrap(),
            fixture.db.acquire_export_slot().unwrap(),
        ]
    });
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("export.browser.test.cjs");
    std::fs::write(&script, DRIVER).unwrap();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command =
        home_fixture::browser_command(script.to_str().unwrap(), &origin, &fixture.token);
    command.arg(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/topcoat/native/browser_fixture.cjs"),
    );
    command
        .arg(FILENAME)
        .arg(expected)
        .arg(if capacity_blocked {
            "capacity"
        } else {
            "download"
        });
    let result = tokio::time::timeout(std::time::Duration::from_secs(90), command.output()).await;
    server.abort();
    let output = result.expect("native export browser timed out").unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(cursor(&fixture), before);
}

#[tokio::test]
async fn native_issue_export_viewer_browser_downloads_without_any_api_request() {
    browser(Role::Viewer, false).await;
}

#[tokio::test]
async fn native_issue_export_maintainer_browser_downloads_without_any_api_request() {
    browser(Role::Maintainer, false).await;
}

#[tokio::test]
async fn native_issue_export_browser_shows_real_capacity_error_and_allows_retry() {
    browser(Role::Viewer, true).await;
}

const DRIVER: &str = r###"
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const {mountedProxy,launchBrowser}=require(process.argv[4]);
const upstream=new URL(process.argv[2]),token=process.argv[3],filename=process.argv[5],expected=process.argv[6],capacity=process.argv[7]==='capacity';
test('normal native issue Export downloads exact scoped bytes',async t=>{
  const browser=await launchBrowser();
  try {
    for(const prefix of ['', '/app', '/ACC']) await t.test(prefix||'root',async()=>{
      const proxy=await mountedProxy(upstream,prefix);
      const context=await browser.newContext({viewport:{width:1440,height:900},acceptDownloads:true});
      const requests=[],fontRequests=[],errors=[],consoleErrors=[];
      try {
        await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
        await context.route(/^https:\/\/fonts\.(googleapis|gstatic)\.com\//,route=>route.abort());
        context.on('request',request=>{
          requests.push(new URL(request.url()).pathname);
          if(request.resourceType()==='font')fontRequests.push(new URL(request.url()));
        });
        const page=await context.newPage(); page.setDefaultTimeout(15000);
        page.on('pageerror',error=>errors.push(error.message));
        page.on('console',message=>{if(message.type()==='error') consoleErrors.push({message:message.text(),path:new URL(message.location().url).pathname});});
        assert.equal((await page.goto(`${proxy.origin}${prefix}/ACC/issues/ACC-1`)).status(),200);
        await page.getByRole('button',{name:'Native export',exact:true}).or(page.getByRole('heading',{name:'Native export',exact:true})).waitFor();
        const loadedFonts=await page.evaluate(async()=>{
          const fonts=await Promise.all([
            document.fonts.load('14px "DM Sans"'),
            document.fonts.load('italic 14px "DM Sans"'),
            document.fonts.load('700 14px "Space Grotesk"'),
          ]);
          await document.fonts.ready;
          return fonts.map(group=>group.map(font=>({family:font.family,status:font.status})));
        });
        assert.ok(loadedFonts.every(group=>group.length>0&&group.every(font=>font.status==='loaded')),
          'The original normal, italic and display fonts load with Google requests blocked.');
        assert.ok(fontRequests.length>=3);
        assert.ok(fontRequests.every(url=>url.origin===proxy.origin&&url.pathname.startsWith(`${prefix}/__topcoat-font-`)),
          'Production font requests use only the current same-origin mount.');
        const exportButton=page.getByRole('button',{name:'Export',exact:true});
        await exportButton.waitFor();
        const exportPath=`${prefix}/__native_issue_export/ACC-1`;
        if(capacity){
          let downloads=0;
          page.on('download',()=>downloads++);
          for(let attempt=0;attempt<2;attempt++){
            const failure=page.waitForResponse(response=>new URL(response.url()).pathname===exportPath);
            await exportButton.click();
            assert.equal((await failure).status(),429,'The shared production export slots cause the error.');
            const error=page.locator('[data-native-issue-export-error]');
            await error.waitFor();
            assert.ok((await error.textContent()).trim(),'Actual export errors are visible in the toolbar.');
            await exportButton.waitFor();
            assert.equal(await exportButton.isDisabled(),false,'An unsuccessful export permits retry.');
          }
          assert.equal(downloads,0);
          assert.equal(requests.filter(path=>path===exportPath).length,2);
          assert.deepEqual(requests.filter(path=>/(^|\/)api\//.test(path)),[]);
          assert.deepEqual(errors,[]);
          assert.deepEqual(consoleErrors,Array.from({length:2},()=>({
            message:'Failed to load resource: the server responded with a status of 429 (Too Many Requests)',
            path:exportPath
          })),'Only the two verified capacity responses may produce browser resource diagnostics.');
          return;
        }
        let releaseDownload;
        const blockedDownload=new Promise(resolve=>releaseDownload=resolve);
        await page.route(`**${exportPath}`,async route=>{
          const response=await route.fetch();
          assert.equal(response.status(),200);
          await blockedDownload;
          await route.fulfill({response});
        });
        const downloadPromise=page.waitForEvent('download');
        await exportButton.click();
        const exporting=page.getByRole('button',{name:'Exporting',exact:true});
        await exporting.waitFor();
        assert.equal(await exporting.isDisabled(),true,'A pending real export disables duplicate submission.');
        releaseDownload();
        const download=await downloadPromise;
        assert.equal(download.suggestedFilename(),filename);
        assert.equal(await download.failure(),null);
        assert.equal(fs.readFileSync(await download.path(),'utf8'),expected);
        assert.ok(requests.includes(exportPath),'Browser fetched the real native download route.');
        assert.deepEqual(requests.filter(path=>/(^|\/)api\//.test(path)),[],'Native Export makes no application API or loopback request.');
        assert.deepEqual(errors,[]);
        assert.deepEqual(consoleErrors,[]);
        await page.getByRole('button',{name:'Export',exact:true}).waitFor();
        assert.equal(await page.evaluate(()=>localStorage.getItem('lific_token')),null);
      }catch(failure){
        throw new Error(`${failure.message}\nBrowser errors: ${JSON.stringify(errors)}\nConsole errors: ${JSON.stringify(consoleErrors)}\nRequests: ${JSON.stringify(requests)}`,{cause:failure});
      }finally{
        await context.close();await proxy.close();
      }
    });
  }finally{await browser.close();}
});
"###;

#[tokio::test]
async fn native_issue_export_retired_toolbar_cannot_download_and_new_owner_can() {
    let fixture = export_fixture(Role::Maintainer);
    let expected = expected(&fixture);
    let before = cursor(&fixture);
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("export.retirement.browser.test.cjs");
    std::fs::write(&script, RETIREMENT_DRIVER).unwrap();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command =
        home_fixture::browser_command(script.to_str().unwrap(), &origin, &fixture.token);
    command.arg(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/topcoat/native/browser_fixture.cjs"),
    );
    command.arg(FILENAME).arg(expected);
    let result = tokio::time::timeout(std::time::Duration::from_secs(90), command.output()).await;
    server.abort();
    let output = result
        .expect("native export retirement browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(cursor(&fixture), before);
}

const RETIREMENT_DRIVER: &str = r###"
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const {mountedProxy,launchBrowser}=require(process.argv[4]);
const upstream=new URL(process.argv[2]),token=process.argv[3],filename=process.argv[5],expected=process.argv[6];
const settle=page=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
test('real page refresh retires an in-flight export toolbar owner',async t=>{
  const browser=await launchBrowser();
  try {
    for(const prefix of ['', '/app', '/ACC']) await t.test(prefix||'root',async()=>{
      const proxy=await mountedProxy(upstream,prefix);
      const context=await browser.newContext({viewport:{width:1440,height:900},acceptDownloads:true});
      const requests=[],errors=[],downloads=[];
      let releaseDownload;
      try {
        await context.addCookies([{name:'lific_token',value:token,url:proxy.origin,httpOnly:true,sameSite:'Lax'}]);
        context.on('request',request=>requests.push({method:request.method(),path:new URL(request.url()).pathname,headers:request.headers()}));
        const page=await context.newPage(); page.setDefaultTimeout(15000);
        page.on('pageerror',error=>errors.push(error.message));
        page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});
        page.on('download',download=>downloads.push(download));
        const documentPath=`${prefix}/ACC/issues/ACC-1`,exportPath=`${prefix}/__native_issue_export/ACC-1`;
        assert.equal((await page.goto(`${proxy.origin}${documentPath}`)).status(),200);
        const button=page.getByRole('button',{name:'Export',exact:true});
        await button.waitFor();
        let responseHeld,routeDone,terminal;
        const held=new Promise(resolve=>responseHeld=resolve);
        const completedRoute=new Promise(resolve=>routeDone=resolve);
        const completedRequest=new Promise(resolve=>terminal=resolve);
        const blocked=new Promise(resolve=>releaseDownload=resolve);
        context.on('requestfinished',request=>{if(new URL(request.url()).pathname===exportPath)terminal({type:'finished'});});
        context.on('requestfailed',request=>{if(new URL(request.url()).pathname===exportPath)terminal({type:'failed',reason:request.failure()?.errorText});});
        await page.route(`**${exportPath}`,async route=>{
          const response=await route.fetch();
          assert.equal(response.status(),200,'The held bytes come from the actual native exporter.');
          assert.equal(await response.text(),expected);
          responseHeld();
          await blocked;
          try {await route.fulfill({response});routeDone(null);}
          catch(failure){routeDone(failure.message);}
        });
        await button.click();
        await held;
        const busy=page.getByRole('button',{name:'Exporting',exact:true});
        await busy.waitFor();
        assert.equal(await busy.isDisabled(),true);
        const refresh=await page.evaluate(async()=>{
          const previous=document.querySelector('button.native-issue-detail__export');
          const detail={};
          window.dispatchEvent(new CustomEvent('topcoat:dev-runtime:v1',{detail}));
          if(!detail.runtime)throw new Error('The installed framework page refresh listener is missing.');
          const response=await detail.runtime.request(new AbortController().signal);
          if(!response.ok)throw new Error(`Production framework refresh failed: ${response.status}`);
          const html=await response.text();
          const parsed=new DOMParser().parseFromString(html,'text/html');
          if(!parsed.querySelector('[data-native-issue-editor="ACC-1"]'))throw new Error('Refresh did not return the actual issue document.');
          // PageUnit's public dev-refresh interface disposes the previous
          // content scope, applies this genuine server document, and hydrates it.
          detail.runtime.replace(()=>document.body.replaceChildren(...Array.from(parsed.body.childNodes,node=>document.importNode(node,true))));
          return {status:response.status,contentType:response.headers.get('content-type'),oldDetached:!previous.isConnected};
        });
        assert.equal(refresh.status,200);
        assert.match(refresh.contentType,/^text\/html/);
        assert.equal(refresh.oldDetached,true,'The previous toolbar belongs to the retired framework content scope.');
        assert.ok(requests.some(request=>request.method==='POST'&&request.path===documentPath&&request.headers['x-topcoat-runtime']==='true'),'Refresh uses the real mounted production framework POST.');
        releaseDownload();
        const result=await completedRequest;
        const deliveryFailure=await completedRoute;
        if(deliveryFailure){
          assert.equal(result.type,'failed','A response may become undeliverable only after genuine request cancellation.');
          assert.match(result.reason,/abort|cancel/i);
        }
        await settle(page);
        assert.equal(downloads.length,0,'Delivering the original response cannot download after its toolbar scope retires.');
        await page.unroute(`**${exportPath}`);
        const current=page.getByRole('button',{name:'Export',exact:true});
        await current.waitFor();
        assert.equal(await current.isDisabled(),false,'The new toolbar owns no pending old export.');
        const freshDownload=page.waitForEvent('download');
        await current.click();
        const download=await freshDownload;
        assert.equal(download.suggestedFilename(),filename);
        assert.equal(await download.failure(),null);
        assert.equal(fs.readFileSync(await download.path(),'utf8'),expected);
        await page.getByRole('button',{name:'Export',exact:true}).waitFor();
        await settle(page);
        assert.equal(downloads.length,1,'Only the replacement owner can produce a download.');
        assert.deepEqual(requests.filter(request=>/(^|\/)api\//.test(request.path)),[]);
        assert.deepEqual(errors,[]);
      }finally{
        releaseDownload?.();
        await context.close();await proxy.close();
      }
    });
  }finally{await browser.close();}
});
"###;
