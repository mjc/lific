//! Home presentation derived from the original Svelte page.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use super::{
    home_model::HomeModel,
    icons::{priority_icon, status_icon},
    navigation,
    transport::mounted_url,
};

pub(crate) const MASCOT_PATH: &str = "/__native_home/mascot.png";
pub(crate) const MASCOT: &[u8] = include_bytes!("assets/sleeping-lizzy.png");

pub(crate) const STYLESHEET: &str = include_str!("assets/home.css");

pub(crate) fn active_work<'a>(cx: &'a Cx, model: HomeModel<'_>) -> BoxView<'a> {
    let count = model.active_issue_count;
    let groups = model
        .issue_groups
        .into_iter()
        .map(|group| {
            let rows = group
                .visible
                .into_iter()
                .map(|issue| {
                    (
                        issue.identifier.clone(),
                        issue.title.clone(),
                        issue.status,
                        issue.priority,
                    )
                })
                .collect::<Vec<_>>();
            (
                group.project.identifier.clone(),
                group.project.name.clone(),
                group
                    .project
                    .emoji
                    .clone()
                    .filter(|value| !value.is_empty()),
                group.total,
                rows,
            )
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <div class="tc-dashboard__main tc-home-active">
            <div class="tc-home-active__heading">
                <h2>"My active issues"</h2>
                <span data-home-active-count=(count.to_string())>(count)</span>
            </div>
            if groups.is_empty() {
                <div class="tc-home-active__empty">
                    <span
                        class="tc-home-active__mascot"
                        aria-hidden="true"
                        style=(format!(
                            "mask-image:url('{}')",
                            mounted_url(cx, MASCOT_PATH),
                        ))
                    ></span>
                    <p>"All quiet here"</p>
                    <p>
                        "Nothing active or todo assigned to you across your projects right now."
                    </p>
                </div>
            } else {
                for (project_identifier, name, emoji, total, rows) in groups {
                    <section class="tc-dashboard__card">
                        <a
                            class="tc-home-active__project"
                            (navigation::attrs(
                                cx,
                                &format!("/{project_identifier}/overview"),
                            ))
                        >
                            if let Some(emoji) = emoji {
                                (super::icons::project_icon(cx, Some(&emoji), 15))
                            } else {
                                <span class="tc-home-active__initials">
                                    (project_identifier.chars().take(2).collect::<String>())
                                </span>
                            }
                            <span class="tc-home-active__project-name">(name)</span>
                            <span class="tc-home-active__total">(total)</span>
                        </a>
                        for (identifier, title, status, priority) in rows {
                            <a
                                class="tc-dashboard__issue"
                                (navigation::attrs(
                                    cx,
                                    &format!("/{project_identifier}/issues/{identifier}"),
                                ))
                            >
                                (status_icon(cx, status, 14))
                                <span class="tc-dashboard__identifier">(identifier)</span>
                                <span class="tc-dashboard__issue-title">(title)</span>
                                (priority_icon(cx, priority, 15))
                            </a>
                        }
                        if total > 6 {
                            <a
                                class="tc-home-active__overflow"
                                (navigation::attrs(
                                    cx,
                                    &format!("/{project_identifier}/issues"),
                                ))
                            >
                                (format!("View all {total} in {project_identifier}"))
                                <svg
                                    width="11"
                                    height="11"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                    aria-hidden="true"
                                >
                                    <path d="M7 7h10v10"></path>
                                    <path d="M7 17 17 7"></path>
                                </svg>
                            </a>
                        }
                    </section>
                }
            }
        </div>
    }
    .boxed()
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, sync::Arc};

    use topcoat::{context::CxTestBuilder, router::RemoteAddr, view::ViewExt};

    use super::super::home_model::derive_home;
    use super::*;
    use crate::{
        db::{self, models::*, queries},
        ratelimit::IpNetwork,
    };

    #[tokio::test]
    async fn active_work_renders_initial_rows_with_counts_safe_text_and_mounted_links() {
        let db = db::open_memory().unwrap();
        let (project, issues) = {
            let conn = db.write().unwrap();
            let project = queries::create_project(
                &conn,
                &CreateProject {
                    name: "Project <script>".into(),
                    identifier: "ACC".into(),
                    emoji: Some(String::new()),
                    ..Default::default()
                },
            )
            .unwrap();
            let issues = (0..7)
                .map(|index| {
                    let mut issue = queries::create_issue(
                        &conn,
                        &CreateIssue {
                            project_id: project.id,
                            title: format!("Work {index} <script>"),
                            status: Status::Active,
                            priority: Priority::High,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    // Render ordering uses a fixed input clock; read ordering is tested separately.
                    issue.updated_at = "2026-10-03 16:00:00".into();
                    issue
                })
                .collect::<Vec<_>>();
            (project, issues)
        };
        let projects = [project];
        let (mut parts, ()) = axum::http::Request::builder()
            .header("x-forwarded-prefix", "/ACC")
            .body(())
            .unwrap()
            .into_parts();
        parts
            .extensions
            .insert(RemoteAddr("127.0.0.1:5000".parse::<SocketAddr>().unwrap()));
        let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
        let cx = CxTestBuilder::new()
            .request_context(parts)
            .app_context(proxies)
            .build();
        let html = active_work(&cx, derive_home(&projects, &issues))
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(html.contains("My active issues"));
        assert!(html.contains("data-home-active-count=\"7\""));
        assert!(html.contains("Project &lt;script&gt;"));
        assert!(html.contains("class=\"tc-home-active__initials\">AC</span>"));
        assert!(!html.contains("<script>"));
        assert_eq!(html.matches("class=\"tc-dashboard__issue\"").count(), 6);
        assert!(html.contains("href=\"/ACC/ACC/issues/ACC-1\""));
        assert!(html.contains("href=\"/ACC/ACC/overview\""));
        assert!(html.contains("href=\"/ACC/ACC/issues\""));
        assert_eq!(html.matches("data-topcoat-link=\"intent\"").count(), 8);
        assert!(html.contains("View all 7 in ACC"));
        assert!(!html.contains("Loading your dashboard"));
    }

    #[tokio::test]
    async fn empty_work_keeps_original_copy_and_mascot_without_issue_links() {
        let cx = Cx::default();
        let html = active_work(&cx, derive_home(&[], &[]))
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(html.contains("All quiet here"));
        assert!(html.contains("Nothing active or todo assigned to you across your projects"));
        assert!(html.contains("data-home-active-count=\"0\""));
        assert!(html.contains("/__native_home/mascot.png"));
        assert!(!html.contains("class=\"tc-dashboard__issue\""));
    }
}
