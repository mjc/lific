//! Genuine native status-column Board base inside the persistent workspace.
use super::{icons, navigation};
use crate::db::models::{Priority, Status};
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

pub(crate) const STYLESHEET: &str = include_str!("assets/board.css");

pub(crate) fn content<'a>(
    cx: &'a Cx,
    project: &str,
    pending_issue_ids: &[i64],
) -> topcoat::Result<BoxView<'a>> {
    let (project, mut issues) = super::issue_list::authorized_rows(cx, project, pending_issue_ids)?;
    issues.sort_by(|a, b| {
        priority_rank(a.priority)
            .cmp(&priority_rank(b.priority))
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    let statuses = [
        Status::Backlog,
        Status::Todo,
        Status::Active,
        Status::Done,
        Status::Cancelled,
    ];
    let columns = statuses.map(|status| {
        let cards = issues
            .iter()
            .filter(|issue| issue.status == status)
            .map(|issue| {
                let href = format!("/{}/issues/{}", project.identifier, issue.identifier);
                (issue.clone(), href)
            })
            .collect::<Vec<_>>();
        (status, cards)
    });
    let content = view! {
        cx =>
        <section
            class="native-board"
            data-native-board=(project.identifier.clone())
            aria-label="Issue board"
        >
            <div class="native-board__columns">
                for (status, cards) in columns {
                    <section
                        class="native-board__column"
                        data-native-board-status=(status.as_str())
                        aria-label=(status.as_str())
                    >
                        <header class="native-board__header">
                            (icons::status_icon(cx, status, 14))
                            <h2>(status.as_str())</h2>
                            <span
                                class="native-board__count"
                                data-native-board-count=(cards.len().to_string())
                            >
                                (cards.len().to_string())
                            </span>
                        </header>
                        <div class="native-board__cards">
                            if cards.is_empty() {
                                <p class="native-board__empty">"All quiet"</p>
                            }
                            for (issue, href) in cards {
                                <div class="relative group">
                                    <a
                                        class="native-board__card"
                                        data-native-board-card=(issue.id.to_string())
                                        (navigation::attrs(cx, &href))
                                    >
                                        <div class="native-board__card-top">
                                            <span class="native-board__identifier">
                                                (issue.identifier.clone())
                                            </span>
                                            (icons::priority_icon(cx, issue.priority, 14))
                                        </div>
                                        <h3
                                            class=(if matches!(status, Status::Done | Status::Cancelled) {
                                                "native-board__title native-board__title--closed"
                                            } else {
                                                "native-board__title"
                                            })
                                        >
                                            (issue.title)
                                        </h3>
                                    </a>
                                    <span class="absolute right-8 top-2">
                                        (super::issue_peek::button(cx, &issue.identifier))
                                    </span>
                                </div>
                            }
                        </div>
                    </section>
                }
            </div>
        </section>
    }.boxed();
    Ok(super::home_shell::page_region(
        cx,
        content,
        None,
        "Board".to_owned(),
    ))
}

fn priority_rank(priority: Priority) -> u8 {
    match priority {
        Priority::Urgent => 0,
        Priority::High => 1,
        Priority::Medium => 2,
        Priority::Low => 3,
        Priority::None => 4,
    }
}
