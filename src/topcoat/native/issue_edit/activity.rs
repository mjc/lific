//! Pinned issue Activity timeline: authorized SQL history and Rust-owned display.

use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, component, view},
};

use super::super::{context, icons, session};
use crate::{
    db::{
        DbPool,
        models::{Activity, Priority, Status},
        queries,
    },
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

pub(crate) const STYLESHEET: &str = include_str!("activity.css");

/// Match the issue REST feed's default tail, but authorize and read together.
pub(crate) fn load(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    identifier: &str,
) -> Result<Vec<Activity>, LificError> {
    crate::api::require_user(identity)?;
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let id = queries::resolve_identifier(&tx, identifier)?;
    let items = crate::services::activity::list_activity_conn(
        &tx,
        identity,
        queries::activity::ActivityScope::Issue(id),
        None,
        None,
        None,
    )?
    .items;
    tx.commit()?;
    Ok(items)
}

#[shard("/__native_issue_edit/activity")]
pub(crate) async fn native_issue_activity(
    cx: &Cx,
    identifier: String,
    revision: i64,
) -> topcoat::Result<impl View> {
    let _ = revision;
    let caller = session::read(cx, context::caller(cx))?;
    let items = session::read(cx, load(context::db(cx), &caller.identity, &identifier))?;
    Ok(timeline(cx, items))
}

pub(crate) fn timeline(cx: &Cx, items: Vec<Activity>) -> BoxView<'_> {
    view! { cx => timeline_component(items: items) }.boxed()
}

fn actor_name(item: &Activity) -> &str {
    item.actor_display_name
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            item.actor_username
                .as_deref()
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("system")
}

