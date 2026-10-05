//! Three-step GitHub import using the shared Rust collector/application service.
use super::super::{context, icons, session};
use super::import_model::valid_repository;
use crate::realtime::RealtimeHub;
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Signal, expr, procedure, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

// Result, issues, comments, labels, existing, excluded PRs.
type Outcome = (Result<String, String>, usize, usize, usize, usize, usize);
// Keep primitive wire arguments separate because expr! does not support tuple literals.
#[allow(clippy::too_many_arguments)]
#[procedure("/__native_overview/import_github")]
async fn run(
    cx: &Cx,
    account: i64,
    project: i64,
    repo: String,
    token: String,
    state: String,
    open: String,
    closed: String,
    dry_run: bool,
) -> topcoat::Result<Outcome> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Ok((
            Err("Your account changed. Reload this page.".into()),
            0,
            0,
            0,
            0,
            0,
        ));
    }
    if !valid_repository(&repo) {
        return Ok((
            Err("Enter a repository as owner/name.".into()),
            0,
            0,
            0,
            0,
            0,
        ));
    }
    let repo = repo.trim_matches(super::labels_model::js_whitespace);
    let token = token.trim_matches(super::labels_model::js_whitespace);
    let req = crate::services::github_import::GithubImportRequest {
        repo: repo.into(),
        token: (!token.is_empty()).then(|| token.to_owned()),
        state,
        map_open: open,
        map_closed: closed,
        dry_run,
    };
    let result = caller
        .scope(crate::services::github_import::run(
            context::db(cx),
            app_context::<RealtimeHub>(cx),
            &caller.identity,
            project,
            req,
        ))
        .await;
    match result {
        Ok(summary) => Ok((
            Ok(if dry_run { "preview" } else { "done" }.into()),
            summary.issues_created,
            if dry_run {
                summary.comments_planned
            } else {
                summary.comments_created
            },
            if dry_run {
                summary.labels_planned
            } else {
                summary.labels_created
            },
            summary.issues_skipped_existing,
            summary.skipped_non_issues,
        )),
        Err(error) => Ok((Err(super::actions::error_message(error)), 0, 0, 0, 0, 0)),
    }
}
#[derive(Clone)]
struct State {
    repo: Signal<String>,
    repository_valid: Signal<bool>,
    token: Signal<String>,
    filter: Signal<String>,
    open: Signal<String>,
    closed: Signal<String>,
    step: Signal<String>,
    busy: Signal<bool>,
    error: Signal<String>,
    issues: Signal<usize>,
    comments: Signal<usize>,
    labels: Signal<usize>,
    existing: Signal<usize>,
    excluded: Signal<usize>,
}
fn run_attributes(
    cx: &Cx,
    state: &State,
    account: i64,
    project: i64,
    preview: bool,
    revision: Signal<usize>,
) -> Attributes {
    let repo = state.repo.clone();
    let token = state.token.clone();
    let filter = state.filter.clone();
    let open = state.open.clone();
    let closed = state.closed.clone();
    let step = state.step.clone();
    let busy = state.busy.clone();
    let error = state.error.clone();
    let issues = state.issues.clone();
    let comments = state.comments.clone();
    let labels = state.labels.clone();
    let existing = state.existing.clone();
    let excluded = state.excluded.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(async |_event: Event| {
        if !busy.get() {
            busy.set(true);
            error.set("".to_owned());
            let original_repo = repo.get();
            let original_token = token.get();
            let original_filter = filter.get();
            let original_open = open.get();
            let original_closed = closed.get();
            let _failed = || {
                failed_busy.set(false);
                failed_error.set("Import did not complete. Try again.".to_owned());
            };
            let _run = async || {
                let result = run(
                    account,
                    project,
                    original_repo,
                    original_token,
                    original_filter,
                    original_open,
                    original_closed,
                    preview,
                )
                .await;
                busy.set(false);
                if result.0.is_ok() {
                    step.set(result.0.unwrap());
                    issues.set(result.1);
                    comments.set(result.2);
                    labels.set(result.3);
                    existing.set(result.4);
                    excluded.set(result.5);
                    if if !preview { result.1 > 0_usize } else { false } {
                        revision.increment();
                    }
                } else {
                    error.set(result.0.unwrap_err());
                }
            };
            raw!(
                "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                ()
            );
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}
pub(super) fn panel(cx: &Cx, account: i64, project: i64, revision: Signal<usize>) -> BoxView<'_> {
    let state = State {
        repo: signal(cx, String::new),
        repository_valid: signal(cx, || false),
        token: signal(cx, String::new),
        filter: signal(cx, || "all".into()),
        open: signal(cx, || "backlog".into()),
        closed: signal(cx, || "done".into()),
        step: signal(cx, || "configure".into()),
        busy: signal(cx, || false),
        error: signal(cx, String::new),
        issues: signal(cx, || 0_usize),
        comments: signal(cx, || 0_usize),
        labels: signal(cx, || 0_usize),
        existing: signal(cx, || 0_usize),
        excluded: signal(cx, || 0_usize),
    };
    let repo = state.repo.clone();
    let repository_valid = state.repository_valid.clone();
    let token = state.token.clone();
    let filter = state.filter.clone();
    let open = state.open.clone();
    let closed = state.closed.clone();
    let step = state.step.clone();
    let busy = state.busy.clone();
    let error = state.error.clone();
    let issues = state.issues.clone();
    let comments = state.comments.clone();
    let labels = state.labels.clone();
    let existing = state.existing.clone();
    let excluded = state.excluded.clone();
    let preview = run_attributes(cx, &state, account, project, true, revision.clone());
    let import_attributes = run_attributes(cx, &state, account, project, false, revision);
    view!{cx => <section class="native-overview-import" data-native-overview-import=""><h2>(icons::project_icon(cx,Some("lucide:Download"),16))" Import from GitHub"</h2><p>"Pull issues from a GitHub repo into this project. Pull requests are skipped. Re-running never duplicates — already-imported issues are recognized and left alone."</p><div class="native-overview-import__card">
  <div :hidden=$(step.get()!="configure")>
   <label>"Repository"<input type="text" spellcheck="false" placeholder="owner/name" :value=$(repo.get()) :disabled=$(busy.get()) @input=$(|event:Event|{repository_valid.set(false);repo.set(event.target.value);})></label>
   <label>"Token "<span>"optional for public repos"</span><input type="password" spellcheck="false" autocomplete="off" placeholder="ghp_…" :value=$(token.get()) :disabled=$(busy.get()) @input=$(|event:Event|token.set(event.target.value))></label>
   <div class="native-overview-import__mapping"><label>"Issues"<select :value=$(filter.get()) :disabled=$(busy.get()) @change=$(|event:Event|filter.set(event.target.value))><option value="all">"Open + closed"</option><option value="open">"Open only"</option><option value="closed">"Closed only"</option></select></label><label>"Open →"<select :value=$(open.get()) :disabled=$(busy.get()) @change=$(|event:Event|open.set(event.target.value))>for status in ["backlog","todo","active","done","cancelled"]{<option value=(status)>(status)</option>}</select></label><label>"Closed →"<select :value=$(closed.get()) :disabled=$(busy.get()) @change=$(|event:Event|closed.set(event.target.value))>for status in ["backlog","todo","active","done","cancelled"]{<option value=(status)>(status)</option>}</select></label></div>
   native_overview_repository_valid(repo:$(repo.get()), valid:repository_valid.clone())
   <button type="button" :disabled=$(if busy.get(){true}else{!repository_valid.get()}) (preview)><span :hidden=$(busy.get())>"Preview import"</span><span :hidden=$(!busy.get())>"Previewing…"</span>(icons::project_icon(cx,Some("lucide:ArrowRight"),14))</button>
  </div>
  <div :hidden=$(step.get()!="preview")><p>"Previewing "<span class="native-overview-import__repo">$(repo.get().trim().to_owned())</span>". This will create:"</p>
   (stats(cx,issues.clone(),comments.clone(),labels.clone()))
   <p :hidden=$(existing.get()==0_usize)>$(existing.get())" already imported (will be skipped)."</p><p :hidden=$(excluded.get()==0_usize)>$(excluded.get())" pull request(s) excluded."</p>
   <div class="native-overview-import__actions"><button type="button" :disabled=$(if busy.get(){true}else{issues.get()==0_usize}) (import_attributes)><span :hidden=$(busy.get())>"Import "$(issues.get())" issue"<span :hidden=$(issues.get()==1_usize)>"s"</span></span><span :hidden=$(!busy.get())>"Importing…"</span></button><button type="button" :disabled=$(busy.get()) @click=$(|_event:Event|{step.set("configure".to_owned());error.set("".to_owned());})>"Back"</button></div>
  </div>
  <div :hidden=$(step.get()!="done")><h3>(icons::project_icon(cx,Some("lucide:Check"),15))" Import complete"</h3>(stats(cx,issues,comments,labels))<p :hidden=$(existing.get()==0_usize)>$(existing.get())" already-imported issue(s) skipped."</p><button type="button" @click=$(|_event:Event|{step.set("configure".to_owned());error.set("".to_owned());})>"Import another repo"</button></div>
  <p role="alert" :hidden=$(error.get().is_empty())>$(error.get())</p>
 </div></section>}.boxed()
}
fn stats(
    cx: &Cx,
    issues: Signal<usize>,
    comments: Signal<usize>,
    labels: Signal<usize>,
) -> BoxView<'_> {
    view!{cx => <div class="native-overview-import__stats"><div><strong>$(issues.get())</strong><span>"Issues"</span></div><div><strong>$(comments.get())</strong><span>"Comments"</span></div><div><strong>$(labels.get())</strong><span>"Labels"</span></div></div>}.boxed()
}

#[shard("/__native_overview/repository_valid")]
async fn native_overview_repository_valid(
    cx: &Cx,
    repo: String,
    valid: Signal<bool>,
) -> topcoat::Result<impl View> {
    let caller = session::read(cx, context::caller(cx))?;
    session::read(cx, crate::api::require_user(&caller.identity))?;
    let result = valid_repository(&repo);
    Ok(view! {cx => <span hidden="" @mount=$(|_event:Event|valid.set(result))></span>})
}
