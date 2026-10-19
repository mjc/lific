//! Server populated Insights cards using the pinned Main presentation.
use super::super::icons::UiIcon;
use super::super::{avatar, dates, icons, transport};
use crate::db::models::{ActorStat, InsightsPayload, Priority, Status};
use topcoat::{
    context::Cx,
    runtime::{Signal, signal},
    view::{BoxView, ViewExt, view},
};

pub(super) fn content<'a>(cx: &'a Cx, data: &InsightsPayload) -> BoxView<'a> {
    if data.status_counts.total == 0 {
        let mascot = transport::mounted_url(cx, "/__native_home/mascot.png");
        return view! {
            cx =>
            <div
                class="native-insights h-full min-h-0 overflow-y-auto leading-[1.6] text-[var(--tc-text)]"
            >
                <div
                    class="native-insights__empty flex flex-col items-center py-20 gap-4 px-6 max-w-[480px] mx-auto text-center"
                >
                    <div
                        class="native-insights__mascot shrink-0 w-[250px] h-[105px] opacity-50 bg-[var(--tc-faint)]"
                        aria-hidden="true"
                        style=(format!(
                            "mask:url({mascot}) center / contain no-repeat;-webkit-mask:url({mascot}) center / contain no-repeat",
                        ))
                    ></div>
                    <div class="flex flex-col items-center gap-1.5">
                        <p class="text-heading font-medium text-[var(--tc-text)] m-0">
                            "Nothing to chart yet"
                        </p>
                        <p
                            class="text-body-sm text-[var(--tc-muted)] leading-relaxed m-0"
                        >
                            "Insights fills in once this project has issues to measure — creation trends, closures, and who's been doing the work."
                        </p>
                    </div>
                </div>
            </div>
        }.boxed();
    }
    let s = &data.status_counts;
    let p = &data.priority_counts;
    let status = distribution(
        cx,
        vec![
            (
                "Backlog".into(),
                s.backlog,
                Some(icons::status_icon(cx, Status::Backlog, 13)),
            ),
            (
                "Todo".into(),
                s.todo,
                Some(icons::status_icon(cx, Status::Todo, 13)),
            ),
            (
                "Active".into(),
                s.active,
                Some(icons::status_icon(cx, Status::Active, 13)),
            ),
            (
                "Done".into(),
                s.done,
                Some(icons::status_icon(cx, Status::Done, 13)),
            ),
            (
                "Cancelled".into(),
                s.cancelled,
                Some(icons::status_icon(cx, Status::Cancelled, 13)),
            ),
        ],
        "Nothing yet",
    );
    let priority = distribution(
        cx,
        vec![
            (
                "Urgent".into(),
                p.urgent,
                Some(icons::priority_icon(cx, Priority::Urgent, 13)),
            ),
            (
                "High".into(),
                p.high,
                Some(icons::priority_icon(cx, Priority::High, 13)),
            ),
            (
                "Medium".into(),
                p.medium,
                Some(icons::priority_icon(cx, Priority::Medium, 13)),
            ),
            (
                "Low".into(),
                p.low,
                Some(icons::priority_icon(cx, Priority::Low, 13)),
            ),
            (
                "None".into(),
                p.none,
                Some(icons::priority_icon(cx, Priority::None, 13)),
            ),
        ],
        "Nothing yet",
    );
    let modules = distribution(
        cx,
        data.module_counts
            .iter()
            .take(6)
            .map(|module| (module.name.clone(), module.count, None))
            .collect(),
        "No modules yet",
    );
    let overflow = data.module_counts.len().saturating_sub(6);
    let module_body = view! {
        cx =>
        (modules)
        if overflow > 0 {
            <p
                class="native-insights__overflow text-micro text-[var(--tc-faint)] mt-2 mb-0"
            >
                (format!("+{overflow} more"))
            </p>
        }
    }
    .boxed();
    let status = distribution_card(cx, "Status", status);
    let priority = distribution_card(cx, "Priority", priority);
    let modules = distribution_card(cx, "Module", module_body);
    let window = format!("last {} weeks", data.weeks);
    let chart = super::chart::chart(cx, &data.created_per_week, &data.closed_per_week);
    let clock = signal(cx, || chrono::Utc::now().timestamp_millis() as f64);
    let clock_mount = dates::clock_mount(cx, clock.clone());
    let actors = actor_list(cx, &data.top_actors, clock);
    view! {
        cx =>
        <div
            class="native-insights h-full min-h-0 overflow-y-auto leading-[1.6] text-[var(--tc-text)]"
            (clock_mount)
        >
            <div
                class="native-insights__content max-w-[1100px] mx-auto px-6 py-6 flex flex-col gap-5"
            >
                <section
                    class="native-insights__card native-insights__hero rounded-xl bg-[var(--tc-surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] p-5"
                >
                    <div
                        class="native-insights__heading flex items-center gap-2 mb-4 text-[var(--tc-muted)]"
                    >
                        (icons::ui_icon(cx, UiIcon::Insights, 15))
                        <h2
                            class="text-body-lg font-semibold text-[var(--tc-text)] m-0 leading-[1.2] tracking-[-0.02em]"
                        >
                            "Created vs. closed"
                        </h2>
                        <span
                            class="text-micro text-[var(--tc-faint)] tabular-nums ml-auto"
                        >
                            (window.clone())
                        </span>
                    </div>
                    (chart)
                </section>
                <div
                    class="native-insights__distributions grid grid-cols-1 md:grid-cols-3 gap-4 items-stretch"
                >
                    (status)
                    (priority)
                    (modules)
                </div>
                <section
                    class="native-insights__card native-insights__actors rounded-xl bg-[var(--tc-surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] p-4"
                >
                    <div
                        class="native-insights__heading flex items-center gap-2 mb-2 text-[var(--tc-muted)]"
                    >
                        (icons::ui_icon(cx, UiIcon::Members, 14))
                        <h3
                            class="text-micro font-semibold uppercase tracking-widest text-[var(--tc-faint)] m-0 leading-[1.2]"
                        >
                            "Top actors"
                        </h3>
                        <span
                            class="text-micro text-[var(--tc-faint)] tabular-nums ml-auto"
                        >
                            (window)
                        </span>
                    </div>
                    (actors)
                </section>
            </div>
        </div>
    }.boxed()
}