fn short_value(value: Option<&str>, max: usize) -> String {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return "(none)".into();
    };
    // The source collapses runs of LF only, then applies JavaScript's trim.
    static NEWLINES: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\n+").expect("pinned activity newline expression is valid")
    });
    let flat = NEWLINES.replace_all(value, " ");
    let flat = flat.trim_matches(|character: char| {
        matches!(character,
            '\u{0009}'..='\u{000d}' | ' ' | '\u{00a0}' | '\u{1680}' |
            '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
            '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    });
    if flat.is_empty() {
        return "(none)".into();
    }
    // String.slice counts UTF-16 code units in the original renderer.
    let units: Vec<_> = flat.encode_utf16().collect();
    if units.len() > max {
        format!("{}…", String::from_utf16_lossy(&units[..max]))
    } else {
        flat.to_owned()
    }
}

fn verb(item: &Activity) -> String {
    let field = item.field.as_deref().unwrap_or("null");
    let new = item.new_value.as_deref().filter(|value| !value.is_empty());
    if item.entity_type == "plan_step" {
        return match item.action.as_str() {
            "create" => "added step".into(),
            "delete" => "removed step".into(),
            "auto-complete" => "auto-completed a step (issue closed)".into(),
            "auto-reopen" => "reopened a step (issue reopened)".into(),
            "update" => match field {
                "done" => {
                    if new == Some("1") {
                        "completed a step".into()
                    } else {
                        "reopened a step".into()
                    }
                }
                "title" => "renamed a step".into(),
                "description" => "edited a step’s description".into(),
                "issue" => {
                    if new.is_some() {
                        "linked a step to".into()
                    } else {
                        "unlinked a step from".into()
                    }
                }
                _ => format!("changed a step’s {field}"),
            },
            _ => generic_verb(item, field),
        };
    }
    if item.entity_type == "plan" {
        return match item.action.as_str() {
            "create" => "created this plan".into(),
            "delete" => "deleted the plan".into(),
            "auto-archive" => "archived the plan (anchor issue closed)".into(),
            "update" => match field {
                "status" => "set status to".into(),
                "title" => "renamed the plan".into(),
                "anchor_issue" => {
                    if new.is_some() {
                        "set anchor to".into()
                    } else {
                        "cleared the anchor".into()
                    }
                }
                _ => format!("changed {field}"),
            },
            _ => generic_verb(item, field),
        };
    }
    generic_verb(item, field)
}

fn generic_verb(item: &Activity, field: &str) -> String {
    match item.action.as_str() {
        "create" if item.entity_type == "comment" => "commented".into(),
        "delete" if item.entity_type == "comment" => "deleted a comment".into(),
        "update" if item.entity_type == "comment" => "edited a comment".into(),
        "create" => format!("created this {}", item.entity_type),
        "delete" => format!("deleted {}", item.entity_type),
        "update" => format!("changed {field}"),
        "attach" => "added label".into(),
        "detach" => "removed label".into(),
        "link" | "unlink" => format!(
            "{}ed {}",
            item.action,
            item.field
                .as_deref()
                .unwrap_or("relates_to")
                .replacen('_', " ", 1)
        ),
        "wait" => "started waiting on".into(),
        "unwait" => "stopped waiting on".into(),
        _ => item.action.clone(),
    }
}

fn clock_mount(cx: &Cx, now: Signal<f64>) -> Attributes {
    let handler = expr!(|_event: Event| {
        raw!("let interval;", ());
        let _tick = || {
            now.set(raw!("cx.hydrate(Date.now())", 0.0));
        };
        let _visible = |_event: Event| {
            let hidden = raw!("cx.hydrate(document.visibilityState === 'hidden')", false);
            if hidden {
                raw!("clearInterval(interval); interval=undefined;", ());
            } else {
                raw!("${_tick}();", ());
                raw!(
                    "if(interval===undefined) interval=setInterval(()=>${_tick}(),30000);",
                    ()
                );
            };
        };
        raw!("${_visible}(${_event});", ());
        raw!(
            "document.addEventListener('visibilitychange',event=>${_visible}(cx.event(event)),{signal:cx.abortSignal}); cx.abortSignal.addEventListener('abort',()=>clearInterval(interval),{once:true});",
            ()
        );
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attributes
}

#[component]
async fn timeline_component(cx: &Cx, items: Vec<Activity>) -> topcoat::Result<impl View> {
    let count = items.len();
    let expanded = signal(cx, || false);
    let now = signal(cx, || chrono::Utc::now().timestamp_millis() as f64);
    let mounted = clock_mount(cx, now.clone());
    Ok(view! { cx =>
        if count > 0 {
            <section class="native-issue-activity" data-native-issue-activity="" (mounted)>
                <div class="native-issue-activity__header">
                    (icons::project_icon(cx, Some("lucide:History"), 13))
                    <h2>"Activity"</h2><span class="native-issue-activity__count">(count.to_string())</span>
                </div>
                <ol><div class="native-issue-activity__rail" aria-hidden="true"></div>
                    for (index, item) in items.into_iter().enumerate() {
                        <li data-activity-id=(item.id.to_string()) :hidden=$(if index >= 6 { !expanded.get() } else { false }) :style=$(if index == 5 { if !expanded.get() { "padding-bottom:0" } else { "" } } else { "" })>
                            <span class="native-issue-activity__dot" aria-hidden="true"></span>
                            (keyed_row(cx, item, now.clone()))
                        </li>
                    }
                </ol>
                if count > 6 {
                    <button type="button" class="native-issue-activity__all" @click=$(|_event: Event| { expanded.set(!expanded.get()); })>
                        <span :style=$(if expanded.get() { "display:inline-flex;transition:transform .15s;transform:rotate(180deg)" } else { "display:inline-flex;transition:transform .15s" })>(icons::project_icon(cx, Some("lucide:ChevronDown"), 12))</span>
                        <span :hidden=$(expanded.get())>(format!("Show all {count} entries"))</span>
                        <span :hidden=$(!expanded.get())>"Show recent only"</span>
                    </button>
                }
            </section>
        }
    })
}

fn keyed_row(cx: &Cx, item: Activity, now: Signal<f64>) -> BoxView<'_> {
    let row_cx = cx.keyed(item.id);
    view! { row_cx => activity_row(item: item, now: now) }.boxed()
}

#[component]
async fn activity_row(cx: &Cx, item: Activity, now: Signal<f64>) -> topcoat::Result<impl View> {
    let actor = actor_name(&item).to_owned();
    let description = verb(&item);
    let title = if item.entity_type == "plan_step" || item.entity_type == "plan" {
        match item.action.as_str() {
            "create" => item.new_value.as_deref(),
            "delete" => item.old_value.as_deref(),
            _ => None,
        }
    } else {
        None
    }
    .filter(|value| !value.is_empty())
    .map(|value| short_value(Some(value), 60));
    let long =
        item.action == "update" && matches!(item.field.as_deref(), Some("description" | "content"));
    let open = signal(cx, || false);
    let old = item.old_value.clone().unwrap_or_default();
    let new = item.new_value.clone().unwrap_or_default();
    let values = value_view(cx, &item, title);
    Ok(view! { cx =>
        <div class="native-issue-activity__line">
            <span class="native-issue-activity__actor">(actor)</span>" "
            if item.actor_is_bot { <span class="native-issue-activity__agent">"agent"</span>" " }
            (description)" "
            if long {
                <button type="button" @click=$(|_event: Event| { open.set(!open.get()); })>
                    <span :hidden=$(open.get())>"show change"</span><span :hidden=$(!open.get())>"hide change"</span>
                    <span :style=$(if open.get() { "display:inline-flex;transition:transform .15s;transform:rotate(180deg)" } else { "display:inline-flex;transition:transform .15s" })>(icons::project_icon(cx, Some("lucide:ChevronDown"), 11))</span>
                </button>
            } else { (values) }
            " "(time_view(cx, &item.ts, &item.transport, now))
        </div>
        if long {
            <div class="native-issue-activity__values" :hidden=$(!open.get())>
                <div class="native-issue-activity__values-old">(if old.is_empty() { "(empty)".to_owned() } else { old })</div>
                <div class="native-issue-activity__values-new">(if new.is_empty() { "(empty)".to_owned() } else { new })</div>
            </div>
        }
    })
}

fn value_view<'a>(cx: &'a Cx, item: &Activity, title: Option<String>) -> BoxView<'a> {
    if let Some(title) = title {
        return view! { cx => <span class="native-issue-activity__quoted">"“"(title)"”"</span> }
            .boxed();
    }
    let old = item.old_value.clone().unwrap_or_default();
    let new = item.new_value.clone().unwrap_or_default();
    let field = item.field.as_deref();
    if item.action == "update" {
        match field {
            Some("done" | "description" | "content") => view! { cx => "" }.boxed(),
            Some("issue" | "anchor_issue") => { let value = item.new_value.clone().or_else(|| item.old_value.clone()).unwrap_or_default(); view! { cx => <span class="native-issue-activity__identifier">(value)</span> }.boxed() },
            Some("status") if item.entity_type == "plan" => view! { cx => <span class="native-issue-activity__new" style="text-transform:capitalize">(new)</span> }.boxed(),
            Some("status" | "priority") => {
                let old_icon = value_icon(cx, field.unwrap(), &old);
                let new_icon = value_icon(cx, field.unwrap(), &new);
                view! { cx =>
                    <span class="native-issue-activity__icon-value">(old_icon)(old)</span>" "
                    <span class="native-issue-activity__arrow">"→"</span>" "
                    <span class="native-issue-activity__icon-value native-issue-activity__new">(new_icon)(new)</span>
                }.boxed()
            },
            _ => { let old = short_value(item.old_value.as_deref(), 40); let new = short_value(item.new_value.as_deref(), 40); view! { cx => <span class="native-issue-activity__old">(old)</span>" "<span class="native-issue-activity__arrow">"→"</span>" "<span class="native-issue-activity__new">(new)</span> }.boxed() },
        }
    } else {
        let value = match item.action.as_str() {
            "attach" | "link" | "wait" | "create" => new,
            _ => old,
        };
        match item.action.as_str() {
            "attach" | "detach" => {
                view! { cx => <span class="native-issue-activity__label">(value)</span> }.boxed()
            }
            "link" | "unlink" => {
                view! { cx => <span class="native-issue-activity__identifier">(value)</span> }
                    .boxed()
            }
            "wait" | "unwait" => {
                let value = short_value(Some(&value), 60);
                view! { cx => <span class="native-issue-activity__new">(value)</span> }.boxed()
            }
            "create" if item.entity_type == "comment" => {
                let value = short_value(Some(&value), 60);
                view! { cx => <span class="native-issue-activity__quoted">"“"(value)"”"</span> }
                    .boxed()
            }
            _ => view! { cx => "" }.boxed(),
        }
    }
}

fn value_icon<'a>(cx: &'a Cx, field: &str, value: &str) -> BoxView<'a> {
    if field == "status" {
        match value.parse::<Status>() {
            Ok(status) => icons::status_icon(cx, status, 12),
            // Unknown historical statuses use Circle with the source's faint color.
            Err(_) => view! { cx => <span style="color:var(--tc-faint);display:inline-flex">(icons::project_icon(cx, Some("lucide:Circle"), 12))</span> }.boxed(),
        }
    } else {
        icons::priority_icon(cx, value.parse().unwrap_or(Priority::None), 12)
    }
}

