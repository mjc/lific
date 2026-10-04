//! Native Home chrome. Display snapshots never authorize palette reads.

use super::home_data::Snapshot;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, connected, shard, signal},
    view::{BoxView, View, ViewExt, view},
};

pub(crate) const STYLESHEET: &str = include_str!("assets/home-shell.css");

pub(crate) fn shell<'a>(cx: &'a Cx, snapshot: &Snapshot, content: BoxView<'a>) -> BoxView<'a> {
    shell_with_palette(cx, snapshot, content, signal(cx, || false))
}

pub(crate) fn shell_with_palette<'a>(
    cx: &'a Cx,
    snapshot: &Snapshot,
    content: BoxView<'a>,
    palette_open: Signal<bool>,
) -> BoxView<'a> {
    let collapsed = signal(cx, || false);
    let query = signal(cx, String::new);
    let projects = snapshot.projects.clone();
    let display_name = if snapshot.user.display_name.is_empty() {
        snapshot.user.username.clone()
    } else {
        snapshot.user.display_name.clone()
    };
    view! { cx =>
        <div class="native-home-shell" :data-collapsed=$(if collapsed.get() { "true" } else { "false" })>
            <aside class="native-home-sidebar" aria-label="Workspace sidebar">
                <a class="native-home-brand" href=(super::transport::mounted_url(cx, "/"))>"Lific"</a>
                <button id="native-home-palette-open" class="native-home-launcher" @click=$(|_event| palette_open.set(true))>
                    (super::icons::project_icon(cx, Some("lucide:Search"), 16)) "Jump to…"
                </button>
                <nav aria-label="Workspace">
                    <a class="native-home-destination" href=(super::transport::mounted_url(cx, "/")) aria-current="page">
                        (super::icons::project_icon(cx, Some("lucide:House"), 16)) "Home"
                    </a>
                    <div class="native-home-project-heading">"Projects"</div>
                    for project in projects {
                        <section class="native-home-project">
                            <a class="native-home-destination native-home-project-title" href=(super::transport::mounted_url(cx, &format!("/{}/overview", project.identifier)))>
                                (super::icons::project_icon(cx, project.emoji.as_deref().filter(|value| !value.is_empty()).or(Some("lucide:Folder")), 16))
                                <span>(project.name)</span>
                            </a>
                            <div class="native-home-project-links">
                                for (suffix, title, icon) in [("issues", "Issues", "lucide:CircleDot"), ("pages", "Pages", "lucide:FileText"), ("plans", "Plans", "lucide:Map")] {
                                    <a class="native-home-destination" href=(super::transport::mounted_url(cx, &format!("/{}/{suffix}", project.identifier)))>
                                        (super::icons::project_icon(cx, Some(icon), 14)) (title)
                                    </a>
                                }
                            </div>
                        </section>
                    }
                </nav>
                <a class="native-home-account native-home-destination" href=(super::transport::mounted_url(cx, "/settings"))>
                    (super::icons::project_icon(cx, Some("lucide:UserRound"), 18)) <span>(display_name)</span>
                </a>
            </aside>
            <div class="native-home-body">
                <header class="native-home-topbar">
                    <button id="native-home-collapse" class="native-home-icon-button" aria-label="Toggle sidebar" :aria-expanded=$(if collapsed.get() { "false" } else { "true" }) @click=$(|_event| collapsed.set(!collapsed.get()))>
                        (super::icons::project_icon(cx, Some("lucide:PanelLeft"), 18))
                    </button>
                    <span>"Home"</span>
                </header>
                <main class="native-home-panel">(content)</main>
            </div>
            <div class="native-home-palette-backdrop" :hidden=$(!palette_open.get())>
                <section class="native-home-palette" role="dialog" aria-modal="true" aria-labelledby="native-home-palette-title">
                    <header><h2 id="native-home-palette-title">"Jump to project"</h2>
                        <button id="native-home-palette-close" class="native-home-icon-button" aria-label="Close project search" @click=$(|_event| palette_open.set(false))>
                            (super::icons::project_icon(cx, Some("lucide:X"), 18))
                        </button>
                    </header>
                    <label for="native-home-palette-query">"Search visible projects"</label>
                    <input id="native-home-palette-query" type="search" maxlength="128" autocomplete="off" :value=$(query.get()) @input=$(|event: Event| query.set(event.target.value))>
                    native_home_palette_results(query: $(query.get()), open: $(palette_open.get()))
                </section>
            </div>
        </div>
    }.boxed()
}

fn matching_projects<'a>(
    projects: &'a [crate::db::models::Project],
    query: &str,
) -> Vec<&'a crate::db::models::Project> {
    let query = query
        .trim()
        .chars()
        .take(128)
        .collect::<String>()
        .to_lowercase();
    projects
        .iter()
        .filter(|project| {
            project.name.to_lowercase().contains(&query)
                || project.identifier.to_lowercase().contains(&query)
        })
        .take(20)
        .collect()
}

