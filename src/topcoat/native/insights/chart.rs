//! Main's weekly trend geometry with Rust-owned browser hover state.

use crate::db::models::WeekPoint;
use chrono::NaiveDate;
use topcoat::{
    context::Cx,
    runtime::{Event, signal},
    view::{BoxView, ViewExt, view},
};

const WIDTH: f64 = 680.0;
const BASELINE: f64 = 198.0;
const PLOT_WIDTH: f64 = 644.0;
const PLOT_HEIGHT: f64 = 184.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

fn nice_ticks(maximum: f64) -> Vec<i64> {
    let maximum = if maximum <= 0.0 {
        4.0
    } else {
        let base = 10.0_f64.powf(maximum.log10().floor());
        let normalized = maximum / base;
        base * if normalized <= 1.0 {
            1.0
        } else if normalized <= 2.0 {
            2.0
        } else if normalized <= 5.0 {
            5.0
        } else {
            10.0
        }
    };
    if maximum <= 5.0 {
        return (0..=maximum as i64).collect();
    }
    (0..=4)
        .map(|index| (maximum / 4.0 * f64::from(index)).round() as i64)
        .collect()
}

fn x_at(index: usize, count: usize) -> f64 {
    if count <= 1 {
        28.0 + PLOT_WIDTH / 2.0
    } else {
        28.0 + PLOT_WIDTH * index as f64 / (count - 1) as f64
    }
}
fn y_at(value: i64, axis_max: i64) -> f64 {
    BASELINE - value as f64 / axis_max as f64 * PLOT_HEIGHT
}
fn label_indices(count: usize) -> Vec<usize> {
    if count <= 1 {
        return vec![0];
    }
    if count <= 6 {
        return (0..count).collect();
    }
    let last = count - 1;
    let first = last / 3 + usize::from(last % 3 == 2);
    vec![0, first, last - first, last]
}
fn week_label(week: &str) -> String {
    NaiveDate::parse_from_str(week, "%Y-%m-%d")
        .map(|date| date.format("%b %-d").to_string())
        .unwrap_or_default()
}
fn smooth_path(points: &[Point]) -> String {
    let Some(first) = points.first() else {
        return String::new();
    };
    let mut path = format!("M {} {}", first.x, first.y);
    for (index, pair) in points.windows(2).enumerate() {
        let p0 = points[index.saturating_sub(1)];
        let p1 = pair[0];
        let p2 = pair[1];
        let p3 = points.get(index + 2).copied().unwrap_or(p2);
        path.push_str(&format!(
            " C {} {}, {} {}, {} {}",
            p1.x + (p2.x - p0.x) / 6.0,
            p1.y + (p2.y - p0.y) / 6.0,
            p2.x - (p3.x - p1.x) / 6.0,
            p2.y - (p3.y - p1.y) / 6.0,
            p2.x,
            p2.y
        ));
    }
    path
}
fn smooth_area_path(points: &[Point]) -> String {
    let Some(first) = points.first() else {
        return String::new();
    };
    if points.len() == 1 {
        return format!(
            "M {} {BASELINE} L {} {} L {} {BASELINE} Z",
            first.x, first.x, first.y, first.x
        );
    }
    let last = points.last().unwrap_or(first);
    format!(
        "{} L {} {BASELINE} L {} {BASELINE} Z",
        smooth_path(points),
        last.x,
        first.x
    )
}

