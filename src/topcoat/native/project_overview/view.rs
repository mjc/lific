//! Pinned overview hero, attention and activity presentation with shared native chrome.
use super::super::icons::UiIcon;
use super::super::{context, dates, icons, session, transport};
use super::{
    controls::{self, Controls},
    model,
};
use crate::{
    db::models::{Activity, IssueStatusCounts, Priority},
    services::project_overview::OverviewReads,
};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, shard, signal},
    view::{BoxView, View, ViewExt, view},
};

pub(super) fn topbar<'a>(cx: &'a Cx, identifier: &str) -> BoxView<'a> {
    let identifier = identifier.to_owned();
    let (export_error, export_button) = super::export::toolbar_fragments(cx, &identifier);
    view! { cx => <div class="native-overview__topbar"><div class="native-overview__breadcrumb"><a href=(transport::mounted_url(cx, &format!("/{identifier}/issues")))>(identifier.clone())</a>(icons::ui_icon(cx,UiIcon::BreadcrumbSeparator,12))<span>"Overview"</span></div><div class="native-overview__topbar-actions">(export_error)(export_button)</div></div> }.boxed()
}

pub(super) fn content<'a>(
    cx: &'a Cx,
    reads: &OverviewReads,
    controls: &Controls,
    notice: Option<String>,
    continuation: Option<&super::management_model::Continuation>,
    revision: Signal<usize>,
) -> topcoat::Result<BoxView<'a>> {
    let can = model::capabilities(
        reads.enforced,
        reads.user.is_admin,
        reads.role,
        reads.project.lead_user_id == Some(reads.user.id),
    );
    let project = &reads.project;
    let identifier = project.identifier.clone();
    let clock = signal(cx, || chrono::Utc::now().timestamp_millis() as f64);
    // Ranking uses the initial browser date. TimeAgo's own shared clock ticks independently.
    let rank_clock = signal(cx, || chrono::Utc::now().timestamp_millis() as f64);
    let saved = controls.saved.clone();
    let error = controls.error.clone();
    let counts = reads.counts.as_ref().ok();
    let activity = reads.activity.as_ref().ok();
    let activity_first = activity.and_then(|items| items.first());
    let activities = activity
        .map(|items| items.iter().take(8).cloned().collect())
        .unwrap_or_default();
    let archive = if can.publish {
        Some(super::archive::panel(
            cx,
            reads.user.id,
            project.id,
            &identifier,
        )?)
    } else {
        None
    };
    // Construct child views while borrowed reads and continuations are available.
    // The lazy parent captures these owned views and primitive display values.
    let icon_class = if can.manage {
        "native-overview__icon"
    } else {
        "native-overview__icon native-overview__icon--readonly"
    };
    let hero_icon = if can.manage {
        controls::icon(cx, controls)
    } else if let Some(emoji) = project.emoji.as_deref().filter(|value| !value.is_empty()) {
        icons::project_icon(cx, Some(emoji), 22)
    } else {
        let initials = identifier.chars().take(2).collect::<String>();
        view! { cx => <span>(initials)</span> }.boxed()
    };
    let hero_name = controls::name(cx, controls, can.manage);
    let hero_identifier = controls::copy_identifier(cx, &identifier);
    let description = controls::description(cx, controls, can.manage);
    let read_only = !can.manage && reads.enforced;
    let created = dates::absolute(cx, &project.created_at);
    let active = activity_first.map(|activity| dates::relative(cx, &activity.ts, clock.clone()).0);
    let completion = counts
        .filter(|counts| counts.total > 0)
        .map(|counts| completion(cx, counts));
    let group = reads
        .groups
        .as_ref()
        .ok()
        .map(|groups| controls::group(cx, controls, project, groups));
    let labels = super::labels::panel(
        cx,
        reads.user.id,
        project.id,
        &identifier,
        reads.labels.as_ref().map_or(0, Vec::len),
        can.edit,
    );
    let publication = if can.publish {
        Some(super::publish::panel(cx, reads.user.id, project))
    } else {
        None
    };
    let members = if can.manage {
        Some(super::members::panel(cx, reads, continuation))
    } else {
        None
    };
    let import = if can.manage {
        Some(super::import::panel(
            cx,
            reads.user.id,
            project.id,
            revision,
        ))
    } else {
        None
    };
    let danger = if can.manage {
        Some(super::danger::panel(cx, reads, continuation))
    } else {
        None
    };
    let clock_mount = dates::clock_mount(cx, clock.clone());
    let recent = recent_activity(cx, &identifier, activities, clock);
    let content = view! { cx => <div class="native-overview" (clock_mount) @mount=$(|_event: Event| rank_clock.set(raw!("cx.hydrate(Date.now())",0.0)))>
        <div class="native-overview__column">
            if let Some(notice) = notice { <div role="alert" class="native-overview__notice" data-native-project-notice="">(notice)</div> }
            <section class="native-overview__hero">
                <div class=(icon_class)>
                    (hero_icon)
                </div>
                <div class="native-overview__identity">
                    <div class="native-overview__name-row">(hero_name)(hero_identifier)
                        <span class="native-overview__saved" role="status" :hidden=$(!saved.get())>(icons::ui_icon(cx,UiIcon::Saved,11))" Saved"</span>
                        if read_only { <span class="native-overview__readonly" title="Only a project lead or admin can change project settings.">"Read-only"</span> }
                    </div>
                    (description)
                    <div class="native-overview__dates"><span>"Created "(created)</span>
                        if let Some(active) = active { <span>"·"</span><span>"Active "(active)</span> }
                    </div>
                </div>
                if let Some(completion) = completion { (completion) }
            </section>
            <div role="alert" class="native-overview__error" :hidden=$(error.get().is_empty())>$(error.get())</div>
            native_overview_attention(identifier: identifier.clone(), browser_milliseconds: $(rank_clock.get()))
            if let Some(group) = group { (group) }
            (labels)
            if let Some(publication) = publication { (publication) }
            if let Some(archive) = archive { (archive) }
            if let Some(members) = members { (members) }
            if let Some(import) = import { (import) }
            (recent)
            if let Some(danger) = danger { (danger) }
            <div class="native-overview__bottom-space" aria-hidden="true"></div>
        </div>
    </div> }.boxed();
    Ok(content)
}

