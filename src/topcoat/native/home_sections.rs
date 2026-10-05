//! Original Home right rail. Inputs are display data, never authorization.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use super::{
    home_local::{self, RecentEntry, RecentType},
    transport::mounted_url,
};
use crate::db::models::{Activity, Page, Project};

pub(crate) const STYLESHEET: &str = include_str!("assets/home-sections.css");

/// Pages and activity retain the original selection/caps supplied by Snapshot.
/// The browser-local recent list holds fifteen entries; Home displays eight.
pub(crate) fn right_rail<'a>(
    cx: &'a Cx,
    projects: &[Project],
    pinned_pages: &[Page],
    activity: &[Activity],
    recents: &[RecentEntry],
) -> BoxView<'a> {
    right_rail_with_rate(
        cx,
        projects,
        pinned_pages,
        activity,
        recents,
        view! { cx => }.boxed(),
    )
}

pub(crate) fn right_rail_with_rate<'a>(
    cx: &'a Cx,
    projects: &[Project],
    pinned_pages: &[Page],
    activity: &[Activity],
    recents: &[RecentEntry],
    activity_rate: BoxView<'a>,
) -> BoxView<'a> {
    let has_recents = !recents.is_empty();
    let has_pinned_pages = !pinned_pages.is_empty();
    let has_activity = !activity.is_empty();
    let recent_rows = home_local::display_recents(recents)
        .into_iter()
        .filter_map(|recent| {
            let destination = home_local::recent_route(&recent)?;
            let icon = match recent.kind {
                RecentType::Issue => "lucide:CircleDot",
                RecentType::Page => "lucide:FileText",
                RecentType::Plan => "lucide:ListChecks",
            };
            Some((
                mounted_url(cx, &destination),
                recent.title,
                recent.project,
                icon,
            ))
        })
        .collect::<Vec<_>>();
    let pinned_rows = pinned_pages
        .iter()
        .filter_map(|page| {
            let project = projects
                .iter()
                .find(|project| Some(project.id) == page.project_id)
                .filter(|project| !project.identifier.is_empty())?;
            Some((
                mounted_url(cx, &format!("/{}/pages/{}", project.identifier, page.id)),
                page.title.clone(),
                project.identifier.clone(),
            ))
        })
        .collect::<Vec<_>>();
    let activity_rows = activity
        .iter()
        .map(|event| {
            let row = super::home_activity::activity_row(event, projects);
            (
                row.actor,
                row.verb,
                row.label,
                row.destination
                    .map(|destination| mounted_url(cx, &destination)),
            )
        })
        .collect::<Vec<_>>();
    view! { cx =>
        <aside class="tc-home-sections">
            if has_recents {
                <section data-home-section="recents">
                    (heading(cx, "lucide:History", "Recently viewed"))
                    <div class="tc-home-sections__rows">
                        for (destination, title, project, icon) in recent_rows {
                            <a class="tc-home-sections__row" href=(destination)>
                                <span class="tc-home-sections__icon">(super::icons::project_icon(cx, Some(icon), 13))</span>
                                <span class="tc-home-sections__title">(title)</span>
                                <span class="tc-home-sections__project">(project)</span>
                            </a>
                        }
                    </div>
                </section>
            }
            if has_pinned_pages {
                <section data-home-section="pinned">
                    (heading(cx, "lucide:Pin", "Pinned pages"))
                    <div class="tc-home-sections__rows">
                        for (destination, title, project) in pinned_rows {
                            <a class="tc-home-sections__row" href=(destination)>
                                <span class="tc-home-sections__icon">(super::icons::project_icon(cx, Some("lucide:FileText"), 13))</span>
                                <span class="tc-home-sections__title">(title)</span>
                                <span class="tc-home-sections__project">(project)</span>
                            </a>
                        }
                    </div>
                </section>
            }
            if has_activity {
                <section data-home-section="activity">
                    <div class="tc-home-sections__heading" style="justify-content:space-between">
                        <div style="display:flex;align-items:center;gap:.5rem;min-width:0">
                            <span class="tc-home-sections__icon">(super::icons::project_icon(cx, Some("lucide:ArrowUpRight"), 12))</span>
                            <h2>"Recent activity"</h2>
                        </div>
                        <span style="font-size:.6875rem;color:var(--tc-faint);font-variant-numeric:tabular-nums;text-align:right">(activity_rate)</span>
                    </div>
                    <div class="tc-home-sections__rows">
                        for (actor, verb, label, destination) in activity_rows {
                            if let Some(destination) = destination {
                                <a class="tc-home-sections__activity" href=(destination)>
                                    (activity_text(cx, actor, verb, label))
                                </a>
                            } else {
                                <button type="button" class="tc-home-sections__activity" disabled=(true)>
                                    (activity_text(cx, actor, verb, label))
                                </button>
                            }
                        }
                    </div>
                </section>
            } else {
                <span hidden="hidden">(activity_rate)</span>
            }
        </aside>
    }.boxed()
}