#[shard("/__native_home/palette")]
async fn native_home_palette_results(
    cx: &Cx,
    query: String,
    open: bool,
) -> topcoat::Result<impl View> {
    let connected = connected(cx);
    // A closed dialog renders no catalog. Opening and every query resolve current authority.
    let projects = if open {
        let caller = super::context::caller(cx)?;
        crate::api::require_user(&caller.identity)?;
        crate::services::projects::list_visible_projects(super::context::db(cx), &caller.identity)?
    } else {
        Vec::new()
    };
    let matches = matching_projects(&projects, &query)
        .into_iter()
        .map(|project| (project.identifier.clone(), project.name.clone()))
        .collect::<Vec<_>>();
    Ok(view! {
        <nav class="native-home-palette-results" aria-label="Project search results" data-native-home-connected=(if connected { "true" } else { "false" })>
            if open && matches.is_empty() { <p>"No matching projects"</p> }
            for (identifier, name) in matches {
                <a class="native-home-destination" href=(super::transport::mounted_url(cx, &format!("/{identifier}/overview")))>
                    (super::icons::project_icon(cx, Some("lucide:Folder"), 16)) <span>(name)</span><small>(identifier)</small>
                </a>
            }
        </nav>
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        self,
        models::{AuthUser, CreateProject},
        queries,
    };
    use topcoat::context::CxTestBuilder;

    fn snapshot() -> Snapshot {
        let db = db::open_memory().unwrap();
        let conn = db.write().unwrap();
        let projects = [("ACC", "Accounts <script>"), ("DCS", "Documents")]
            .into_iter()
            .map(|(identifier, name)| {
                queries::create_project(
                    &conn,
                    &CreateProject {
                        identifier: identifier.into(),
                        name: name.into(),
                        ..Default::default()
                    },
                )
                .unwrap()
            })
            .collect();
        Snapshot {
            user: AuthUser {
                id: 1,
                username: "member".into(),
                display_name: "Member".into(),
                is_admin: false,
            },
            projects,
            issues: Vec::new(),
            pinned_pages: Vec::new(),
            activity: Vec::new(),
        }
    }

    #[test]
    fn palette_matches_visible_catalog_name_or_identifier_and_preserves_order() {
        let data = snapshot();
        assert_eq!(
            matching_projects(&data.projects, " acc ")
                .iter()
                .map(|p| p.identifier.as_str())
                .collect::<Vec<_>>(),
            ["ACC"]
        );
        assert_eq!(
            matching_projects(&data.projects, "DOCUMENT")
                .iter()
                .map(|p| p.identifier.as_str())
                .collect::<Vec<_>>(),
            ["DCS"]
        );
        assert_eq!(matching_projects(&data.projects, "").len(), 2);
        assert!(matching_projects(&data.projects, "missing").is_empty());
    }

    #[topcoat::view::component]
    async fn shell_fixture(cx: &Cx, empty_name: bool) -> topcoat::Result<impl View> {
        let mut data = snapshot();
        if empty_name {
            data.user.display_name.clear();
        }
        let content = view! { cx => <p>"Actual Home content"</p> }.boxed();
        Ok(shell(cx, &data, content))
    }

    #[tokio::test]
    async fn shell_account_uses_username_when_display_name_is_empty() {
        let cx = CxTestBuilder::new().build();
        let outer = view! { cx => shell_fixture(empty_name: true) };
        let html = outer.single().await.unwrap().render(&cx);
        let account = html
            .split("native-home-account")
            .nth(1)
            .expect("account link");
        let account = account.split("</a>").next().unwrap();
        assert!(
            account.contains("member"),
            "account link must name the member: {account}"
        );
    }

    #[tokio::test]
    async fn shell_populates_safe_project_navigation_and_native_controls() {
        let cx = CxTestBuilder::new().build();
        let outer = view! { cx => shell_fixture(empty_name: false) };
        let html = outer.single().await.unwrap().render(&cx);
        for expected in [
            "Accounts &lt;script&gt;",
            "href=\"/ACC/overview\"",
            "href=\"/DCS/issues\"",
            "href=\"/settings\"",
            "Actual Home content",
            "native-home-collapse",
            "native-home-palette-open",
            "native-home-palette-query",
            "native-home-palette-close",
            "data-topcoat-on:click",
            "data-topcoat-on:input",
        ] {
            assert!(html.contains(expected), "missing {expected}");
        }
        assert!(!html.contains("<script>"));
        assert!(!html.contains("data-lific-"));
    }
}