fn completion<'a>(cx: &'a Cx, counts: &IssueStatusCounts) -> BoxView<'a> {
    let done = counts.done;
    let total = counts.total;
    let fraction = (done as f64 / total as f64).clamp(0.0, 1.0);
    let circumference = std::f64::consts::TAU * 23.5;
    let percent = (fraction * 100.0).round() as i64;
    let offset = circumference * (1.0 - fraction);
    let mounted = signal(cx, || false);
    view! { cx => <div class="native-overview__completion"><div class="native-overview__ring" role="progressbar" aria-valuenow=(percent.to_string()) aria-valuemin="0" aria-valuemax="100" @mount=$(|_event: Event| { let _paint = || mounted.set(true); raw!("requestAnimationFrame(()=>${_paint}());",()); })>
        <svg width="52" height="52" viewBox="0 0 52 52" aria-hidden="true"><circle cx="26" cy="26" r="23.5" fill="none" stroke="var(--tc-border)" stroke-width="5"></circle>
        <circle class="native-overview__arc" cx="26" cy="26" r="23.5" fill="none" stroke="var(--tc-success)" stroke-width="5" stroke-linecap="round" stroke-dasharray=(circumference.to_string()) :stroke-dashoffset=$(if mounted.get() { offset } else { circumference })></circle></svg>
        <div>(percent)<span>"%"</span></div>
    </div><span>(format!("{done}/{total} done"))</span></div> }.boxed()
}