fn distribution_card<'a>(cx: &'a Cx, title: &'static str, body: BoxView<'a>) -> BoxView<'a> {
    view! {
        cx =>
        <section
            class="native-insights__card rounded-xl bg-[var(--tc-surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] p-4 flex flex-col"
        >
            <h3
                class="text-micro font-semibold uppercase tracking-widest text-[var(--tc-faint)] mt-0 mb-3 leading-[1.2]"
            >
                (title)
            </h3>
            (body)
        </section>
    }.boxed()
}

fn distribution<'a>(
    cx: &'a Cx,
    items: Vec<(String, i64, Option<BoxView<'a>>)>,
    empty: &'static str,
) -> BoxView<'a> {
    let max = items
        .iter()
        .map(|(_, count, _)| *count)
        .max()
        .unwrap_or(1)
        .max(1);
    let rows = items
        .into_iter()
        .map(|(label, count, icon)| {
            let width = if count > 0 {
                (count as f64 / max as f64 * 100.0).max(3.0)
            } else {
                0.0
            };
            (label, count, icon, format!("width:{width}%"))
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        if rows.is_empty() {
            <p
                class="native-insights__list-empty text-body-sm text-[var(--tc-faint)] py-2 m-0"
            >
                (empty)
            </p>
        } else {
            <div class="native-insights__distribution flex flex-col gap-2">
                for (label, count, icon, width) in rows {
                    <div
                        class="native-insights__distribution-row flex items-center gap-2.5"
                    >
                        if let Some(icon) = icon {
                            <span
                                class="native-insights__distribution-icon shrink-0 w-3.5 flex items-center justify-center"
                            >
                                (icon)
                            </span>
                        }
                        <span
                            class="native-insights__distribution-label text-body-sm text-[var(--tc-muted)] w-[92px] truncate shrink-0"
                            title=(label.clone())
                        >
                            (label)
                        </span>
                        <div
                            class="native-insights__bar flex-1 h-2 rounded-full bg-[var(--tc-bg-subtle)] overflow-hidden min-w-0"
                        >
                            <div
                                class="h-full rounded-full transition-[width] duration-300 bg-[var(--tc-accent)]"
                                style=(width)
                            ></div>
                        </div>
                        <span
                            class="native-insights__count text-caption tabular-nums text-[var(--tc-faint)] w-7 text-right shrink-0"
                        >
                            (count)
                        </span>
                    </div>
                }
            </div>
        }
    }.boxed()
}