fn time_view<'a>(cx: &'a Cx, timestamp: &str, transport: &str, now: Signal<f64>) -> BoxView<'a> {
    let date = chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
        .map_or(f64::NAN, |date| date.and_utc().timestamp_millis() as f64);
    let datetime = timestamp.to_owned();
    let timestamp = timestamp.to_owned();
    let transport = transport.to_owned();
    let full = signal(cx, || timestamp.clone());
    let fallback = signal(cx, || timestamp.clone());
    let title = signal(cx, || format!("{timestamp} · via {transport}"));
    view! { cx =>
        <span class="native-issue-activity__time" :title=$(title.get())>
            "· "<time datetime=(datetime) :title=$(full.get()) @mount=$(|_event: Event| {
                full.set(raw!("cx.hydrate(new Date(${timestamp}.toString()+'Z').toLocaleDateString('en-US',{month:'short',day:'numeric',year:'numeric',hour:'numeric',minute:'2-digit'}))", String::new()));
                let _local = full.get();
                title.set(raw!("cx.hydrate(${_local}.toString()+' · via '+${transport}.toString())", String::new()));
                fallback.set(raw!("cx.hydrate(new Date(${timestamp}.toString()+'Z').toLocaleDateString('en-US',{month:'short',day:'numeric'}))", String::new()));
            })>
                $(if (now.get() - date) < 60000.0 { "just now".to_owned() }
                else { if (now.get() - date) < 3600000.0 {
                    let epoch = now.get();
                    let minutes = raw!("cx.hydrate(Math.floor((Number(${epoch}.toString())-Number(${date}.toString()))/60000))", ((epoch - date) / 60000.0).floor());
                    raw!("cx.hydrate(${minutes}.toString()+'m ago')", format!("{minutes}m ago"))
                } else { if (now.get() - date) < 86400000.0 {
                    let epoch = now.get();
                    let hours = raw!("cx.hydrate(Math.floor((Number(${epoch}.toString())-Number(${date}.toString()))/3600000))", ((epoch - date) / 3600000.0).floor());
                    raw!("cx.hydrate(${hours}.toString()+'h ago')", format!("{hours}h ago"))
                } else { if (now.get() - date) < 604800000.0 {
                    let epoch = now.get();
                    let days = raw!("cx.hydrate(Math.floor((Number(${epoch}.toString())-Number(${date}.toString()))/86400000))", ((epoch - date) / 86400000.0).floor());
                    raw!("cx.hydrate(${days}.toString()+'d ago')", format!("{days}d ago"))
                } else { fallback.get() } } } })
            </time>" via "(transport)
        </span>
    }.boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actor::Transport,
        db::models::{Role, UpdateIssue},
    };
    use scraper::{Html, Selector};

    fn fixture() -> (
        super::super::super::home_fixture::Fixture,
        Option<ResolvedIdentity>,
    ) {
        let fixture = super::super::super::home_fixture::fixture();
        let user =
            queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token).unwrap();
        let identity = Some(crate::auth::fresh_identity(&user, Transport::Web));
        (fixture, identity)
    }

    async fn html(items: Vec<Activity>) -> Html {
        let cx = Cx::default();
        let rendered = timeline(&cx, items).single().await.unwrap().render(&cx);
        Html::parse_fragment(&rendered)
    }

    #[tokio::test]
    async fn relation_history_excludes_hidden_targets_for_viewers_and_maintainers() {
        for role in [Role::Viewer, Role::Maintainer] {
            let (fixture, identity) = fixture();
            {
                let conn = fixture.db.write().unwrap();
                let source = queries::resolve_identifier(&conn, "ACC-1").unwrap();
                let allowed = queries::resolve_identifier(&conn, "ACC-2").unwrap();
                let hidden = queries::resolve_identifier(&conn, "HIDE-1").unwrap();
                let project = queries::get_issue(&conn, source).unwrap().project_id;
                queries::members::upsert_member(
                    &conn,
                    project,
                    identity.as_ref().unwrap().user.id,
                    role,
                )
                .unwrap();
                for relation in ["blocks", "relates_to", "duplicate"] {
                    for target in [allowed, hidden] {
                        queries::link_issues(&conn, source, target, relation).unwrap();
                        queries::unlink_issues(&conn, source, target).unwrap();
                    }
                }
                let raw = queries::activity::list_activity(
                    &conn,
                    queries::activity::ActivityScope::Issue(source),
                    None,
                    None,
                )
                .unwrap();
                assert_eq!(
                    raw.items
                        .iter()
                        .filter(|item| {
                            item.old_value.as_deref() == Some("HIDE-1")
                                || item.new_value.as_deref() == Some("HIDE-1")
                        })
                        .count(),
                    6,
                    "the real relation triggers must capture every hidden link/unlink"
                );
            }
            let items = load(&fixture.db, &identity, "ACC-1").unwrap();
            let serialized = serde_json::to_string(&items).unwrap();
            assert!(
                !serialized.contains("HIDE-1"),
                "hidden references entered the authorized Activity model for {role:?}: {serialized}"
            );
            assert_eq!(
                items
                    .iter()
                    .filter(|item| {
                        item.old_value.as_deref() == Some("ACC-2")
                            || item.new_value.as_deref() == Some("ACC-2")
                    })
                    .count(),
                6,
                "all permitted historical relations must survive unchanged"
            );
            let cx = Cx::default();
            let rendered = timeline(&cx, items).single().await.unwrap().render(&cx);
            assert!(
                !rendered.contains("HIDE-1"),
                "hidden references entered HTML or signals"
            );
            assert!(rendered.contains("ACC-2"));
        }
    }

    #[test]
    fn deleted_visible_targets_keep_relation_history() {
        let (fixture, identity) = fixture();
        {
            let conn = fixture.db.write().unwrap();
            let source = queries::resolve_identifier(&conn, "ACC-1").unwrap();
            let allowed = queries::resolve_identifier(&conn, "ACC-2").unwrap();
            queries::link_issues(&conn, source, allowed, "relates_to").unwrap();
            queries::unlink_issues(&conn, source, allowed).unwrap();
            queries::delete_issue(&conn, allowed).unwrap();
            conn.execute(
                "UPDATE issues SET deleted_at='2000-01-01 00:00:00' WHERE id=?1",
                [allowed],
            )
            .unwrap();
            assert_eq!(
                queries::trash::purge_tombstones(&conn, 1).unwrap().issues,
                1
            );
            assert!(queries::resolve_identifier(&conn, "ACC-2").is_err());
        }
        let items = load(&fixture.db, &identity, "ACC-1").unwrap();
        assert!(
            items
                .iter()
                .any(|item| item.action == "link" && item.new_value.as_deref() == Some("ACC-2"))
        );
        assert!(
            items
                .iter()
                .any(|item| item.action == "unlink" && item.old_value.as_deref() == Some("ACC-2"))
        );
    }

    #[tokio::test]
    async fn four_real_issue_audits_render_original_history_and_change_values() {
        let (fixture, identity) = fixture();
        {
            let conn = fixture.db.write().unwrap();
            let id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
            queries::update_issue(
                &conn,
                id,
                &UpdateIssue {
                    title: Some("Production issue initial title".into()),
                    description: Some("# Production markdown\n\nExact initial description.".into()),
                    priority: Some(Priority::Medium),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let items = load(&fixture.db, &identity, "ACC-1").unwrap();
        assert_eq!(
            items.len(),
            4,
            "Same create plus three changed fields as the paired master fixture."
        );
        assert!(items.windows(2).all(|pair| pair[0].id > pair[1].id));
        let expected_ids: Vec<_> = items.iter().map(|item| item.id.to_string()).collect();
        let dom = html(items).await;
        let actual_ids: Vec<_> = dom
            .select(&Selector::parse("li[data-activity-id]").unwrap())
            .map(|item| item.value().attr("data-activity-id").unwrap().to_owned())
            .collect();
        assert_eq!(actual_ids, expected_ids);
        let text = dom.root_element().text().collect::<String>();
        for expected in [
            "Activity",
            "4",
            "system",
            "created this issue",
            "changed title",
            "changed description",
            "changed priority",
            "Visible active initial work",
            "Production issue initial title",
            "none",
            "medium",
            "show change",
            "hide change",
            "# Production markdown\n\nExact initial description.",
            "via system",
        ] {
            assert!(
                text.contains(expected),
                "Missing pinned Activity output {expected:?}: {text}"
            );
        }
        assert_eq!(
            dom.select(&Selector::parse("[data-priority='medium']").unwrap())
                .count(),
            1
        );
        assert_eq!(
            dom.select(&Selector::parse(".native-issue-activity__values[hidden]").unwrap())
                .count(),
            1
        );
        assert_eq!(
            dom.select(&Selector::parse("time[datetime]").unwrap())
                .count(),
            4
        );
    }

    #[test]
    fn fresh_viewer_and_current_account_guard_history_in_the_same_read() {
        let (fixture, identity) = fixture();
        assert!(matches!(
            load(&fixture.db, &identity, "HIDE-1"),
            Err(LificError::Forbidden(_))
        ));
        let user = identity.as_ref().unwrap().user.id;
        fixture
            .db
            .write()
            .unwrap()
            .execute("UPDATE users SET is_active=0 WHERE id=?1", [user])
            .unwrap();
        assert!(matches!(
            load(&fixture.db, &identity, "ACC-1"),
            Err(LificError::Forbidden(_))
        ));
        assert!(matches!(
            load(&fixture.db, &None, "ACC-1"),
            Err(LificError::Forbidden(_))
        ));
    }

    #[tokio::test]
    async fn empty_feed_has_no_header_and_history_defaults_to_six_rows() {
        let (fixture, identity) = fixture();
        let id = queries::resolve_identifier(&fixture.db.read().unwrap(), "ACC-1").unwrap();
        fixture
            .db
            .write()
            .unwrap()
            .execute("DELETE FROM audit_log WHERE issue_id=?1", [id])
            .unwrap();
        let items = load(&fixture.db, &identity, "ACC-1").unwrap();
        assert!(items.is_empty());
        assert_eq!(
            html(items)
                .await
                .select(&Selector::parse("section").unwrap())
                .count(),
            0
        );
        for index in 0..8 {
            queries::update_issue(
                &fixture.db.write().unwrap(),
                id,
                &UpdateIssue {
                    title: Some(format!("Actual edit {index}")),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let items = load(&fixture.db, &identity, "ACC-1").unwrap();
        assert_eq!(items.len(), 8);
        let dom = html(items).await;
        assert_eq!(
            dom.select(&Selector::parse("li:not([hidden])").unwrap())
                .count(),
            6
        );
        assert_eq!(
            dom.select(&Selector::parse("li[hidden]").unwrap()).count(),
            2
        );
        assert!(
            dom.root_element()
                .text()
                .collect::<String>()
                .contains("Show all 8 entries")
        );
    }

    #[tokio::test]
    async fn actor_join_fallbacks_and_untrusted_values_remain_text() {
        let (fixture, identity) = fixture();
        let id = queries::resolve_identifier(&fixture.db.read().unwrap(), "ACC-1").unwrap();
        let user = identity.as_ref().unwrap().user.id;
        {
            let conn = fixture.db.write().unwrap();
            conn.execute(
                "UPDATE users SET display_name='Visible actor' WHERE id=?1",
                [user],
            )
            .unwrap();
            conn.execute(
                "UPDATE audit_log SET actor_user_id=?1,transport='web' WHERE issue_id=?2",
                rusqlite::params![user, id],
            )
            .unwrap();
        }
        let mut items = load(&fixture.db, &identity, "ACC-1").unwrap();
        assert_eq!(actor_name(&items[0]), "Visible actor");
        items[0].actor_display_name = Some(String::new());
        assert_eq!(
            actor_name(&items[0]),
            items[0].actor_username.as_deref().unwrap()
        );
        items[0].actor_username = None;
        assert_eq!(actor_name(&items[0]), "system");
        items[0].action = "update".into();
        items[0].field = Some("title".into());
        items[0].old_value = None;
        items[0].new_value = Some("<img src=x onerror=alert(1)>".into());
        let dom = html(items).await;
        assert_eq!(dom.select(&Selector::parse("img").unwrap()).count(), 0);
        assert!(
            dom.root_element()
                .text()
                .collect::<String>()
                .contains("(none) → <img src=x onerror=alert(1)>")
        );
    }

    #[tokio::test]
    async fn real_actor_deletion_preserves_audit_rows_and_agent_badge() {
        let (fixture, identity) = fixture();
        let user = identity.as_ref().unwrap().user.id;
        let id = queries::resolve_identifier(&fixture.db.read().unwrap(), "ACC-1").unwrap();
        let bot = queries::users::create_bot_user(
            &fixture.db.write().unwrap(),
            user,
            "activity-bot",
            "History agent",
            None,
        )
        .unwrap();
        fixture
            .db
            .write()
            .unwrap()
            .execute(
                "UPDATE audit_log SET actor_user_id=?1,transport='mcp' WHERE issue_id=?2",
                rusqlite::params![bot.id, id],
            )
            .unwrap();
        let before = load(&fixture.db, &identity, "ACC-1").unwrap();
        assert!(before[0].actor_is_bot);
        assert_eq!(actor_name(&before[0]), "History agent");
        let dom = html(before).await;
        assert_eq!(
            dom.select(&Selector::parse(".native-issue-activity__agent").unwrap())
                .count(),
            1
        );
        assert!(
            dom.root_element()
                .text()
                .collect::<String>()
                .contains("via mcp")
        );
        fixture
            .db
            .write()
            .unwrap()
            .execute("DELETE FROM users WHERE id=?1", [bot.id])
            .unwrap();
        let after = load(&fixture.db, &identity, "ACC-1").unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(actor_name(&after[0]), "system");
        assert!(!after[0].actor_is_bot);
        assert_eq!(after[0].transport, "mcp");
    }

    #[test]
    fn source_actor_verbs_and_utf16_short_values_are_preserved() {
        let (fixture, identity) = fixture();
        let mut item = load(&fixture.db, &identity, "ACC-1").unwrap().remove(0);
        for (entity, action, field, value, expected) in [
            ("comment", "create", None, Some("comment"), "commented"),
            (
                "issue_relation",
                "link",
                Some("blocked_by"),
                None,
                "linked blocked by",
            ),
            ("issue", "wait", None, None, "started waiting on"),
            (
                "plan_step",
                "update",
                Some("done"),
                Some("1"),
                "completed a step",
            ),
            (
                "plan_step",
                "auto-reopen",
                None,
                None,
                "reopened a step (issue reopened)",
            ),
            (
                "plan",
                "update",
                Some("anchor_issue"),
                None,
                "cleared the anchor",
            ),
        ] {
            item.entity_type = entity.into();
            item.action = action.into();
            item.field = field.map(str::to_owned);
            item.new_value = value.map(str::to_owned);
            assert_eq!(verb(&item), expected);
        }
        assert_eq!(short_value(None, 40), "(none)");
        assert_eq!(short_value(Some(" \n\n \n "), 40), "(none)");
        assert_eq!(short_value(Some(" one\n\ntwo "), 40), "one two");
        assert_eq!(short_value(Some("a😀b"), 3), "a😀…");
        assert_eq!(short_value(Some("\u{feff}one\u{feff}"), 40), "one");
        assert_eq!(short_value(Some("\u{0085}"), 40), "\u{0085}");
    }
}