#[shard("/__native_overview/attention")]
async fn native_overview_attention(
    cx: &Cx,
    identifier: String,
    browser_milliseconds: f64,
) -> topcoat::Result<impl View> {
    let caller = session::read(cx, context::caller(cx))?;
    let reads = session::read(
        cx,
        crate::services::project_overview::load(context::db(cx), &caller.identity, &identifier),
    )?;
    let now = if browser_milliseconds.is_finite() {
        browser_milliseconds as i64
    } else {
        chrono::Utc::now().timestamp_millis()
    };
    let issues = reads.issues.unwrap_or_else(|error| {
        tracing::warn!(error=%error,"overview attention read failed");
        Vec::new()
    });
    let attention = model::attention(&issues, now);
    let more = attention.more;
    let rows = attention
        .issues
        .into_iter()
        .map(|issue| {
            let age = model::days_since(&issue.created_at, now);
            let idle = model::days_since(&issue.updated_at, now);
            let heat = if issue.priority == Priority::Urgent {
                "var(--tc-error)"
            } else if issue.priority == Priority::High || idle >= 14 {
                "var(--tc-warn)"
            } else {
                "var(--tc-faint)"
            };
            (
                issue.identifier.clone(),
                issue.title.clone(),
                issue.priority,
                issue.status,
                model::age_label(age),
                if idle >= 14 {
                    Some(model::age_label(idle))
                } else {
                    None
                },
                heat,
            )
        })
        .collect::<Vec<_>>();
    Ok(
        view! {cx => <section data-native-overview-attention=""><div class="native-overview__heading"><h2>"Needs attention"</h2>
            if more>0 {<a href=(transport::mounted_url(cx,&format!("/{identifier}/issues")))>(format!("+{more} more open"))(icons::ui_icon(cx,UiIcon::Forward,11))</a>}
            </div>
            if rows.is_empty(){<div class="native-overview__empty"><span>(icons::ui_icon(cx,UiIcon::AllClear,16))</span><div><p>"Nothing needs attention"</p><p>"Everything open is fresh and on track."</p></div></div>}
            else {<div class="native-overview__attention-card">for (issue,title,priority,status,age,idle,heat) in rows {
                <a class="native-overview__attention-row" href=(transport::mounted_url(cx,&format!("/{identifier}/issues/{issue}")))>
                    <span class="native-overview__heat" style=(format!("background:{heat}"))></span>(icons::priority_icon(cx,priority,15))<span class="native-overview__issue-id">(issue)</span><span class="native-overview__issue-title">(title)</span>
                    <div class="native-overview__issue-cues"><span>(format!("open {age}"))</span>if let Some(idle)=idle{<span class="native-overview__idle">(format!("idle {idle}"))</span>}(icons::status_icon(cx,status,14))</div>
                </a>
            }</div>}
        </section>},
    )
}

fn recent_activity<'a>(
    cx: &'a Cx,
    identifier: &str,
    items: Vec<Activity>,
    now: Signal<f64>,
) -> BoxView<'a> {
    let identifier = identifier.to_owned();
    let rows = items
        .into_iter()
        .map(|item| {
            (
                model::actor_name(&item).to_owned(),
                model::activity_text(&item),
                dates::relative(cx, &item.ts, now.clone()).0,
                item.id,
            )
        })
        .collect::<Vec<_>>();
    view! {cx => if !rows.is_empty(){<section data-native-overview-activity=""><div class="native-overview__heading"><h2>"Recent activity"</h2><a href=(transport::mounted_url(cx,&format!("/{identifier}/activity")))>(icons::ui_icon(cx,UiIcon::History,11))" Full log"</a></div>
        <div class="native-overview__activity-rows">for (actor,text,time,id) in rows{<div class="native-overview__activity-row" data-activity-id=(id.to_string())><span></span><p><strong>(actor)</strong>" "(text)" "<span>"· "(time)</span></p></div>}</div>
    </section>} }.boxed()
}