fn actor_list<'a>(cx: &'a Cx, actors: &[ActorStat], clock: Signal<f64>) -> BoxView<'a> {
    let max = actors
        .iter()
        .map(|actor| actor.actions)
        .max()
        .unwrap_or(1)
        .max(1);
    let rows = actors.iter().map(|actor| {
        let name = avatar::display_name(actor.display_name.as_deref(), actor.username.as_deref(), "system").to_owned();
        let initials = avatar::initials(&name);
        let class = if actor.is_bot { "native-insights__avatar native-insights__avatar--bot size-6 rounded-full flex items-center justify-center text-micro font-bold shrink-0 select-none bg-[var(--tc-accent-subtle)] text-[var(--tc-accent)] border border-solid border-[var(--tc-accent)]" } else { "native-insights__avatar size-6 rounded-full flex items-center justify-center text-micro font-bold shrink-0 select-none bg-[var(--tc-accent)] text-[var(--tc-accent-text)]" };
        let volume = format!("width:calc({}% - 0.5rem)",(actor.actions as f64/max as f64*100.0).max(4.0));
        (name,initials,class,actor.is_bot,super::super::numbers::count(cx,actor.actions),dates::relative_time_view(cx,&actor.last_ts,clock.clone()),volume)
    }).collect::<Vec<_>>();
    view! {
        cx =>
        if rows.is_empty() {
            <p
                class="native-insights__list-empty text-body-sm text-[var(--tc-faint)] py-2 m-0"
            >
                "No activity in this window"
            </p>
        } else {
            <div class="native-insights__actor-list flex flex-col gap-1">
                for (name, initials, class, bot, actions, last_seen, volume) in rows {
                    <div
                        class="native-insights__actor relative flex items-center gap-2.5 px-1 py-1.5 rounded-md overflow-hidden"
                    >
                        <span class=(class)>(initials)</span>
                        <div class="native-insights__actor-detail flex-1 min-w-0">
                            <div
                                class="native-insights__actor-name flex items-center gap-1.5"
                            >
                                <span
                                    class="text-body-sm text-[var(--tc-text)] truncate font-medium"
                                >
                                    (name)
                                </span>
                                if bot {
                                    <span
                                        class="native-insights__agent text-micro font-semibold uppercase tracking-wider px-1 py-px rounded bg-[var(--tc-accent-subtle)] text-[var(--tc-accent)] shrink-0"
                                    >
                                        "agent"
                                    </span>
                                }
                            </div>
                            <div
                                class="native-insights__last-seen text-micro text-[var(--tc-faint)]"
                            >
                                "last seen "
                                (last_seen)
                            </div>
                        </div>
                        <span
                            class="native-insights__actions text-caption text-[var(--tc-muted)] tabular-nums shrink-0"
                        >
                            (actions)
                        </span>
                        <span
                            class="native-insights__actor-volume absolute bottom-0 left-1 h-[2px] rounded-full bg-[var(--tc-accent)] opacity-30"
                            aria-hidden="true"
                            style=(volume)
                        ></span>
                    </div>
                }
            </div>
        }
    }.boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{IssueStatusCounts, ModuleCount, PriorityCounts};

    #[topcoat::view::component]
    async fn insights_fixture(
        cx: &Cx,
        data: InsightsPayload,
    ) -> topcoat::Result<impl topcoat::view::View> {
        Ok(content(cx, &data))
    }

    fn payload(total: i64) -> InsightsPayload {
        InsightsPayload {
            weeks: 12,
            created_per_week: Vec::new(),
            closed_per_week: Vec::new(),
            status_counts: IssueStatusCounts {
                total,
                ..Default::default()
            },
            priority_counts: PriorityCounts::default(),
            module_counts: Vec::new(),
            top_actors: Vec::new(),
        }
    }

    #[tokio::test]
    async fn zero_issues_show_original_sleeping_mascot_and_copy() {
        let cx = Cx::default();
        let html = view! { cx => insights_fixture(data: payload(0)) }
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(html.contains("Nothing to chart yet"));
        assert!(html.contains("/__native_home/mascot.png"));
        assert!(html.contains("Insights fills in once this project has issues to measure"));
        assert!(!html.contains("Created vs. closed"));
    }

    #[tokio::test]
    async fn populated_distribution_order_and_module_cap_match_main() {
        let cx = Cx::default();
        let mut data = payload(5);
        data.module_counts = (0..8)
            .map(|index| ModuleCount {
                module_id: Some(index),
                name: format!("Module {index}"),
                count: 8 - index,
            })
            .collect();
        let html = view! { cx => insights_fixture(data: data) }
            .single()
            .await
            .unwrap()
            .render(&cx);
        let document = scraper::Html::parse_document(&html);
        let labels = document
            .select(&scraper::Selector::parse(".native-insights__distribution-label").unwrap())
            .map(|node| node.text().collect::<String>())
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            [
                "Backlog",
                "Todo",
                "Active",
                "Done",
                "Cancelled",
                "Urgent",
                "High",
                "Medium",
                "Low",
                "None",
                "Module 0",
                "Module 1",
                "Module 2",
                "Module 3",
                "Module 4",
                "Module 5"
            ]
        );
        assert!(html.contains("+2 more"));
        assert!(html.contains("No activity in this window"));
        assert!(!html.contains("Module 6"));
    }

    #[tokio::test]
    async fn actor_names_badges_initials_and_action_counts_match_main() {
        let cx = Cx::default();
        let mut data = payload(1);
        data.top_actors = vec![
            ActorStat {
                actor_user_id: Some(1),
                display_name: Some("Mary  Jane".into()),
                username: Some("ignored".into()),
                is_bot: true,
                actions: 1234,
                last_ts: "2026-01-02 03:04:05".into(),
                top_transport: "mcp".into(),
            },
            ActorStat {
                actor_user_id: Some(2),
                display_name: Some(String::new()),
                username: Some("some_user".into()),
                is_bot: false,
                actions: 2,
                last_ts: "2026-01-02 03:04:05".into(),
                top_transport: "web".into(),
            },
            ActorStat {
                actor_user_id: None,
                display_name: None,
                username: None,
                is_bot: false,
                actions: 1,
                last_ts: "2026-01-02 03:04:05".into(),
                top_transport: "web".into(),
            },
        ];
        let html = view! { cx => insights_fixture(data: data) }
            .single()
            .await
            .unwrap()
            .render(&cx);
        let document = scraper::Html::parse_document(&html);
        let text = |selector: &str| {
            document
                .select(&scraper::Selector::parse(selector).unwrap())
                .map(|node| node.text().collect::<String>())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            text(".native-insights__actor-name > span:first-child"),
            ["Mary  Jane", "some_user", "system"]
        );
        assert_eq!(text(".native-insights__avatar"), ["MJ", "SU", "S"]);
        assert_eq!(text(".native-insights__agent"), ["agent"]);
        assert_eq!(text(".native-insights__actions"), ["1,234", "2", "1"]);
        assert_eq!(
            document
                .select(
                    &scraper::Selector::parse(".native-insights__last-seen time[datetime]")
                        .unwrap()
                )
                .count(),
            3
        );
    }
}