fn heading<'a>(cx: &'a Cx, icon: &'static str, title: &str) -> BoxView<'a> {
    let title = title.to_owned();
    view! { cx =>
        <div class="tc-home-sections__heading">
            <span class="tc-home-sections__icon">(super::icons::project_icon(cx, Some(icon), 12))</span>
            <h2>(title)</h2>
        </div>
    }.boxed()
}

fn activity_text(cx: &Cx, actor: String, verb: String, label: Option<String>) -> BoxView<'_> {
    view! { cx =>
        <span class="tc-home-sections__activity-text">
            <span class="tc-home-sections__actor">(actor)</span>
            " " (verb)
            if let Some(label) = label {
                " " <span class="tc-home-sections__label">(label)</span>
            }
        </span>
    }
    .boxed()
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, sync::Arc};

    use topcoat::{context::CxTestBuilder, router::RemoteAddr, view::ViewExt};

    use super::super::home_local::RecentType;
    use super::*;
    use crate::ratelimit::IpNetwork;

    fn project() -> Project {
        Project {
            id: 7,
            name: "Account project".into(),
            identifier: "ACC".into(),
            description: String::new(),
            emoji: None,
            lead_user_id: None,
            sort_order: 0,
            created_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-03T18:00:00Z".into(),
            is_public: false,
        }
    }

    fn page(project_id: i64, title: &str) -> Page {
        Page {
            id: 42,
            project_id: Some(project_id),
            sequence: Some(42),
            identifier: "ACC-DOC-42".into(),
            folder_id: None,
            title: title.into(),
            content: String::new(),
            sort_order: 0.0,
            status: "active".into(),
            pinned: true,
            created_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-03T18:00:00Z".into(),
            seq: 42,
            labels: Vec::new(),
        }
    }

    fn activity() -> Activity {
        Activity {
            id: 1,
            ts: "2026-10-03T18:00:00Z".into(),
            actor_user_id: Some(2),
            actor_username: Some("alice".into()),
            actor_display_name: Some("Alice <script>".into()),
            actor_is_bot: false,
            transport: "web".into(),
            entity_type: "issue".into(),
            entity_id: 1,
            entity_label: Some("ACC-1".into()),
            project_id: Some(7),
            issue_id: Some(1),
            page_id: None,
            action: "update".into(),
            field: Some("title".into()),
            old_value: None,
            new_value: None,
        }
    }

    fn recent(kind: RecentType, route_id: &str, title: &str) -> RecentEntry {
        RecentEntry {
            kind,
            route_id: route_id.into(),
            identifier: "Not displayed identifier".into(),
            title: title.into(),
            project: "ACC".into(),
            ts: 1_791_050_400_000,
        }
    }

    fn cx(prefix: Option<&str>) -> Cx {
        let mut request = axum::http::Request::builder();
        if let Some(prefix) = prefix {
            request = request.header("x-forwarded-prefix", prefix);
        }
        let (mut parts, ()) = request.body(()).unwrap().into_parts();
        parts
            .extensions
            .insert(RemoteAddr("127.0.0.1:5000".parse::<SocketAddr>().unwrap()));
        let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
        CxTestBuilder::new()
            .request_context(parts)
            .app_context(proxies)
            .build()
    }

    async fn render(
        cx: &Cx,
        projects: &[Project],
        pages: &[Page],
        activity: &[Activity],
        recents: &[RecentEntry],
    ) -> String {
        right_rail(cx, projects, pages, activity, recents)
            .single()
            .await
            .unwrap()
            .render(cx)
    }

    #[tokio::test]
    async fn native_home_sections_render_populated_rows_with_original_copy_and_safe_text() {
        let recents = [
            recent(RecentType::Issue, "ACC-1", "Recent <script>"),
            recent(RecentType::Page, "2", "Recent page"),
            recent(RecentType::Plan, "3", "Recent plan"),
        ];
        let html = render(
            &cx(None),
            &[project()],
            &[page(7, "Pinned <script>")],
            &[activity()],
            &recents,
        )
        .await;
        let recent = html.find("Recently viewed").unwrap();
        let pinned = html.find("Pinned pages").unwrap();
        let activity = html.find("Recent activity").unwrap();
        assert!(recent < pinned && pinned < activity);
        for text in [
            "Recent &lt;script&gt;",
            "Pinned &lt;script&gt;",
            "Alice &lt;script&gt;",
            "changed title on",
            "ACC-1",
            "Recent plan",
        ] {
            assert!(html.contains(text), "missing {text}: {html}");
        }
        assert!(html.contains("width=\"12\""));
        assert!(html.contains("width=\"13\""));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("Not displayed identifier"));
        assert!(!html.contains("2026-10-03T18:00:00Z"));
        assert!(!html.contains("just now"));
    }

    #[tokio::test]
    async fn native_home_sections_omit_all_empty_sections_without_new_empty_copy() {
        let html = render(&cx(None), &[], &[], &[], &[]).await;
        assert!(html.contains("<aside"));
        assert!(!html.contains("<section"));
        for heading in ["Recently viewed", "Pinned pages", "Recent activity"] {
            assert!(!html.contains(heading));
        }
    }

    #[tokio::test]
    async fn native_home_sections_links_use_logical_routes_with_root_app_and_acc_mounts() {
        let recents = [
            recent(RecentType::Issue, "ACC-1", "Issue"),
            recent(RecentType::Page, "2", "Page"),
            recent(RecentType::Plan, "3", "Plan"),
        ];
        for prefix in [None, Some("/app"), Some("/ACC")] {
            let html = render(
                &cx(prefix),
                &[project()],
                &[page(7, "Pinned page")],
                &[activity()],
                &recents,
            )
            .await;
            for route in [
                "/ACC/issues/ACC-1",
                "/ACC/pages/2",
                "/ACC/plans/3",
                "/ACC/pages/42",
            ] {
                let expected = format!("href=\"{}{route}\"", prefix.unwrap_or(""));
                assert!(html.contains(&expected), "missing {expected}: {html}");
            }
            assert!(!html.contains("href=\"/ACC/pages/ACC-DOC-42\""));
            assert!(!html.contains("/api/"));
        }
    }

    #[tokio::test]
    async fn native_home_sections_keep_pinned_header_when_selected_project_is_missing() {
        let html = render(
            &cx(None),
            &[project()],
            &[page(999, "Missing project page")],
            &[],
            &[],
        )
        .await;
        assert!(html.contains("Pinned pages"));
        assert!(!html.contains("Missing project page"));
        assert!(!html.contains("href="));
        assert!(!html.contains("Recently viewed"));
        assert!(!html.contains("Recent activity"));
    }

    #[tokio::test]
    async fn native_home_sections_disable_unknown_activity_and_escape_labels() {
        let mut unknown = activity();
        unknown.entity_type = "project".into();
        unknown.entity_label = Some("<img onerror=alert(1)>".into());
        let html = render(&cx(None), &[project()], &[], &[unknown], &[]).await;
        assert!(html.contains("Recent activity"));
        assert!(html.contains("<button"));
        assert!(html.contains("type=\"button\""));
        assert!(html.contains("disabled"));
        assert!(html.contains("&lt;img onerror=alert(1)&gt;"));
        assert!(!html.contains("<img"));
        assert!(!html.contains("href="));
    }

    #[tokio::test]
    async fn native_home_sections_display_only_first_eight_recents_in_stored_order() {
        let recents = (1..=9)
            .map(|id| {
                recent(
                    RecentType::Page,
                    &id.to_string(),
                    &format!("Recent entry {id}"),
                )
            })
            .collect::<Vec<_>>();
        let html = render(&cx(None), &[project()], &[], &[], &recents).await;
        assert_eq!(html.matches("href=").count(), 8);
        assert!(html.find("Recent entry 1").unwrap() < html.find("Recent entry 8").unwrap());
        assert!(!html.contains("Recent entry 9"));
    }
}