pub(super) fn chart<'a>(cx: &'a Cx, created: &[WeekPoint], closed: &[WeekPoint]) -> BoxView<'a> {
    let count = created.len();
    let maximum = created
        .iter()
        .chain(closed)
        .map(|point| point.count)
        .max()
        .unwrap_or(0)
        .max(0);
    let ticks = nice_ticks(maximum as f64);
    let axis_max = ticks.last().copied().unwrap_or(1).max(1);
    let points = |values: &[WeekPoint]| {
        values
            .iter()
            .enumerate()
            .map(|(index, point)| Point {
                x: x_at(index, count),
                y: y_at(point.count, axis_max),
            })
            .collect::<Vec<_>>()
    };
    let created_points = points(created);
    let closed_points = points(closed);
    let created_line = smooth_path(&created_points);
    let closed_line = smooth_path(&closed_points);
    let created_area = smooth_area_path(&created_points);
    let closed_area = smooth_area_path(&closed_points);
    let labels = label_indices(count)
        .into_iter()
        .map(|index| {
            (
                x_at(index, count),
                created
                    .get(index)
                    .map(|point| week_label(&point.week_start))
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    let hover = signal(cx, || None::<usize>);
    let columns = created
        .iter()
        .enumerate()
        .map(|(index, point)| {
            (
                index,
                x_at(index, count),
                week_label(&point.week_start),
                point.count,
                closed.get(index).map_or(0, |point| point.count),
            )
        })
        .collect::<Vec<_>>();
    let accessible_rows = columns.clone();
    view! {
        cx =>
        <div
            class="native-insights-chart relative select-none"
            data-native-insights-chart=""
        >
            <svg
                class="w-full h-auto block"
                viewBox="0 0 680 220"
                role="img"
                aria-label="Issues created vs closed per week"
            >
                for tick in ticks {
                    <line
                        x1="28"
                        x2="672"
                        y1=(y_at(tick, axis_max).to_string())
                        y2=(y_at(tick, axis_max).to_string())
                        stroke="var(--border)"
                        stroke-width="1"
                        stroke-dasharray=(if tick == 0 { "" } else { "2 3" })
                    ></line>
                    <text
                        x="22"
                        y=((y_at(tick, axis_max) + 3.0).to_string())
                        text-anchor="end"
                        fill="var(--text-faint)"
                        font-size="9"
                    >
                        (tick.to_string())
                    </text>
                }
                for (x, label) in labels {
                    <text
                        x=(x.to_string())
                        y="214"
                        text-anchor="middle"
                        fill="var(--text-faint)"
                        font-size="9"
                    >
                        (label)
                    </text>
                }
                if maximum > 0 {
                    <path d=(created_area) fill="var(--accent)" opacity="0.10"></path>
                    <path d=(closed_area) fill="var(--success)" opacity="0.10"></path>
                    <path
                        d=(created_line)
                        fill="none"
                        stroke="var(--accent)"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    ></path>
                    <path
                        d=(closed_line)
                        fill="none"
                        stroke="var(--success)"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    ></path>
                    for index in 0..count {
                        <line
                            class="[&[hidden]]:hidden"
                            data-trend-cursor=(index.to_string())
                            x1=(x_at(index, count).to_string())
                            x2=(x_at(index, count).to_string())
                            y1="14"
                            y2="198"
                            stroke="var(--text-faint)"
                            stroke-width="1"
                            stroke-dasharray="2 3"
                            :hidden=$(if hover.get().is_none() {
                                true
                            } else {
                                hover.get().unwrap() != index
                            })
                        ></line>
                    }
                    for (index, point) in created_points.into_iter().enumerate() {
                        <circle
                            cx=(point.x.to_string())
                            cy=(point.y.to_string())
                            :r=$(if hover.get().is_none() {
                                2.0
                            } else {
                                if hover.get().unwrap() == index { 3.5 } else { 2.0 }
                            })
                            fill="var(--accent)"
                        ></circle>
                    }
                    for (index, point) in closed_points.into_iter().enumerate() {
                        <circle
                            cx=(point.x.to_string())
                            cy=(point.y.to_string())
                            :r=$(if hover.get().is_none() {
                                2.0
                            } else {
                                if hover.get().unwrap() == index { 3.5 } else { 2.0 }
                            })
                            fill="var(--success)"
                        ></circle>
                    }
                }
            </svg>
            if maximum > 0 {
                <div
                    class="native-insights-chart__overlay absolute inset-0 flex"
                    style=(format!(
                        "left:{}%;right:{}%",
                        28.0 / WIDTH * 100.0,
                        8.0 / WIDTH * 100.0,
                    ))
                >
                    for index in 0..count {
                        <div
                            class="native-insights-chart__column flex-1 h-full cursor-default"
                            role="presentation"
                            data-trend-week=(index.to_string())
                            @mouseenter=$(|_event: Event| hover.set(
                                    raw!(r#"cx.some(${index})"#, Some(index)),
                                ))
                            @mouseleave=$(|_event: Event| hover.set(
                                    raw!(r#"cx.hydrate({t:"Option",v:null})"#, None::<usize>),
                                ))
                        ></div>
                    }
                </div>
                for (index, x, label, created_count, closed_count) in columns {
                    <div
                        class="native-insights-chart__tooltip absolute top-[2px] z-10 pointer-events-none px-2.5 py-1.5 rounded-md bg-[var(--surface)] border border-solid border-[var(--border)] shadow-[0_4px_12px_rgba(0,0,0,0.18)] whitespace-nowrap -translate-x-1/2 [&[hidden]]:hidden"
                        data-trend-tooltip=(index.to_string())
                        style=(format!("left:{}%", (x / WIDTH * 100.0).clamp(8.0, 92.0)))
                        :hidden=$(if hover.get().is_none() {
                            true
                        } else {
                            hover.get().unwrap() != index
                        })
                    >
                        <p
                            class="text-caption font-medium text-[var(--text)] mt-0 mb-0.5"
                        >
                            (label)
                        </p>
                        <p class="text-micro text-[var(--accent)] m-0">
                            "Created "
                            <span class="tabular-nums font-semibold">
                                (created_count.to_string())
                            </span>
                        </p>
                        <p class="text-micro text-[var(--success)] m-0">
                            "Closed "
                            <span class="tabular-nums font-semibold">
                                (closed_count.to_string())
                            </span>
                        </p>
                    </div>
                }
            }
        </div>
        <table class="native-insights-chart__sr-only sr-only">
            <caption>"Issues created vs closed per week"</caption>
            <thead>
                <tr>
                    <th scope="col">"Week"</th>
                    <th scope="col">"Created"</th>
                    <th scope="col">"Closed"</th>
                </tr>
            </thead>
            <tbody>
                for (_, _, label, created_count, closed_count) in accessible_rows {
                    <tr>
                        <th scope="row">(label)</th>
                        <td>(created_count.to_string())</td>
                        <td>(closed_count.to_string())</td>
                    </tr>
                }
            </tbody>
        </table>
        <div class="native-insights-chart__legend flex items-center gap-4 mt-1 px-1">
            <span
                class="flex items-center gap-1.5 text-caption text-[var(--text-muted)]"
            >
                <span
                    class="native-insights-chart__legend-dot size-2 rounded-full shrink-0 bg-[var(--accent)]"
                ></span>
                "Created"
            </span>
            <span
                class="flex items-center gap-1.5 text-caption text-[var(--text-muted)]"
            >
                <span
                    class="native-insights-chart__legend-dot size-2 rounded-full shrink-0 bg-[var(--success)]"
                ></span>
                "Closed"
            </span>
        </div>
    }.boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn week_axis_labels_match_main_windows() {
        for (count, expected) in [
            (1, vec![0]),
            (4, vec![0, 1, 2, 3]),
            (6, vec![0, 1, 2, 3, 4, 5]),
            (12, vec![0, 4, 7, 11]),
            (26, vec![0, 8, 17, 25]),
            (52, vec![0, 17, 34, 51]),
        ] {
            assert_eq!(label_indices(count), expected, "{count} week labels");
        }
    }

    #[test]
    fn axis_ticks_match_main_zero_small_and_large_domains() {
        assert_eq!(nice_ticks(0.0), vec![0, 1, 2, 3, 4]);
        assert_eq!(nice_ticks(3.0), vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(nice_ticks(11.0), vec![0, 5, 10, 15, 20]);
    }
}
